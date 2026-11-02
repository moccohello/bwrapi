//! Match-long terrain, walkability and region tables, and the vision tile
//! columns that come from the same ActiveTile array.

use std::sync::Arc;

use bwrapi_layout::Layout;

use crate::columns::Table;
use crate::decode::bwgame::BWGAME_ROOT_READ_BYTES;
use crate::decode::map::{
    ACTIVE_TILE_HAS_CREEP_BIT, ACTIVE_TILE_TEMPORARY_CREEP_BIT, BWGAME_MAP_WIDTH_OFFSET,
};
use crate::fake::{in_game, FakeMemory, Shared, MODULE_BASE, ROOT};
use crate::tables::source;
use crate::Engine;

const ACTIVE_TILES: usize = 0x4_0000_0000;
const SAI: usize = 0x5_0000_0000;
const NEIGHBORS: usize = 0x6_0000_0000;
const ROOT2: usize = 0x7_0000_0000;

const MATCH_TABLES: u64 =
    source::TERRAIN | source::WALKABILITY | source::REGIONS | source::REGION_NEIGHBORS;

fn engine(memory: &Arc<FakeMemory>, sources: u64) -> Engine {
    let mut engine = Engine::with_memory(Box::new(Shared(Arc::clone(memory)))).unwrap();
    engine.subscribe(sources);
    engine
}

fn put_u8(memory: &FakeMemory, address: usize, value: u8) {
    memory.patch(address, &[value]);
}

fn put_u16(memory: &FakeMemory, address: usize, value: u16) {
    memory.patch(address, &value.to_le_bytes());
}

fn put_u32(memory: &FakeMemory, address: usize, value: u32) {
    memory.patch(address, &value.to_le_bytes());
}

fn put_u64(memory: &FakeMemory, address: usize, value: u64) {
    memory.patch(address, &value.to_le_bytes());
}

/// A 4x4 map: two SAI regions (0 accessible on higher ground, 1 an
/// inaccessible island), tile (0,0) split between them, and ActiveTile flags
/// for the terrain and vision columns.
fn map_memory() -> Arc<FakeMemory> {
    let memory = in_game(1);
    let map = &Layout::X64.map;
    memory.patch(ROOT + BWGAME_MAP_WIDTH_OFFSET, &[4, 0, 4, 0]);
    memory.put_module(map.active_tile_pointer_module_offset, &(ACTIVE_TILES as u64).to_le_bytes());
    memory.put_module(map.sai_pathing_pointer_module_offset, &(SAI as u64).to_le_bytes());

    let mut cells = vec![0u8; 16 * 4];
    // The flag bytes are the inverted visible/explored masks.
    cells[0] = 0b1111_1110;
    cells[1] = 0b1111_1101;
    cells[3] = 0x02; // ground height 2
    cells[1 * 4 + 2] = ACTIVE_TILE_HAS_CREEP_BIT;
    cells[2 * 4 + 2] = 0x80; // always unbuildable
    cells[3 * 4 + 2] = 0x04; // unwalkable
    cells[4 * 4 + 3] = ACTIVE_TILE_TEMPORARY_CREEP_BIT;
    cells[5 * 4 + 3] = 0x08; // occupied by a building
    memory.put(ACTIVE_TILES, &cells);

    memory.put(SAI, &vec![0u8; map.sai_pathing_allocation_bytes]);
    put_u32(&memory, SAI, 2); // region count
    let region = |id: usize| SAI + map.region_table + id * map.region_stride;
    // Region 0: accessible, higher ground, group 1, one neighbor.
    put_u16(&memory, region(0), 0x1ff9);
    put_u16(&memory, region(0) + 2, 1);
    put_u16(&memory, region(0) + 4, 14);
    put_u8(&memory, region(0) + 7, 1);
    put_u64(&memory, region(0) + 0x10, NEIGHBORS as u64);
    put_u32(&memory, region(0) + 24, 64 << 8);
    put_u32(&memory, region(0) + 28, 48 << 8);
    for (offset, value) in [(32, 0), (34, 0), (36, 128), (38, 64)] {
        put_u16(&memory, region(0) + offset, value);
    }
    put_u8(&memory, region(0) + 0x28, 0x85); // priority 5 after the mask
    // Region 1: inaccessible island, group 2, no neighbors.
    put_u16(&memory, region(1), 0x1ffd);
    put_u16(&memory, region(1) + 2, 2);
    put_u32(&memory, region(1) + 24, 32 << 8);
    put_u32(&memory, region(1) + 28, 16 << 8);
    for (offset, value) in [(32, 32), (34, 16), (36, 64), (38, 48)] {
        put_u16(&memory, region(1) + offset, value);
    }
    // Tile grid: (0,0) is split entry 0; (1,0) is the region 1 island; the
    // rest belong to region 0.
    put_u16(&memory, SAI + map.tile_grid, 0x2000);
    put_u16(&memory, SAI + map.tile_grid + 2, 1);
    put_u16(&memory, SAI + map.split_table, 0x00f0); // minitiles 4..8 go to region2
    put_u16(&memory, SAI + map.split_table + 2, 0);
    put_u16(&memory, SAI + map.split_table + 4, 1);
    // Region 0's neighbor id, with high bits the 13-bit mask must drop.
    memory.put(NEIGHBORS, &0x8001u16.to_le_bytes());
    memory
}

