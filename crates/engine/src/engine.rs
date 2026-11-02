//! Reads a running game from outside its process. `acquire` reads the
//! subscribed sources with large block reads, decodes them into column
//! tables, and checks the game frame did not advance while reading. When the
//! game runs the in-game DLL, an engine given its publication copies the
//! tables the game thread published instead (see [`crate::publish`]).

use std::sync::Arc;
use std::time::{Duration, Instant};

use bwrapi_layout::{read_pointer, Layout};
use bwrapi_memory::{MemoryError, ProcessMemory, ProcessModule};

use crate::decode::bwgame::BWGAME_FRAME_COUNT_OFFSET;
use crate::publish::{self, Attempt, Publication};
use crate::read;
use crate::tables::*;

/// Frame flags.
pub mod frame_flag {
    /// The game frame advanced during the read twice in a row; rows may mix
    /// two frames.
    pub const TORN: u32 = 2;
    /// The frame is from a replay.
    pub const REPLAY: u32 = 4;
}

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    NotInGame,
    /// `step` saw no new game frame before its timeout.
    Timeout,
    /// The process exited.
    Gone,
    /// The game build has no layout here.
    UnsupportedBuild,
    Memory(MemoryError),
}

impl From<MemoryError> for Error {
    fn from(error: MemoryError) -> Self {
        Self::Memory(error)
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotInGame => f.write_str("not in game"),
            Self::Timeout => f.write_str("no new frame before the timeout"),
            Self::Gone => f.write_str("process exited"),
            Self::UnsupportedBuild => f.write_str("unsupported game build"),
            Self::Memory(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for Error {}

/// One read of the game. Tables of sources not in `sources` are empty.
#[derive(Clone, Debug, Default)]
pub struct Frame {
    pub sequence: u64,
    /// Increases when a new match starts.
    pub match_id: u32,
    /// Game frame number; `u32::MAX` when unknown.
    pub game_frame: u32,
    pub flags: u32,
    /// Local player id; `u8::MAX` when unknown (replays, observers).
    pub local_player: u8,
    pub map_width: u16,
    pub map_height: u16,
    /// Sources this frame carries: the subscribed ones that could be read.
    pub sources: u64,
    pub players: Players,
    pub units: Units,
    pub unit_counts: UnitCounts,
    pub upgrades: Upgrades,
    pub techs: Techs,
    pub bullets: Bullets,
    pub vision: Vision,
    pub viewport: Viewport,
    pub selection: Selection,
    pub orders: Orders,
    pub render: Render,
    pub game: Game,
    pub forces: Forces,
    pub start_locations: StartLocations,
    pub unit_availability: UnitAvailability,
    /// Match-long tables (terrain, walkability, regions), shared between frames.
    pub shared: MatchTables,
}

impl Frame {
    fn reset(&mut self) {
        self.clear_tables();
        self.map_width = 0;
        self.map_height = 0;
    }
}

/// What a source reader may use: the process, its layout, and the BWGame
/// root of this frame.
pub(crate) struct Target<'a> {
    pub memory: &'a dyn ProcessMemory,
    pub layout: &'static Layout,
    pub module: ProcessModule,
    /// BWGame root; 0 when unknown.
    pub root: usize,
}

impl Target<'_> {
    pub fn read(&self, address: usize, buffer: &mut [u8]) -> Result<(), MemoryError> {
        self.memory.read_into(address, buffer)
    }

    /// Module-relative read; fails for RVAs this build does not have.
    pub fn read_module(&self, rva: usize, buffer: &mut [u8]) -> Result<(), MemoryError> {
        match self.module.range_address(rva, buffer.len()) {
            Some(address) => self.read(address, buffer),
            None => Err(MemoryError::InvalidRead { size: buffer.len() }),
        }
    }

    pub fn module_u32(&self, rva: usize) -> Option<u32> {
        let mut raw = [0; 4];
        self.read_module(rva, &mut raw).ok()?;
        Some(u32::from_le_bytes(raw))
    }

    pub fn module_pointer(&self, rva: usize) -> Option<usize> {
        let mut raw = [0; 8];
        let raw = &mut raw[..self.layout.pointer_bytes];
        self.read_module(rva, raw).ok()?;
        read_pointer(raw, 0, self.layout.pointer_bytes)
    }

    pub fn u32_at(&self, address: usize) -> Option<u32> {
        let mut raw = [0; 4];
        self.read(address, &mut raw).ok()?;
        Some(u32::from_le_bytes(raw))
    }
}

/// BWGame player slots the repair and refund totals keep.
pub const RESOURCE_SLOTS: usize = 12;

/// One player's minerals and gas paid for repairs and got back from cancels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ResourceTotals {
    pub repaired_minerals: i32,
    pub repaired_gas: i32,
    pub refunded_minerals: i32,
    pub refunded_gas: i32,
}

