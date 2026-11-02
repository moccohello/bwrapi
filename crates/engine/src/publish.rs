//! Frames the in-game DLL publishes for readers in other processes. The game
//! thread already holds a decoded frame at its frame boundary; copying its
//! tables into shared memory spares an outside reader decoding the game
//! through another process, and the reader turns the copy back into the same
//! tables with plain column copies.
//!
//! Two mappings: a fixed [`Control`] block (version, reader slots, the newest
//! publication) and a data mapping of [`SLOTS`] frame slots followed by one
//! match area. A slot is a header with the row count of every table, then the
//! carried tables in source bit order, each column padded to 8 bytes. The
//! match area holds the match-long tables the same way and is rewritten only
//! when the match or the requested match tables change.
//!
//! The game thread is the only writer and never waits. A slot and the match
//! area start with a sequence that is odd while being written and `2 * n`
//! once publication `n` is complete; a reader keeps a copy only when the
//! sequence reads `2 * n` before and after it. A reader [`SLOTS`]
//! publications behind loses its slot to the game and takes the newest one.

use std::ops::Range;
use std::ptr;
use std::sync::atomic::Ordering::{AcqRel, Acquire, Relaxed, Release};
use std::sync::atomic::{fence, AtomicU32, AtomicU64};
use std::time::Duration;

use crate::columns::{Columns, ColumnsMut};
use crate::engine::{CommandBasis, Frame};
use crate::tables::{MatchTables, FRAME_TABLES, TABLES};

pub const VERSION: u32 = 1;
/// Frame slots of a data mapping. Three let the game write one while a reader
/// copies the newest and a slow reader still finishes the one before.
pub const SLOTS: usize = 3;
/// Readers attached at the same time.
pub const READERS: usize = 16;
/// How long `acquire` waits for the game to publish a source the reader just
/// subscribed. The game publishes on a subscription change without waiting
/// for a new frame, so this only runs out when the game cannot read it.
pub const COVER_TIMEOUT: Duration = Duration::from_millis(500);

const TABLE_COUNT: usize = TABLES.len();
const MATCH_SOURCES: u64 = ((1 << TABLE_COUNT) - 1) & !((1 << FRAME_TABLES) - 1);
const DATA_HEADER: usize = 64;
const SLOT_DATA: usize = pad_to(8 + std::mem::size_of::<Header>(), 64);
const MATCH_DATA: usize = pad_to(8 + std::mem::size_of::<MatchHeader>(), 64);
/// Mappings grow in steps of this many bytes.
const GRANULE: usize = 64 << 10;

const fn pad_to(bytes: usize, to: usize) -> usize {
    (bytes + to - 1) / to * to
}

/// Control block shared as `Local\bwrapi_game_publish_<pid>`.
#[repr(C)]
#[derive(Default)]
pub struct Control {
    pub version: AtomicU32,
    /// Generation of the data mapping that holds publication `published`;
    /// 0 before the first publication.
    pub generation: AtomicU32,
    /// Publications so far; the newest sits in slot `(published - 1) % SLOTS`.
    pub published: AtomicU64,
    /// Bumped by a reader that attaches, subscribes or leaves, so the game
    /// publishes even while the frame stands still (a paused game, the menu).
    pub change: AtomicU32,
    /// Game-thread time of the last publication, in microseconds.
    pub publish_us: AtomicU32,
    pub readers: [ReaderSlot; READERS],
}

#[repr(C)]
#[derive(Default)]
pub struct ReaderSlot {
    /// Process id of the reader holding the slot; 0 when free.
    pub owner: AtomicU32,
    pub sources: AtomicU64,
}

impl Control {
    /// Union of the attached readers' sources, or `None` when no reader is
    /// attached and the game publishes nothing.
    pub fn readers(&self) -> Option<u64> {
        let mut attached = false;
        let mut sources = 0;
        for reader in &self.readers {
            if reader.owner.load(Acquire) != 0 {
                attached = true;
                sources |= reader.sources.load(Acquire);
            }
        }
        attached.then_some(sources)
    }

    /// Claims a free reader slot for process `owner`.
    pub fn attach(&self, owner: u32) -> Option<usize> {
        let slot = self
            .readers
            .iter()
            .position(|reader| reader.owner.compare_exchange(0, owner, AcqRel, Relaxed).is_ok())?;
        self.change.fetch_add(1, Release);
        Some(slot)
    }

