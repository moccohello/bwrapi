//! Publication round trips: the game side writes frames into a region, and
//! an engine reading the publication gets the same tables back.

use std::sync::atomic::Ordering::Acquire;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use bwrapi_layout::Layout;

use crate::columns::Columns;
use crate::decode::bwgame::BWGAME_FRAME_COUNT_OFFSET;
use crate::decode::map::BWGAME_MAP_WIDTH_OFFSET;
use crate::fake::{in_game, put_unit, FakeMemory, Shared, ROOT};
use crate::publish::{Control, Publication, Region, Writer, READERS};
use crate::tables::*;
use crate::{CommandBasis, Engine, Error, Frame};

const TILES: usize = 0x4_0000_0000;
const FRAME_SOURCES: u64 = (1 << FRAME_TABLES) - 1;

/// Shared memory of one game: the control block, every data region made so
/// far, and per-reader wake-up flags that behave like auto-reset events.
#[derive(Default)]
struct SharedMemory {
    control: Control,
    regions: Mutex<Vec<(u32, Region)>>,
    signals: Mutex<[bool; READERS]>,
    signalled: Condvar,
}

struct TestPublication {
    memory: Arc<SharedMemory>,
    slot: usize,
}

impl TestPublication {
    fn attach(memory: &Arc<SharedMemory>) -> Box<Self> {
        let slot = memory.control.attach(1).expect("a free reader slot");
        Box::new(Self { memory: Arc::clone(memory), slot })
    }
}

impl Publication for TestPublication {
    fn control(&self) -> &Control {
        &self.memory.control
    }

    fn data(&mut self, generation: u32) -> Option<Region> {
        let regions = self.memory.regions.lock().unwrap();
        regions.iter().find(|(made, _)| *made == generation).map(|(_, region)| *region)
    }

    fn wait(&self, timeout: Duration) {
        let signals = self.memory.signals.lock().unwrap();
        let (mut signals, _) =
            self.memory.signalled.wait_timeout_while(signals, timeout, |signals| !signals[self.slot]).unwrap();
        signals[self.slot] = false;
    }

    fn slot(&self) -> usize {
        self.slot
    }
}

impl Drop for TestPublication {
    fn drop(&mut self) {
        self.memory.control.detach(self.slot);
    }
}

/// The game thread's side: one writer, regions made on demand.
struct Game {
    memory: Arc<SharedMemory>,
    writer: Writer,
}

impl Game {
    fn new() -> Self {
        Self { memory: Arc::default(), writer: Writer::default() }
    }

    fn publish(&mut self, frame: Option<&Frame>, basis: Option<&CommandBasis>) {
        let memory = Arc::clone(&self.memory);
        let requested = memory.control.readers().unwrap_or(0);
        let mut map = |generation: u32, len: usize| {
            let words = Box::leak(vec![0u64; len.div_ceil(8)].into_boxed_slice());
            let region = unsafe { Region::new(words.as_mut_ptr().cast(), len) };
            memory.regions.lock().unwrap().push((generation, region));
            Some(region)
        };
        assert!(unsafe { self.writer.publish(&memory.control, frame, requested, requested, basis, &mut map) });
        *memory.signals.lock().unwrap() = [true; READERS];
        memory.signalled.notify_all();
    }
}

fn reader(game: &Game, sources: u64) -> Engine {
    let mut engine = Engine::with_memory(Box::new(Shared(Arc::new(FakeMemory::new())))).unwrap();
    engine.subscribe(sources);
    engine.read_published(TestPublication::attach(&game.memory));
    engine
}

fn column_bytes(table: &dyn Columns, columns: &[crate::ColumnInfo], column: usize) -> Vec<u8> {
    let bytes = table.rows() * columns[column].kind.bytes();
    unsafe { std::slice::from_raw_parts(table.column_ptr(column), bytes) }.to_vec()
}

/// Every carried table of `read` equals `game`'s, column by column.
fn assert_same_tables(game: &Frame, read: &Frame) {
    assert_eq!(read.sources, game.sources);
    for (index, info) in TABLES.iter().enumerate().filter(|(index, _)| game.sources & 1 << index != 0) {
        let (game_table, read_table) = (game.table(index).unwrap(), read.table(index).unwrap());
        assert_eq!(read_table.rows(), game_table.rows(), "{}", info.name);
        for column in 0..info.columns.len() {
            assert_eq!(
                column_bytes(read_table, info.columns, column),
                column_bytes(game_table, info.columns, column),
                "{}.{}",
                info.name,
                info.columns[column].name
            );
        }
    }
}