#[test]
fn match_tables_decode_the_synthetic_map() {
    let memory = map_memory();
    let frame = engine(&memory, MATCH_TABLES).acquire().unwrap();

    let terrain = &frame.shared.terrain;
    assert_eq!(terrain.rows(), 16);
    let row = |index: usize| terrain.row(index).unwrap();
    assert_eq!((row(0).buildable, row(0).ground_height, row(0).creep), (1, 2, 0));
    assert_eq!((row(1).buildable, row(1).creep), (1, 1));
    assert_eq!((row(2).buildable, row(3).buildable), (0, 0));
    assert_eq!(row(4).creep, 1);
    assert_eq!((row(9).buildable, row(9).ground_height, row(9).creep), (1, 0, 0));
    // Region columns: the split tile, the island tile and an unsplit tile.
    assert_eq!((row(0).region, row(0).region2, row(0).split_mask), (0, 1, 0x00f0));
    assert_eq!((row(1).region, row(1).region2, row(1).split_mask), (1, 1, 0));
    assert_eq!((row(2).region, row(2).region2, row(2).split_mask), (0, 0, 0));

    let walkability = &frame.shared.walkability;
    assert_eq!(walkability.rows(), 256);
    let at = |x: usize, y: usize| walkability.walkable[y * 16 + x];
    // The split minitiles 4..8 of tile (0,0) belong to the island.
    assert_eq!((at(0, 0), at(3, 0), at(0, 1), at(3, 1), at(0, 2)), (1, 1, 0, 0, 1));
    assert_eq!((at(4, 0), at(7, 3), at(8, 0), at(15, 7)), (0, 0, 1, 1));
    // The engine-border minitiles are not walkable: the last four rows, and
    // the 20-minitile side borders of the last eight rows.
    assert_eq!((at(0, 12), at(15, 15), at(15, 11)), (0, 0, 0));

    let regions = &frame.shared.regions;
    assert_eq!(regions.rows(), 2);
    let first = regions.row(0).unwrap();
    assert_eq!((first.id, first.group, first.center_x, first.center_y), (0, 1, 64, 48));
    assert_eq!((first.left, first.top, first.right, first.bottom), (0, 0, 128, 64));
    assert_eq!((first.accessible, first.higher_ground, first.priority), (1, 1, 5));
    assert_eq!((first.neighbor_start, first.neighbor_count), (0, 1));
    let second = regions.row(1).unwrap();
    assert_eq!((second.id, second.accessible, second.higher_ground), (1, 0, 0));
    assert_eq!((second.neighbor_start, second.neighbor_count), (1, 0));
    assert_eq!(frame.shared.region_neighbors.region, vec![1u16]);
}

#[test]
fn match_tables_are_read_once_per_match_and_again_when_the_root_changes() {
    let memory = map_memory();
    let mut engine = engine(&memory, MATCH_TABLES);
    engine.acquire().unwrap();
    // The SAI allocation is read once plus its stability reread; the neighbor
    // list is read twice the same way.
    let sai_reads = memory.reads_in(SAI, 1);
    assert_eq!(sai_reads, 2);
    assert_eq!(memory.reads_in(ACTIVE_TILES, 1), 1);
    assert_eq!(memory.reads_in(NEIGHBORS, 1), 2);

    engine.acquire().unwrap();
    engine.acquire().unwrap();
    assert_eq!(memory.reads_in(SAI, 1), sai_reads);
    assert_eq!(memory.reads_in(ACTIVE_TILES, 1), 1);

    // A new BWGame root is a new match: the tables are read again.
    let menu = &Layout::X64.menu_root;
    memory.patch(MODULE_BASE + menu.pointer, &((ROOT2 as u64) ^ menu.xor_key).to_le_bytes());
    memory.put(ROOT2, &vec![0; BWGAME_ROOT_READ_BYTES]);
    memory.patch(ROOT2 + BWGAME_MAP_WIDTH_OFFSET, &[4, 0, 4, 0]);
    let frame = engine.acquire().unwrap();
    assert_eq!(frame.match_id, 2);
    assert!(memory.reads_in(SAI, 1) > sai_reads);
    assert_eq!(frame.shared.terrain.rows(), 16);
}

#[test]
fn vision_reads_creep_and_occupied_from_the_active_tile_bits() {
    let memory = map_memory();
    let frame = engine(&memory, source::VISION).acquire().unwrap();
    assert_eq!((frame.map_width, frame.map_height), (4, 4));
    let vision = &frame.vision;
    assert_eq!(vision.rows(), 16);
    assert_eq!((vision.visible[0], vision.explored[0]), (0x01, 0x02));
    // The permanent (byte2) and temporary (byte3) creep bits both count.
    assert_eq!(vision.creep[..6], [0, 1, 0, 0, 1, 0]);
    // Occupied is byte3 bit 0x08.
    assert_eq!(vision.occupied[..6], [0, 0, 0, 0, 0, 1]);
    assert!(vision.creep[6..].iter().all(|value| *value == 0));
    assert!(vision.occupied[6..].iter().all(|value| *value == 0));
}