    pub fn subscribe(&self, slot: usize, sources: u64) {
        self.readers[slot].sources.store(sources, Release);
        self.change.fetch_add(1, Release);
    }

    pub fn detach(&self, slot: usize) {
        self.readers[slot].sources.store(0, Release);
        self.readers[slot].owner.store(0, Release);
        self.change.fetch_add(1, Release);
    }
}

/// A mapped data region.
#[derive(Clone, Copy, Debug)]
pub struct Region {
    base: *mut u8,
    len: usize,
}

// The region is shared memory; the protocol above orders every access.
unsafe impl Send for Region {}

impl Region {
    /// # Safety
    /// `base` is 8-byte aligned and valid for `len` bytes while the region
    /// is used; only the publishing game thread writes through it.
    pub unsafe fn new(base: *mut u8, len: usize) -> Self {
        Self { base, len }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    fn capacity(&self) -> Option<Capacity> {
        if self.len < DATA_HEADER {
            return None;
        }
        let header = unsafe { ptr::read(self.base.cast::<DataHeader>()) };
        let capacity = Capacity { slot_bytes: header.slot_bytes as usize, match_bytes: header.match_bytes as usize };
        let aligned = capacity.slot_bytes % 64 == 0 && capacity.slot_bytes >= SLOT_DATA && capacity.match_bytes >= MATCH_DATA;
        (aligned && capacity.data_len() <= self.len).then_some(capacity)
    }

    fn slot(&self, capacity: Capacity, publication: u64) -> *mut u8 {
        let index = ((publication - 1) % SLOTS as u64) as usize;
        unsafe { self.base.add(DATA_HEADER + index * capacity.slot_bytes) }
    }

    fn match_area(&self, capacity: Capacity) -> *mut u8 {
        unsafe { self.base.add(DATA_HEADER + SLOTS * capacity.slot_bytes) }
    }
}

/// Written once when a data mapping is made; never changes after.
#[repr(C)]
#[derive(Clone, Copy)]
struct DataHeader {
    slot_bytes: u64,
    match_bytes: u64,
}

/// Room of one frame slot and of the match area.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Capacity {
    pub slot_bytes: usize,
    pub match_bytes: usize,
}

impl Capacity {
    /// Bytes of a data mapping with this capacity.
    pub fn data_len(self) -> usize {
        DATA_HEADER + SLOTS * self.slot_bytes + self.match_bytes
    }

    fn fits(self, needed: Capacity) -> bool {
        self.slot_bytes >= needed.slot_bytes && self.match_bytes >= needed.match_bytes
    }

    /// Room for `needed` and half as much again, so a match whose tables
    /// grow makes few new mappings.
    fn grown(self, needed: Capacity) -> Capacity {
        let room = |have: usize, need: usize| have.max(pad_to(need + need / 2, GRANULE));
        Capacity { slot_bytes: room(self.slot_bytes, needed.slot_bytes), match_bytes: room(self.match_bytes, needed.match_bytes) }
    }
}

/// Slot header after the sequence.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct Header {
    /// Match area publication the slot's match tables are in.
    match_seq: u64,
    /// Tables the slot carries (match ones through the match area).
    pub sources: u64,
    /// Sources the game read for its readers.
    pub requested: u64,
    pub root: u64,
    pub pool_base: u64,
    pub match_id: u32,
    pub game_frame: u32,
    pub flags: u32,
    pub in_game: u8,
    pub local_player: u8,
    pub has_basis: u8,
    _pad: u8,
    pub map_width: u16,
    pub map_height: u16,
    rows: [u32; TABLE_COUNT],
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct MatchHeader {
    sources: u64,
    rows: [u32; TABLE_COUNT],
}

fn table_bytes(index: usize, rows: usize) -> usize {
    TABLES[index].columns.iter().map(|column| pad_to(rows * column.kind.bytes(), 8)).sum()
}

fn tables_bytes(sources: u64, rows: &[u32; TABLE_COUNT], range: Range<usize>) -> usize {
    range.filter(|index| sources & 1 << index != 0).map(|index| table_bytes(index, rows[index] as usize)).sum()
}

fn frame_rows(frame: &Frame, sources: u64, range: Range<usize>) -> [u32; TABLE_COUNT] {
    let mut rows = [0; TABLE_COUNT];
    for index in range.filter(|index| sources & 1 << index != 0) {
        rows[index] = frame.table(index).map_or(0, |table| table.rows() as u32);
    }
    rows
}