fn game_memory(units: usize, slots: usize, width: u16, height: u16) -> Arc<FakeMemory> {
    let memory = in_game(slots);
    for slot in 0..units {
        put_unit(&memory, slot * slots / units, (slot % 200) as u16, (slot % 8) as u8, slot as i16, -(slot as i16), 1);
    }
    memory.patch(ROOT + BWGAME_FRAME_COUNT_OFFSET, &77u32.to_le_bytes());
    let mut size = width.to_le_bytes().to_vec();
    size.extend(height.to_le_bytes());
    memory.patch(ROOT + BWGAME_MAP_WIDTH_OFFSET, &size);
    memory.put_module(Layout::X64.map.active_tile_pointer_module_offset, &(TILES as u64).to_le_bytes());
    let tiles: Vec<u8> = (0..usize::from(width) * usize::from(height) * 4).map(|at| (at * 7) as u8).collect();
    memory.put(TILES, &tiles);
    memory
}

#[test]
fn an_outside_reader_rebuilds_the_tables_the_game_engine_read() {
    let memory = game_memory(1200, 1700, 128, 96);
    let mut engine = Engine::with_memory(Box::new(Shared(Arc::clone(&memory)))).unwrap();
    engine.subscribe(FRAME_SOURCES);
    let mut game = Game::new();
    let mut outside = reader(&game, FRAME_SOURCES);

    let frame = engine.acquire().unwrap();
    assert_eq!(frame.units.rows(), 1200);
    assert_eq!(frame.vision.rows(), 128 * 96);
    let basis = engine.command_basis();
    game.publish(Some(&frame), basis.as_ref());

    let read = outside.acquire().unwrap();
    assert_same_tables(&frame, &read);
    assert_eq!(read.units, frame.units);
    assert_eq!(read.vision, frame.vision);
    assert_eq!(
        (read.match_id, read.game_frame, read.flags, read.local_player, read.map_width, read.map_height),
        (frame.match_id, frame.game_frame, frame.flags, frame.local_player, frame.map_width, frame.map_height)
    );
    assert_eq!(outside.command_basis(), basis);
    assert!(basis.is_some());
}

#[test]
fn a_reader_gets_only_its_own_sources_of_the_union() {
    let memory = game_memory(10, 16, 4, 4);
    let mut engine = Engine::with_memory(Box::new(Shared(Arc::clone(&memory)))).unwrap();
    let mut game = Game::new();
    let mut units = reader(&game, source::UNITS);
    let mut vision = reader(&game, source::VISION);
    engine.subscribe(game.memory.control.readers().unwrap());
    assert_eq!(game.memory.control.readers(), Some(source::UNITS | source::VISION));
    let frame = engine.acquire().unwrap();
    game.publish(Some(&frame), None);

    let read = units.acquire().unwrap();
    assert_eq!((read.sources, read.units.rows(), read.vision.rows()), (source::UNITS, 10, 0));
    let read = vision.acquire().unwrap();
    assert_eq!((read.sources, read.units.rows(), read.vision.rows()), (source::VISION, 0, 16));

    drop(units);
    drop(vision);
    assert_eq!(game.memory.control.readers(), None);
}

#[test]
fn a_reader_takes_no_publication_older_than_its_subscription() {
    let mut game = Game::new();
    let earlier = reader(&game, source::UNITS);
    game.publish(Some(&synthetic_frame(1, 100, 3, 0)), None);
    drop(earlier);

    // The game published nothing while no reader was attached.
    let mut outside = reader(&game, source::UNITS);
    assert_eq!(outside.step(Duration::from_millis(5)), Err(Error::Timeout));
    assert_eq!(outside.acquire().err(), Some(Error::NotInGame));
    game.publish(Some(&synthetic_frame(1, 5000, 3, 0)), None);
    assert_eq!(outside.step(Duration::from_secs(5)), Ok(()));
    assert_eq!(outside.acquire().unwrap().game_frame, 5000);
}

#[test]
fn a_game_closed_at_the_menu_is_gone() {
    let memory = Arc::new(FakeMemory::new());
    let mut outside = Engine::with_memory(Box::new(Shared(Arc::clone(&memory)))).unwrap();
    let mut game = Game::new();
    outside.read_published(TestPublication::attach(&game.memory));
    game.publish(None, None);
    assert_eq!(outside.step(Duration::from_millis(5)), Err(Error::NotInGame));
    assert_eq!(outside.acquire().err(), Some(Error::NotInGame));

    memory.kill();
    assert_eq!(outside.step(Duration::from_millis(5)), Err(Error::Gone));
    assert_eq!(outside.acquire().err(), Some(Error::Gone));
}

