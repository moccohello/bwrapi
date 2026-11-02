//! In-memory process for engine tests: byte ranges at absolute addresses.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use bwrapi_layout::{CunitLayout, Layout};
use bwrapi_memory::{MemoryError, MemoryResult, ProcessMemory, ProcessModule};

pub const MODULE_BASE: usize = 0x1_4000_0000;

pub struct FakeMemory {
    pub module: ProcessModule,
    ranges: Mutex<Vec<(usize, Vec<u8>)>>,
    pub reads: Mutex<Vec<(usize, usize)>>,
    alive: AtomicBool,
}

impl FakeMemory {
    /// An x64 game process with nothing mapped.
    pub fn new() -> Self {
        Self {
            module: ProcessModule { base_address: MODULE_BASE, size: Layout::X64.module_size as usize },
            ranges: Mutex::new(Vec::new()),
            reads: Mutex::new(Vec::new()),
            alive: AtomicBool::new(true),
        }
    }

    pub fn put(&self, address: usize, bytes: &[u8]) {
        self.ranges.lock().unwrap().push((address, bytes.to_vec()));
    }

    pub fn put_module(&self, rva: usize, bytes: &[u8]) {
        self.put(MODULE_BASE + rva, bytes);
    }

    /// Writes into an existing range that covers `address`.
    pub fn patch(&self, address: usize, bytes: &[u8]) {
        let mut ranges = self.ranges.lock().unwrap();
        let (base, data) = ranges
            .iter_mut()
            .rev()
            .find(|(base, data)| address >= *base && address + bytes.len() <= base + data.len())
            .expect("patch inside a range");
        let at = address - *base;
        data[at..at + bytes.len()].copy_from_slice(bytes);
    }

    pub fn read_exact(&self, address: usize, size: usize) -> MemoryResult<Vec<u8>> {
        self.reads.lock().unwrap().push((address, size));
        let ranges = self.ranges.lock().unwrap();
        ranges
            .iter()
            .rev()
            .find(|(base, data)| address >= *base && address + size <= base + data.len())
            .map(|(base, data)| data[address - base..address - base + size].to_vec())
            .ok_or(MemoryError::ReadFailed { size })
    }

    pub fn reads_in(&self, start: usize, len: usize) -> usize {
        self.reads.lock().unwrap().iter().filter(|(address, _)| (start..start + len).contains(address)).count()
    }

    pub fn kill(&self) {
        self.alive.store(false, Ordering::Relaxed);
    }
}

impl ProcessMemory for FakeMemory {
    fn main_module(&self) -> ProcessModule {
        self.module
    }

    fn read_into(&self, address: usize, buffer: &mut [u8]) -> MemoryResult<()> {
        let bytes = self.read_exact(address, buffer.len())?;
        buffer.copy_from_slice(&bytes);
        Ok(())
    }

    fn is_alive(&self) -> MemoryResult<bool> {
        Ok(self.alive.load(Ordering::Relaxed))
    }
}

/// Lets a test keep the memory while the engine owns a handle to it.
pub struct Shared(pub std::sync::Arc<FakeMemory>);

impl ProcessMemory for Shared {
    fn main_module(&self) -> ProcessModule {
        self.0.main_module()
    }
    fn read_into(&self, address: usize, buffer: &mut [u8]) -> MemoryResult<()> {
        self.0.read_into(address, buffer)
    }
    fn is_alive(&self) -> MemoryResult<bool> {
        self.0.is_alive()
    }
}

/// A minimal in-game x64 process: in game, a BWGame root at `ROOT`, and an
/// empty CUnit pool at `POOL` with `slots` records.
pub const ROOT: usize = 0x2_0000_0000;
pub const POOL: usize = 0x3_0000_0000;

pub fn in_game(slots: usize) -> std::sync::Arc<FakeMemory> {
    let memory = FakeMemory::new();
    write_in_game_image(&Layout::X64, MODULE_BASE, ROOT, POOL, slots, &mut |address, bytes| {
        memory.put(address, bytes)
    });
    std::sync::Arc::new(memory)
}

/// Writes a live unit record into `slot` of the `in_game` pool.
pub fn put_unit(memory: &FakeMemory, slot: usize, unit_type: u16, owner: u8, x: i16, y: i16, uniqueness: u8) {
    write_unit(&Layout::X64, POOL, slot, unit_type, owner, x, y, uniqueness, &mut |address, bytes| {
        memory.patch(address, bytes)
    });
}

/// Writes the minimal in-game image: session flags, the menu-root pointer to
/// `root`, an empty BWGame root block, and a CUnit pool of `slots` records at
/// `pool`. `write` receives absolute addresses, so any image the reader can
/// address works (fake ranges, or a real buffer of the test process).
pub fn write_in_game_image(
    layout: &Layout,
    module_base: usize,
    root: usize,
    pool: usize,
    slots: usize,
    write: &mut dyn FnMut(usize, &[u8]),
) {
    write(module_base + layout.scalars.session_is_in_game_offset, &1u32.to_le_bytes());
    write(module_base + layout.scalars.session_is_replay_offset, &0u32.to_le_bytes());
    write(module_base + layout.menu_root.pointer, &(root as u64 ^ layout.menu_root.xor_key).to_le_bytes());
    write(root, &vec![0; crate::decode::bwgame::BWGAME_ROOT_READ_BYTES]);
    let cunit = &layout.cunit;
    write(module_base + cunit.array_pointer_rva, &((pool + cunit.selection_ptr_adjustment) as u64).to_le_bytes());
    write(module_base + cunit.capacity_pointer_rva, &(slots as u32).to_le_bytes());
    write(pool, &vec![0; slots * cunit.record_stride]);
}

/// Writes a live unit record into `slot` of the pool at `pool`.
pub fn write_unit(
    layout: &Layout,
    pool: usize,
    slot: usize,
    unit_type: u16,
    owner: u8,
    x: i16,
    y: i16,
    uniqueness: u8,
    write: &mut dyn FnMut(usize, &[u8]),
) {
    let c: &CunitLayout = &layout.cunit;
    let at = pool + slot * c.record_stride;
    write(at + c.type_offset, &unit_type.to_le_bytes());
    write(at + c.owner_offset, &[owner]);
    write(at + c.position_x_offset, &x.to_le_bytes());
    write(at + c.position_y_offset, &y.to_le_bytes());
    write(at + c.hit_points_offset, &(40 * 256i32).to_le_bytes());
    write(at + c.order_offset, &[3]);
    write(at + c.uniqueness_identifier_offset, &[uniqueness]);
}