unsafe fn write_table(mut at: *mut u8, index: usize, table: &dyn Columns) -> *mut u8 {
    let rows = table.rows();
    for (column, info) in TABLES[index].columns.iter().enumerate() {
        let bytes = rows * info.kind.bytes();
        if bytes != 0 {
            ptr::copy_nonoverlapping(table.column_ptr(column), at, bytes);
        }
        at = at.add(pad_to(bytes, 8));
    }
    at
}

unsafe fn read_table(mut at: *const u8, index: usize, rows: usize, table: &mut dyn ColumnsMut) -> *const u8 {
    table.set_rows(rows);
    for (column, info) in TABLES[index].columns.iter().enumerate() {
        let bytes = rows * info.kind.bytes();
        if bytes != 0 {
            ptr::copy_nonoverlapping(at, table.column_mut_ptr(column), bytes);
        }
        at = at.add(pad_to(bytes, 8));
    }
    at
}

fn sequence<'a>(at: *mut u8) -> &'a AtomicU64 {
    // Slots and the match area start 64-byte aligned.
    unsafe { AtomicU64::from_ptr(at.cast()) }
}

/// Game-thread side: publishes frames and makes a larger data mapping when
/// one does not fit. A data mapping never grows in place, because readers
/// hold views of it; the next generation replaces it and the old one lives
/// on while a reader still maps it.
#[derive(Default)]
pub struct Writer {
    published: u64,
    generation: u32,
    region: Option<Region>,
    capacity: Capacity,
    match_seq: u64,
    /// Generation, match and sources the match area holds.
    match_generation: u32,
    match_id: u32,
    match_sources: u64,
}

impl Writer {
    /// Publishes `frame`'s tables of `sources`, or that the game is not in a
    /// match when `frame` is `None`. `requested` is what the game read for
    /// its readers; a reader waits for a publication covering its own
    /// sources. `map(generation, len)` makes a zeroed region of `len` bytes
    /// readers find under `generation`; it is called only when the tables do
    /// not fit the current one. Returns false when no region could be made.
    ///
    /// # Safety
    /// `control` is the shared control block and this is its only writer;
    /// the regions `map` returns stay valid until the next call of `map`.
    pub unsafe fn publish(
        &mut self,
        control: &Control,
        frame: Option<&Frame>,
        sources: u64,
        requested: u64,
        basis: Option<&CommandBasis>,
        map: &mut dyn FnMut(u32, usize) -> Option<Region>,
    ) -> bool {
        let sources = frame.map_or(0, |frame| sources & frame.sources);
        let match_sources = sources & MATCH_SOURCES;
        let rows = frame.map_or([0; TABLE_COUNT], |frame| frame_rows(frame, sources, 0..TABLE_COUNT));
        let needed = Capacity {
            slot_bytes: SLOT_DATA + tables_bytes(sources, &rows, 0..FRAME_TABLES),
            match_bytes: MATCH_DATA + tables_bytes(match_sources, &rows, FRAME_TABLES..TABLE_COUNT),
        };
        let region = match self.region {
            Some(region) if self.capacity.fits(needed) => region,
            _ => {
                let capacity = self.capacity.grown(needed);
                let Some(region) = map(self.generation + 1, capacity.data_len()) else {
                    return false;
                };
                ptr::write(
                    region.base.cast::<DataHeader>(),
                    DataHeader { slot_bytes: capacity.slot_bytes as u64, match_bytes: capacity.match_bytes as u64 },
                );
                self.generation += 1;
                self.region = Some(region);
                self.capacity = capacity;
                region
            }
        };

        let mut header = Header::default();
        if let Some(frame) = frame {
            let current = self.match_generation == self.generation && self.match_id == frame.match_id;
            if match_sources != 0 && !(current && match_sources & !self.match_sources == 0) {
                self.write_match(region, frame, match_sources, &rows);
            }
            let current = self.match_generation == self.generation && self.match_id == frame.match_id;
            let carried_match = if current { self.match_sources & match_sources } else { 0 };
            header = Header {
                match_seq: self.match_seq,
                sources: (sources & !MATCH_SOURCES) | carried_match,
                requested,
                root: basis.map_or(0, |basis| basis.root),
                pool_base: basis.map_or(0, |basis| basis.pool_base),
                match_id: frame.match_id,
                game_frame: frame.game_frame,
                flags: frame.flags,
                in_game: 1,
                local_player: frame.local_player,
                has_basis: basis.is_some() as u8,
                _pad: 0,
                map_width: frame.map_width,
                map_height: frame.map_height,
                rows,
            };
        }

        self.published += 1;
        let publication = self.published;
        let slot = region.slot(self.capacity, publication);
        let seq = sequence(slot);
        seq.store(2 * publication - 1, Relaxed);
        fence(Release);
        ptr::write(slot.add(8).cast::<Header>(), header);
        if let Some(frame) = frame {
            let mut at = slot.add(SLOT_DATA);
            for index in (0..FRAME_TABLES).filter(|index| header.sources & 1 << index != 0) {
                at = write_table(at, index, frame.table(index).expect("a table per source"));
            }
        }
        seq.store(2 * publication, Release);
        // A reader loads the generation before the publication count, so it
        // never pairs a new count with a region that lacks that slot.
        control.generation.store(self.generation, Release);
        control.published.store(publication, Release);
        true
    }