fn synthetic_frame(match_id: u32, game_frame: u32, units: usize, tiles: usize) -> Frame {
    let mut frame = Frame { match_id, game_frame, sources: source::UNITS, ..Frame::default() };
    for index in 0..units {
        frame.units.push(UnitRow { id: index as u64 + 1, x: game_frame as i16, y: index as i16, ..UnitRow::UNKNOWN });
    }
    if tiles != 0 {
        frame.sources |= source::TERRAIN | source::WALKABILITY;
        let mut terrain = Terrain::default();
        let mut walkability = Walkability::default();
        for tile in 0..tiles {
            terrain.push(TerrainRow { buildable: 1, ground_height: (tile % 6) as u8, region: match_id as u16, ..TerrainRow::UNKNOWN });
            walkability.push(WalkabilityRow { walkable: (tile % 2) as u8 });
        }
        frame.shared.terrain = Arc::new(terrain);
        frame.shared.walkability = Arc::new(walkability);
    }
    frame
}

#[test]
fn match_tables_are_copied_once_per_match() {
    let mut game = Game::new();
    let mut outside = reader(&game, source::UNITS | source::TERRAIN | source::WALKABILITY);

    let first = synthetic_frame(1, 10, 5, 64);
    game.publish(Some(&first), None);
    let a = outside.acquire().unwrap();
    assert_same_tables(&first, &a);
    let second = synthetic_frame(1, 11, 6, 64);
    game.publish(Some(&second), None);
    let b = outside.acquire().unwrap();
    assert_same_tables(&second, &b);
    assert!(Arc::ptr_eq(&a.shared.terrain, &b.shared.terrain), "same match: no new copy");

    let next_match = synthetic_frame(2, 0, 3, 100);
    game.publish(Some(&next_match), None);
    let c = outside.acquire().unwrap();
    assert_same_tables(&next_match, &c);
    assert_eq!(c.match_id, 2);
    // Frames of the old match keep their tables.
    assert_eq!(a.shared.terrain.rows(), 64);
}

#[test]
fn a_frame_that_outgrows_the_mapping_moves_to_a_new_generation() {
    let mut game = Game::new();
    let mut outside = reader(&game, source::UNITS);
    game.publish(Some(&synthetic_frame(1, 1, 10, 0)), None);
    let generation = game.memory.control.generation.load(Acquire);
    assert_eq!(outside.acquire().unwrap().units.rows(), 10);

    let large = synthetic_frame(1, 2, 3400, 0);
    game.publish(Some(&large), None);
    assert_eq!(game.memory.control.generation.load(Acquire), generation + 1);
    assert_same_tables(&large, &outside.acquire().unwrap());
}

#[test]
fn step_waits_for_the_next_publication_and_reports_the_menu() {
    let mut game = Game::new();
    let mut outside = reader(&game, source::UNITS);
    game.publish(None, None);
    assert_eq!(outside.acquire().err(), Some(Error::NotInGame));
    assert_eq!(outside.step(Duration::from_millis(5)), Err(Error::NotInGame));
    assert_eq!(outside.command_basis(), None);

    game.publish(Some(&synthetic_frame(1, 10, 2, 0)), None);
    assert_eq!(outside.step(Duration::from_millis(5)), Ok(()));
    assert_eq!(outside.acquire().unwrap().game_frame, 10);
    assert_eq!(outside.step(Duration::from_millis(5)), Err(Error::Timeout));

    let publisher = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(20));
        game.publish(Some(&synthetic_frame(1, 11, 2, 0)), None);
    });
    assert_eq!(outside.step(Duration::from_secs(5)), Ok(()));
    publisher.join().unwrap();
    assert_eq!(outside.acquire().unwrap().game_frame, 11);
}

#[test]
fn acquire_after_a_subscription_waits_for_the_game_to_publish_it() {
    let mut game = Game::new();
    let mut outside = reader(&game, source::UNITS);
    game.publish(Some(&synthetic_frame(1, 10, 2, 0)), None);
    outside.subscribe(source::UNITS | source::TERRAIN);

    let control = Arc::clone(&game.memory);
    let publisher = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(20));
        assert_eq!(control.control.readers(), Some(source::UNITS | source::TERRAIN));
        game.publish(Some(&synthetic_frame(1, 10, 2, 9)), None);
    });
    let started = Instant::now();
    let frame = outside.acquire().unwrap();
    assert!(started.elapsed() < Duration::from_millis(400));
    assert_eq!(frame.sources, source::UNITS | source::TERRAIN);
    assert_eq!(frame.shared.terrain.rows(), 9);
    publisher.join().unwrap();
}