/// Buffers and per-match state the source readers keep between frames.
#[derive(Default)]
pub(crate) struct State {
    /// CUnit records at full pool capacity; only the bands the lists reach
    /// are refreshed each frame.
    pub cunits: Vec<u8>,
    /// Base of the CUnit pool read this frame; 0 when not read.
    pub pool_base: usize,
    /// Pool base `cunits` was last read for; a change starts a fresh buffer.
    pub pool_base_of: usize,
    /// Slots the live-unit lists reached this frame (list id + 1, or 1 for
    /// every slot after a whole-pool read).
    pub reached: Vec<u8>,
    pub block: Vec<u8>,
    /// The in-game watcher's latest `(turn-send count, QPC of its change)`;
    /// the game table names the send time only while the count matches what
    /// the read itself sees.
    pub turn_watch: Option<(u32, u64)>,
    /// In-progress research and upgrade entries; reused between frames.
    pub tech_progress: Vec<(u8, u16, u32)>,
    pub upgrade_progress: Vec<(u8, u16, u32)>,
    /// The in-game DLL's repair and refund totals of this match, by BWGame
    /// player slot; `None` without the DLL.
    pub resource_totals: Option<[ResourceTotals; RESOURCE_SLOTS]>,
    /// Match-long tables and the BWGame root they were read for.
    pub shared: MatchTables,
    pub shared_root: usize,
}

/// What a command needs to name units the way the last acquired frame did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommandBasis {
    /// BWGame root of the match.
    pub root: u64,
    /// Address of the first CUnit record.
    pub pool_base: u64,
    /// `u8::MAX` when the game reported none.
    pub local_player: u8,
    pub game_frame: u32,
    pub match_id: u32,
}

pub struct Engine {
    memory: Box<dyn ProcessMemory + Send>,
    layout: &'static Layout,
    module: ProcessModule,
    sources: u64,
    sequence: u64,
    match_id: u32,
    last_root: usize,
    /// Game frame of the last acquired frame.
    last_game_frame: u32,
    last_local_player: u8,
    state: State,
    /// Frames handed out; one whose only owner is the engine is reused.
    frames: Vec<Arc<Frame>>,
    /// Whether reads must confirm the frame did not advance under them.
    /// Outside-the-process readers need it; the game thread at the frame
    /// boundary does not.
    verify_tearing: bool,
    /// Frames the in-game DLL publishes; the game memory is then only asked
    /// whether the process still runs.
    published: Option<publish::Reader>,
    /// Command basis of the last published frame acquired.
    published_basis: Option<CommandBasis>,
}

impl Engine {
    /// Attaches to a running game by process id. Nothing is injected.
    #[cfg(windows)]
    pub fn open(pid: u32) -> Result<Self, Error> {
        let memory = bwrapi_memory::WindowsProcessAttachment::attach_by_pid(
            bwrapi_memory::ProcessId(pid),
        )?;
        Self::with_memory(Box::new(memory))
    }

    /// Engine for code running inside the game process on the game thread:
    /// reads its own memory directly at the frame boundary.
    pub fn in_process() -> Result<Self, Error> {
        let memory = bwrapi_memory::InProcessMemory::attach()?;
        let mut engine = Self::with_memory(Box::new(memory))?;
        engine.verify_tearing = false;
        Ok(engine)
    }

    pub fn with_memory(memory: Box<dyn ProcessMemory + Send>) -> Result<Self, Error> {
        let module = memory.main_module();
        let layout = Layout::detect(module.size).ok_or(Error::UnsupportedBuild)?;
        Ok(Self {
            layout,
            module,
            memory,
            sources: 0,
            sequence: 0,
            match_id: 0,
            last_root: 0,
            last_game_frame: u32::MAX,
            last_local_player: u8::MAX,
            state: State::default(),
            frames: Vec::new(),
            verify_tearing: true,
            published: None,
            published_basis: None,
        })
    }

    /// Reads the frames the in-game DLL publishes through `publication`
    /// from now on, with the same sources and frame semantics.
    pub fn read_published(&mut self, publication: Box<dyn Publication>) {
        let mut reader = publish::Reader::new(publication);
        reader.subscribe(self.sources);
        self.published = Some(reader);
    }