    unsafe fn write_match(&mut self, region: Region, frame: &Frame, sources: u64, rows: &[u32; TABLE_COUNT]) {
        self.match_seq += 1;
        let area = region.match_area(self.capacity);
        let seq = sequence(area);
        seq.store(2 * self.match_seq - 1, Relaxed);
        fence(Release);
        ptr::write(area.add(8).cast::<MatchHeader>(), MatchHeader { sources, rows: *rows });
        let mut at = area.add(MATCH_DATA);
        for index in (FRAME_TABLES..TABLE_COUNT).filter(|index| sources & 1 << index != 0) {
            at = write_table(at, index, frame.table(index).expect("a table per source"));
        }
        seq.store(2 * self.match_seq, Release);
        self.match_generation = self.generation;
        self.match_id = frame.match_id;
        self.match_sources = sources;
    }
}

/// Reader side of one attached reader: the control block, the data mapping
/// of a generation, and a wake-up on each publication.
pub trait Publication: Send {
    fn control(&self) -> &Control;
    /// The data region of `generation`, mapping it in place of an older one;
    /// `None` when that generation is gone already.
    fn data(&mut self, generation: u32) -> Option<Region>;
    /// Blocks until the game publishes again or `timeout` passes.
    fn wait(&self, timeout: Duration);
    /// This reader's slot in [`Control::readers`]; dropping the publication
    /// detaches it.
    fn slot(&self) -> usize;
}

pub(crate) enum Attempt {
    /// The header and the sources copied into the frame.
    Frame(Header, u64),
    NotInGame,
    /// Nothing published since this reader subscribed.
    Nothing,
    /// The newest publication does not cover the subscribed sources yet.
    Uncovered,
    /// The game rewrote what was being read; read again.
    Retry,
}

pub(crate) struct Reader {
    publication: Box<dyn Publication>,
    /// Match tables copied from the match area, the area publication they
    /// came from, and the sources asked and found there.
    shared: MatchTables,
    match_seq: u64,
    match_asked: u64,
    match_have: u64,
    /// Newest publication when this reader last subscribed. The game publishes
    /// only for readers, so that one may be long old: it stands for neither
    /// the current frame nor whether a match runs.
    subscribed_at: u64,
}

impl Reader {
    pub fn new(publication: Box<dyn Publication>) -> Self {
        Self { publication, shared: MatchTables::default(), match_seq: 0, match_asked: 0, match_have: 0, subscribed_at: 0 }
    }

    pub fn subscribe(&mut self, sources: u64) {
        self.subscribed_at = self.publication.control().published.load(Acquire);
        self.publication.control().subscribe(self.publication.slot(), sources);
    }

    pub fn wait(&self, timeout: Duration) {
        self.publication.wait(timeout);
    }

    /// Region, capacity and number of the newest publication.
    fn locate(&mut self) -> Result<(Region, Capacity, u64), Attempt> {
        let control = self.publication.control();
        let generation = control.generation.load(Acquire);
        let publication = control.published.load(Acquire);
        if publication == 0 {
            return Err(Attempt::Nothing);
        }
        let region = self.publication.data(generation).ok_or(Attempt::Retry)?;
        let capacity = region.capacity().ok_or(Attempt::Retry)?;
        Ok((region, capacity, publication))
    }