#[test]
fn a_reader_never_sees_a_torn_frame() {
    let mut game = Game::new();
    let mut outside = reader(&game, source::UNITS);
    game.publish(Some(&synthetic_frame(1, 0, 1000, 0)), None);
    let frames = 20_000u32;
    let publisher = std::thread::spawn(move || {
        let mut frame = synthetic_frame(1, 0, 1000, 0);
        for game_frame in 1..=frames {
            // Rows and values both follow the frame number, so a copy that
            // mixes two publications shows in either.
            frame.game_frame = game_frame;
            crate::columns::Table::clear(&mut frame.units);
            for index in 0..1000 + (game_frame % 97) as usize {
                frame.units.push(UnitRow { id: index as u64 + 1, x: game_frame as i16, ..UnitRow::UNKNOWN });
            }
            game.publish(Some(&frame), None);
        }
    });
    let mut seen = 0;
    let mut last = 0;
    while last < frames {
        let frame = outside.acquire().unwrap();
        assert!(frame.game_frame >= last);
        last = frame.game_frame;
        assert_eq!(frame.units.rows(), 1000 + (last % 97) as usize, "frame {last}");
        assert!(frame.units.x.iter().all(|x| *x == last as i16), "frame {last}");
        seen += 1;
    }
    publisher.join().unwrap();
    assert!(seen > 1);
}

/// Row counts of a large late-game match: a full standard unit pool's worth
/// of units, their orders and sprites, a 128x128 map.
fn late_game_frame() -> Frame {
    let mut frame = synthetic_frame(1, 1, 1600, 0);
    frame.sources = FRAME_SOURCES;
    for index in 0..12 {
        frame.players.push(PlayerRow { id: index, ..PlayerRow::UNKNOWN });
    }
    for index in 0..800u64 {
        frame.orders.push(OrderRow { unit: index + 1, ..OrderRow::UNKNOWN });
    }
    for index in 0..1600u64 {
        frame.render.push(RenderRow { unit: index + 1, ..RenderRow::UNKNOWN });
    }
    for tile in 0..128 * 128 {
        frame.vision.push(VisionRow { visible: tile as u16, ..VisionRow::UNKNOWN });
    }
    for index in 0..400u16 {
        frame.unit_counts.push(UnitCountRow { unit_type: index, ..UnitCountRow::UNKNOWN });
    }
    for index in 0..12 * 61u16 {
        frame.upgrades.push(UpgradeRow { upgrade_type: index, ..UpgradeRow::UNKNOWN });
    }
    for index in 0..12 * 44u16 {
        frame.techs.push(TechRow { tech_type: index, ..TechRow::UNKNOWN });
    }
    for index in 0..64u64 {
        frame.bullets.push(BulletRow { id: index, ..BulletRow::UNKNOWN });
    }
    for index in 0..12u64 {
        frame.selection.push(SelectionRow { unit: index + 1 });
    }
    frame.viewport.push(ViewportRow::UNKNOWN);
    frame.game.push(GameRow::UNKNOWN);
    frame
}

/// Publish and acquire costs at late-game row counts. Run with
/// `--release -- --nocapture` for meaningful numbers.
#[test]
fn publish_and_acquire_costs_at_realistic_row_counts() {
    let frame = late_game_frame();
    let mut game = Game::new();
    let mut outside = reader(&game, FRAME_SOURCES);
    game.publish(Some(&frame), None);
    outside.acquire().unwrap();

    let rounds = 300;
    let started = Instant::now();
    for _ in 0..rounds {
        game.publish(Some(&frame), None);
    }
    let publish = started.elapsed() / rounds;
    let started = Instant::now();
    for _ in 0..rounds {
        let read = outside.acquire().unwrap();
        assert_eq!(read.units.rows(), 1600);
    }
    let acquire = started.elapsed() / rounds;
    let regions = game.memory.regions.lock().unwrap();
    eprintln!(
        "publish {publish:?}/frame, acquire {acquire:?}/frame, data mapping {} KiB ({} units, {} vision rows)",
        regions.last().unwrap().1.len() >> 10,
        frame.units.rows(),
        frame.vision.rows()
    );
}