    pub fn layout(&self) -> &'static Layout {
        self.layout
    }

    /// Selects the sources `acquire` reads (`tables::source` bits).
    pub fn subscribe(&mut self, sources: u64) {
        let changed = sources != self.sources;
        self.sources = sources;
        if let (true, Some(reader)) = (changed, &mut self.published) {
            reader.subscribe(sources);
        }
    }

    /// Names the turn-send count an in-game watcher last saw change and the
    /// QPC of the change; the next `acquire` publishes that time as the game
    /// table's send-time column while the count still matches.
    pub fn set_turn_watch(&mut self, watch: Option<(u32, u64)>) {
        self.state.turn_watch = watch;
    }

    /// Names the repair and refund totals the in-game DLL counted this match;
    /// the next `acquire` publishes them in the players table.
    pub fn set_resource_totals(&mut self, totals: Option<[ResourceTotals; RESOURCE_SLOTS]>) {
        self.state.resource_totals = totals;
    }

    /// Waits until the game has a frame newer than the last acquired one, or
    /// a new match started. Polls the frame counter of a game running in
    /// another process; the game thread calls `acquire` at its frame boundary
    /// instead and never polls.
    pub fn step(&mut self, timeout: Duration) -> Result<(), Error> {
        let deadline = Instant::now() + timeout;
        if self.published.is_some() {
            return self.step_published(deadline);
        }
        loop {
            let mut target = Target { memory: self.memory.as_ref(), layout: self.layout, module: self.module, root: 0 };
            target.root = in_game_root(&target)?;
            if target.root != self.last_root || game_frame(&target) != Some(self.last_game_frame) {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(Error::Timeout);
            }
            std::thread::sleep(Duration::from_micros(250));
        }
    }

    /// The values of the last `acquire`, or `None` before one, after the
    /// game left the match, or when the unit pool cannot be located. The pool
    /// base is read on demand when no unit source was subscribed.
    pub fn command_basis(&self) -> Option<CommandBasis> {
        if self.published.is_some() {
            return self.published_basis;
        }
        if self.last_root == 0 {
            return None;
        }
        let pool_base = match self.state.pool_base {
            0 => {
                let target = Target { memory: self.memory.as_ref(), layout: self.layout, module: self.module, root: self.last_root };
                read::pool::base(&target)?
            }
            base => base,
        };
        Some(CommandBasis {
            root: self.last_root as u64,
            pool_base: pool_base as u64,
            local_player: self.last_local_player,
            game_frame: self.last_game_frame,
            match_id: self.match_id,
        })
    }

    pub fn acquire(&mut self) -> Result<Arc<Frame>, Error> {
        if self.published.is_some() {
            return self.acquire_published();
        }
        let mut target = Target { memory: self.memory.as_ref(), layout: self.layout, module: self.module, root: 0 };
        target.root = match in_game_root(&target) {
            Ok(root) => root,
            Err(error) => {
                if error == Error::NotInGame {
                    self.last_root = 0;
                }
                return Err(error);
            }
        };
        if target.root != self.last_root {
            self.last_root = target.root;
            self.match_id += 1;
        }

        let index = free_frame(&mut self.frames);
        let frame = Arc::get_mut(&mut self.frames[index]).expect("frame has no other owner");

        let mut torn = false;
        let mut read = 0;
        if self.verify_tearing {
            for attempt in 0..2 {
                let before = game_frame(&target);
                frame.reset();
                read = read::fill(&target, &mut self.state, self.sources, frame);
                let after = game_frame(&target);
                frame.game_frame = after.unwrap_or(u32::MAX);
                torn = before != after;
                if !torn || attempt == 1 {
                    break;
                }
            }
        } else {
            frame.reset();
            read = read::fill(&target, &mut self.state, self.sources, frame);
            frame.game_frame = game_frame(&target).unwrap_or(u32::MAX);
        }

        let replay = target.module_u32(self.layout.scalars.session_is_replay_offset) == Some(1);
        self.sequence += 1;
        frame.sequence = self.sequence;
        frame.match_id = self.match_id;
        frame.sources = read;
        frame.flags = if torn { frame_flag::TORN } else { 0 } | if replay { frame_flag::REPLAY } else { 0 };
        frame.local_player = read::local_player(&target).unwrap_or(u8::MAX);
        self.last_game_frame = frame.game_frame;
        self.last_local_player = frame.local_player;
        Ok(Arc::clone(&self.frames[index]))
    }

    /// Copies the newest published frame. Right after a subscription change
    /// it waits (up to [`publish::COVER_TIMEOUT`]) for the game to publish
    /// the new sources; the game publishes on that change without waiting
    /// for a new frame, so the wait is about one loop step of the game.
    fn acquire_published(&mut self) -> Result<Arc<Frame>, Error> {
        let index = free_frame(&mut self.frames);
        let deadline = Instant::now() + publish::COVER_TIMEOUT;
        let mut retries = 0u32;
        let (header, carried) = loop {
            let reader = self.published.as_mut().expect("published engine");
            let frame = Arc::get_mut(&mut self.frames[index]).expect("frame has no other owner");
            let now = Instant::now();
            match reader.read(self.sources, frame, now >= deadline) {
                Attempt::Frame(header, carried) => break (header, carried),
                Attempt::NotInGame => {
                    self.published_basis = None;
                    self.last_root = 0;
                    // The last publication of a closed game stays readable.
                    return Err(if alive(self.memory.as_ref()) { Error::NotInGame } else { Error::Gone });
                }
                Attempt::Nothing | Attempt::Uncovered => {
                    if now >= deadline {
                        // Nothing published at all by now.
                        return Err(if alive(self.memory.as_ref()) { Error::NotInGame } else { Error::Gone });
                    }
                    reader.wait((deadline - now).min(WAIT_SLICE));
                    if !alive(self.memory.as_ref()) {
                        return Err(Error::Gone);
                    }
                }
                Attempt::Retry => {
                    // The game rewrites a slot in microseconds; a writer that
                    // stays mid-write is a game that died there.
                    retries += 1;
                    if retries % 64 == 0 && !alive(self.memory.as_ref()) {
                        return Err(Error::Gone);
                    }
                    std::thread::yield_now();
                }
            }
        };
        let frame = Arc::get_mut(&mut self.frames[index]).expect("frame has no other owner");
        self.sequence += 1;
        frame.sequence = self.sequence;
        frame.match_id = header.match_id;
        frame.game_frame = header.game_frame;
        frame.flags = header.flags;
        frame.local_player = header.local_player;
        frame.map_width = header.map_width;
        frame.map_height = header.map_height;
        frame.sources = carried;
        self.match_id = header.match_id;
        self.last_root = header.root as usize;
        self.last_game_frame = header.game_frame;
        self.last_local_player = header.local_player;
        self.published_basis = (header.has_basis != 0).then_some(CommandBasis {
            root: header.root,
            pool_base: header.pool_base,
            local_player: header.local_player,
            game_frame: header.game_frame,
            match_id: header.match_id,
        });
        Ok(Arc::clone(&self.frames[index]))
    }

    /// `step` over the publication: waits on the game's publish signal
    /// instead of polling the game memory.
    fn step_published(&mut self, deadline: Instant) -> Result<(), Error> {
        let mut retries = 0u32;
        loop {
            let reader = self.published.as_mut().expect("published engine");
            match reader.peek() {
                Attempt::NotInGame => {
                    return Err(if alive(self.memory.as_ref()) { Error::NotInGame } else { Error::Gone })
                }
                Attempt::Frame(header, _)
                    if header.match_id != self.match_id || header.game_frame != self.last_game_frame =>
                {
                    return Ok(())
                }
                Attempt::Retry => {
                    // As in acquire: a slot that stays mid-write is a game that
                    // died there, and the deadline still holds.
                    retries += 1;
                    if retries % 64 == 0 {
                        if !alive(self.memory.as_ref()) {
                            return Err(Error::Gone);
                        }
                        if Instant::now() >= deadline {
                            return Err(Error::Timeout);
                        }
                    }
                    std::thread::yield_now();
                    continue;
                }
                _ => {}
            }
            let now = Instant::now();
            if now >= deadline {
                return Err(if alive(self.memory.as_ref()) { Error::Timeout } else { Error::Gone });
            }
            reader.wait((deadline - now).min(WAIT_SLICE));
            if !alive(self.memory.as_ref()) {
                return Err(Error::Gone);
            }
        }
    }
}