    /// Header of the newest publication.
    pub fn peek(&mut self) -> Attempt {
        let (region, capacity, publication) = match self.locate() {
            Ok(located) => located,
            Err(attempt) => return attempt,
        };
        if publication <= self.subscribed_at {
            return Attempt::Nothing;
        }
        let slot = region.slot(capacity, publication);
        let seq = sequence(slot);
        if seq.load(Acquire) != 2 * publication {
            return Attempt::Retry;
        }
        let header = unsafe { ptr::read_volatile(slot.add(8).cast::<Header>()) };
        fence(Acquire);
        match (seq.load(Relaxed) == 2 * publication, header.in_game) {
            (false, _) => Attempt::Retry,
            (true, 0) => Attempt::NotInGame,
            (true, _) => Attempt::Frame(header, 0),
        }
    }

    /// Copies the newest publication's tables of `sources` into `frame`.
    /// A publication made before this subscription is left for a newer one;
    /// so is one made before the game saw it, unless `accept_uncovered`.
    pub fn read(&mut self, sources: u64, frame: &mut Frame, accept_uncovered: bool) -> Attempt {
        let (region, capacity, publication) = match self.locate() {
            Ok(located) => located,
            Err(attempt) => return attempt,
        };
        if publication <= self.subscribed_at {
            return Attempt::Nothing;
        }
        let slot = region.slot(capacity, publication);
        let seq = sequence(slot);
        if seq.load(Acquire) != 2 * publication {
            return Attempt::Retry;
        }
        let header = unsafe { ptr::read_volatile(slot.add(8).cast::<Header>()) };
        let stable = || {
            fence(Acquire);
            seq.load(Relaxed) == 2 * publication
        };
        if header.in_game == 0 {
            return if stable() { Attempt::NotInGame } else { Attempt::Retry };
        }
        if !accept_uncovered && header.requested & sources != sources {
            return if stable() { Attempt::Uncovered } else { Attempt::Retry };
        }
        // Rows read under a concurrent rewrite are checked by the second
        // sequence load only after the copy; bound them by the slot first.
        if SLOT_DATA + tables_bytes(header.sources, &header.rows, 0..FRAME_TABLES) > capacity.slot_bytes {
            return Attempt::Retry;
        }
        let wanted_match = header.sources & sources & MATCH_SOURCES;
        if wanted_match != 0 && (header.match_seq != self.match_seq || wanted_match & !self.match_asked != 0) {
            if !unsafe { self.load_match(region, capacity, header.match_seq, wanted_match) } {
                return Attempt::Retry;
            }
        }

        let copied = header.sources & sources;
        let mut at = unsafe { slot.add(SLOT_DATA) }.cast_const();
        for index in 0..FRAME_TABLES {
            let table = frame.table_mut(index).expect("a frame table per index");
            if header.sources & 1 << index == 0 {
                table.set_rows(0);
                continue;
            }
            let rows = header.rows[index] as usize;
            if copied & 1 << index != 0 {
                unsafe { read_table(at, index, rows, table) };
            } else {
                table.set_rows(0);
            }
            at = unsafe { at.add(table_bytes(index, rows)) };
        }
        if !stable() {
            return Attempt::Retry;
        }
        frame.shared = self.shared.clone();
        Attempt::Frame(header, (copied & !MATCH_SOURCES) | (wanted_match & self.match_have))
    }

    unsafe fn load_match(&mut self, region: Region, capacity: Capacity, match_seq: u64, wanted: u64) -> bool {
        let area = region.match_area(capacity);
        let seq = sequence(area);
        if seq.load(Acquire) != 2 * match_seq {
            return false;
        }
        let header = ptr::read_volatile(area.add(8).cast::<MatchHeader>());
        if MATCH_DATA + tables_bytes(header.sources & MATCH_SOURCES, &header.rows, FRAME_TABLES..TABLE_COUNT)
            > capacity.match_bytes
        {
            return false;
        }
        let mut at = area.add(MATCH_DATA).cast_const();
        for index in (FRAME_TABLES..TABLE_COUNT).filter(|index| header.sources & 1 << index != 0) {
            let rows = header.rows[index] as usize;
            if wanted & 1 << index != 0 {
                read_table(at, index, rows, self.shared.table_mut(index).expect("a match table per index"));
            }
            at = at.add(table_bytes(index, rows));
        }
        fence(Acquire);
        if seq.load(Relaxed) != 2 * match_seq {
            // Torn copy: read the area again next time.
            self.match_seq = 0;
            return false;
        }
        self.match_seq = match_seq;
        self.match_asked = wanted;
        self.match_have = wanted & header.sources;
        true
    }
}