/// A frame no one else holds, made when every one is held.
fn free_frame(frames: &mut Vec<Arc<Frame>>) -> usize {
    match frames.iter_mut().position(|frame| Arc::get_mut(frame).is_some()) {
        Some(index) => index,
        None => {
            frames.push(Arc::default());
            frames.len() - 1
        }
    }
}

fn alive(memory: &dyn ProcessMemory) -> bool {
    memory.is_alive().unwrap_or(false)
}

/// Longest single wait on the publish signal, so a game that exits is seen
/// within it.
const WAIT_SLICE: Duration = Duration::from_millis(50);

/// The match's BWGame root (0 when unknown), or why there is none.
fn in_game_root(target: &Target) -> Result<usize, Error> {
    match target.module_u32(target.layout.scalars.session_is_in_game_offset) {
        Some(0) => Err(Error::NotInGame),
        Some(_) => Ok(read_root(target).unwrap_or(0)),
        None if !target.memory.is_alive().unwrap_or(false) => Err(Error::Gone),
        None => Err(Error::NotInGame),
    }
}

fn read_root(target: &Target) -> Option<usize> {
    let root = &target.layout.menu_root;
    let root = (target.module_pointer(root.pointer)? as u64 ^ root.xor_key) as usize;
    (root != 0).then_some(root)
}

pub(crate) fn game_frame(target: &Target) -> Option<u32> {
    (target.root != 0).then(|| target.u32_at(target.root + BWGAME_FRAME_COUNT_OFFSET)).flatten()
}
