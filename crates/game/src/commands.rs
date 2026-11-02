//! The command ring between command producers (a bot host in another
//! process, an in-process session, the example tools) and the in-game frame
//! callback that executes commands: a bounded multi-producer queue of fixed
//! command records plus a result ring keyed by command id. The game thread
//! is the only consumer of the queue; results are published by writing the
//! id last, with `Release`.

// The producers and the executor are Windows-only; the ring itself is portable.
#![cfg_attr(not(windows), allow(dead_code))]

use std::cell::UnsafeCell;
use std::sync::atomic::Ordering::{Acquire, AcqRel, Relaxed, Release};
use std::sync::atomic::{AtomicU32, AtomicU64};

use bwrapi_engine::command::UnitCommand;
use bwrapi_engine::CommandBasis;

/// The number of unit ids one ring record carries, from the shared encoding.
pub use bwrapi_engine::command::{COMMAND_MAX_UNITS, COMMAND_TEXT_MAX};

/// What became of a submitted command. The numbers are the public
/// `BwrResult` states, so the FFI surface reports them unchanged.
pub mod command_result {
    /// The game calls ran.
    pub const OK: u32 = 1;
    /// The command is not shaped for its kind.
    pub const REJECTED: u32 = 2;
    /// No match is running (or this build has no command functions).
    pub const UNAVAILABLE: u32 = 3;
    /// A named unit is no longer the unit the caller saw.
    pub const NOT_EXECUTED: u32 = 4;
}

pub const COMMANDS_VERSION: u32 = 2;
pub const COMMAND_RING_CAPACITY: usize = 256;
pub const RESULT_RING_CAPACITY: usize = 1024;

/// One queue slot: the record plus the sequence that guards it. A producer
/// may write the record at queue position `p` when `sequence == p`; the
/// consumer may copy it when `sequence == p + 1`; it is free again for
/// position `p + capacity` once the consumer copied it.
#[repr(C)]
pub(crate) struct CommandSlot {
    pub sequence: AtomicU64,
    pub command: UnsafeCell<CommandRecord>,
}

/// A command as it travels the ring; the fields mirror [`UnitCommand`].
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CommandRecord {
    /// Result id this command publishes under; allocated by the producer.
    pub id: u64,
    pub kind: u32,
    /// Bit 0 queued, bit 1 tile.
    pub flags: u32,
    pub type_id: u32,
    pub slot: u32,
    pub x: i32,
    pub y: i32,
    /// Target unit id, 0 for none.
    pub target: u64,
    /// Unit ids, or the NUL-terminated text of a text kind.
    pub units: [u64; COMMAND_MAX_UNITS],
    pub unit_count: u32,
    pub text_len: u32,
}

impl Default for CommandRecord {
    fn default() -> Self {
        Self {
            id: 0, kind: 0, flags: 0, type_id: 0, slot: 0, x: 0, y: 0, target: 0,
            units: [0; COMMAND_MAX_UNITS], unit_count: 0, text_len: 0,
        }
    }
}

/// One result ring slot; `id` is written last with `Release`, so a reader
/// that acquires the id it wants sees the state and frame of that result.
#[repr(C)]
pub(crate) struct ResultSlot {
    pub state: AtomicU32,
    pub frame: AtomicU32,
    pub id: AtomicU64,
}

/// The command basis a producer read when it submitted, so the frame
/// callback can execute commands without a read of its own. Producers
/// publish it before they queue a record; `sequence` closing the write
/// makes a reader see one producer's fields.
#[repr(C)]
pub(crate) struct BasisSlot {
    pub sequence: AtomicU64,
    pub root: AtomicU64,
    pub pool_base: AtomicU64,
    pub local_player: AtomicU32,
    pub game_frame: AtomicU32,
    pub match_id: AtomicU32,
    pub reserved: AtomicU32,
}

/// Block shared as `Local\bwrapi_game_commands_<pid>`.
#[repr(C)]
pub(crate) struct CommandsBlock {
    pub version: AtomicU32,
    /// Ids handed to commands so far; the next is `fetch_add(1) + 1`.
    pub next_id: AtomicU64,
    /// Next queue position a producer may claim.
    pub tail: AtomicU64,
    /// Next queue position the consumer takes; game thread only.
    pub head: AtomicU64,
    pub basis: BasisSlot,
    pub slots: [CommandSlot; COMMAND_RING_CAPACITY],
    pub results: [ResultSlot; RESULT_RING_CAPACITY],
}

// Every part of the block is reached only through its own protocol: slot
// records between their sequence stores, result fields before their id
// store. The atomics order all of it, so threads and processes may share
// the block.
unsafe impl Sync for CommandsBlock {}

pub(crate) fn commands_name(pid: u32) -> Vec<u16> {
    format!("Local\\bwrapi_game_commands_{pid}").encode_utf16().chain(Some(0)).collect()
}

/// Prepares a zeroed block for use; usable once `version` is published.
pub(crate) fn init(block: &CommandsBlock) {
    for (index, slot) in block.slots.iter().enumerate() {
        slot.sequence.store(index as u64, Relaxed);
    }
    block.version.store(COMMANDS_VERSION, Release);
}

fn slot_at(block: &CommandsBlock, position: u64) -> &CommandSlot {
    &block.slots[(position & (COMMAND_RING_CAPACITY as u64 - 1)) as usize]
}

/// Claims the next free slot and writes `record`; `false` while the queue is
/// full. Any number of producers may call this concurrently.
pub(crate) fn try_push(block: &CommandsBlock, record: CommandRecord) -> bool {
    let mut position = block.tail.load(Relaxed);
    loop {
        let slot = slot_at(block, position);
        let sequence = slot.sequence.load(Acquire);
        if sequence < position {
            return false;
        }
        if sequence > position
            || block.tail.compare_exchange_weak(position, position + 1, AcqRel, Relaxed).is_err()
        {
            position = block.tail.load(Relaxed);
            continue;
        }
        // The sequence guards the record: no reader holds it at `position`.
        unsafe { slot.command.get().write(record) };
        slot.sequence.store(position + 1, Release);
        return true;
    }
}

/// Takes the next queued record; `None` when the queue is empty. Only the
/// game thread calls this.
pub(crate) fn pop(block: &CommandsBlock) -> Option<CommandRecord> {
    let position = block.head.load(Relaxed);
    let slot = slot_at(block, position);
    let sequence = slot.sequence.load(Acquire);
    if sequence != position + 1 {
        return None;
    }
    let record = unsafe { slot.command.get().read() };
    slot.sequence.store(position + COMMAND_RING_CAPACITY as u64, Release);
    block.head.store(position + 1, Release);
    Some(record)
}

/// Whether the queue holds records, as the watcher's cheap idle check.
pub(crate) fn has_commands(block: &CommandsBlock) -> bool {
    block.tail.load(Relaxed) != block.head.load(Relaxed)
}

/// Publishes the basis a submitter read, before it queues the command.
pub(crate) fn publish_basis(block: &CommandsBlock, basis: &CommandBasis) {
    let slot = &block.basis;
    slot.root.store(basis.root, Relaxed);
    slot.pool_base.store(basis.pool_base, Relaxed);
    slot.local_player.store(u32::from(basis.local_player), Relaxed);
    slot.game_frame.store(basis.game_frame, Relaxed);
    slot.match_id.store(basis.match_id, Relaxed);
    slot.reserved.store(0, Relaxed);
    slot.sequence.fetch_add(1, Release);
}

/// The newest published basis, or `None` before any submitter named one.
/// Reading the sequence again after the fields rejects a publish that
/// started in between.
pub(crate) fn take_basis(block: &CommandsBlock) -> Option<CommandBasis> {
    let slot = &block.basis;
    let first = slot.sequence.load(Acquire);
    if first == 0 {
        return None;
    }
    let basis = CommandBasis {
        root: slot.root.load(Relaxed),
        pool_base: slot.pool_base.load(Relaxed),
        local_player: slot.local_player.load(Relaxed) as u8,
        game_frame: slot.game_frame.load(Relaxed),
        match_id: slot.match_id.load(Relaxed),
    };
    (slot.sequence.load(Acquire) == first).then_some(basis)
}

/// Publishes the outcome of command `id`.
pub(crate) fn publish_result(block: &CommandsBlock, id: u64, state: u32, frame: u32) {
    let slot = &block.results[(id % RESULT_RING_CAPACITY as u64) as usize];
    slot.state.store(state, Relaxed);
    slot.frame.store(frame, Relaxed);
    slot.id.store(id, Release);
}

/// The published outcome of `id`, or `None` while the ring carries no entry
/// for it. Reading the id again after the fields rejects a publish that
/// started in between.
pub(crate) fn read_result(block: &CommandsBlock, id: u64) -> Option<(u32, u32)> {
    let slot = &block.results[(id % RESULT_RING_CAPACITY as u64) as usize];
    let first = slot.id.load(Acquire);
    if first != id {
        return None;
    }
    let outcome = (slot.state.load(Relaxed), slot.frame.load(Relaxed));
    (slot.id.load(Acquire) == first).then_some(outcome)
}

/// Packs a command for the ring; `None` when it names more units than a
/// record holds.
fn record_of(id: u64, command: &UnitCommand) -> Option<CommandRecord> {
    if command.units.len() > COMMAND_MAX_UNITS || command.text.len() > COMMAND_TEXT_MAX {
        return None;
    }
    let mut units = [0; COMMAND_MAX_UNITS];
    units[..command.units.len()].copy_from_slice(&command.units);
    if !command.text.is_empty() {
        let mut bytes = [0u8; COMMAND_MAX_UNITS * 8];
        bytes[..command.text.len()].copy_from_slice(&command.text);
        for (unit, chunk) in units.iter_mut().zip(bytes.chunks_exact(8)) {
            *unit = u64::from_le_bytes(chunk.try_into().unwrap());
        }
    }
    Some(CommandRecord {
        id,
        kind: command.kind,
        flags: u32::from(command.queued) | u32::from(command.tile) << 1,
        type_id: command.type_id,
        slot: command.slot,
        x: command.x,
        y: command.y,
        target: command.target,
        units,
        unit_count: command.units.len() as u32,
        text_len: command.text.len() as u32,
    })
}

/// Unpacks a ring record into the shared command type.
fn unit_command(record: &CommandRecord) -> UnitCommand {
    let count = (record.unit_count as usize).min(COMMAND_MAX_UNITS);
    UnitCommand {
        kind: record.kind,
        queued: record.flags & 1 != 0,
        tile: record.flags & 2 != 0,
        type_id: record.type_id,
        slot: record.slot,
        x: record.x,
        y: record.y,
        target: record.target,
        units: record.units[..count].to_vec(),
        text: record_text(record).to_vec(),
    }
}

/// The record's text as the NUL-terminated string game functions take; the
/// text never fills the last byte, so the NUL is always there.
fn record_text_ptr(record: &CommandRecord) -> *const u8 {
    record.units.as_ptr().cast()
}

/// The text bytes a record carries, without the NUL.
fn record_text(record: &CommandRecord) -> &[u8] {
    let bytes = unsafe {
        std::slice::from_raw_parts(record.units.as_ptr().cast::<u8>(), COMMAND_MAX_UNITS * 8)
    };
    &bytes[..(record.text_len as usize).min(COMMAND_TEXT_MAX)]
}

#[cfg(windows)]
mod shared {
    use std::mem::size_of;

    use bwrapi_engine::command::{self, TargetFacts, UnitCommand, UnitFacts};
    use bwrapi_engine::decode::{bwgame, scalars};
    use bwrapi_engine::{unit_slot, CommandBasis};
    use bwrapi_layout::Layout;
    use bwrapi_memory::{InProcessMemory, ProcessMemory};

    use super::{command_result, unit_command, CommandsBlock};

    /// How long a producer keeps retrying a full queue before giving up: a
    /// few frames at the fastest speed.
    const PUSH_WAIT: std::time::Duration = std::time::Duration::from_millis(250);
    /// The game's own selection limit bounds one game call.
    const SELECTION_MAX: usize = 12;

    /// Creates `Local\bwrapi_game_commands_<pid>` in the game process; the
    /// mapping lives as long as the game, so its handle stays open.
    pub(crate) fn create_commands() -> Option<&'static CommandsBlock> {
        use std::ptr::null;
        use std::sync::atomic::Ordering;
        use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
        use windows_sys::Win32::System::Memory::{
            CreateFileMappingW, MapViewOfFile, FILE_MAP_ALL_ACCESS, PAGE_READWRITE,
        };
        use windows_sys::Win32::System::Threading::GetCurrentProcessId;

        let name = super::commands_name(unsafe { GetCurrentProcessId() });
        let size = size_of::<CommandsBlock>();
        let mapping = unsafe {
            CreateFileMappingW(INVALID_HANDLE_VALUE, null(), PAGE_READWRITE, 0, size as u32, name.as_ptr())
        };
        if mapping == 0 {
            return None;
        }
        let view = unsafe { MapViewOfFile(mapping, FILE_MAP_ALL_ACCESS, 0, 0, size) };
        let block = view.Value.cast::<CommandsBlock>();
        let block = (!block.is_null()).then(|| unsafe { &*block })?;
        if block.version.load(Ordering::Acquire) != super::COMMANDS_VERSION {
            super::init(block);
        }
        Some(block)
    }

    /// Producer handle to the command ring of game `pid`, from any process
    /// in the session (including the game itself).
    pub struct GameCommands {
        view: windows_sys::Win32::System::Memory::MEMORY_MAPPED_VIEW_ADDRESS,
        block: *mut CommandsBlock,
    }

    impl GameCommands {
        /// Opens the ring; `None` before the in-game DLL created it.
        pub fn open(pid: u32) -> Option<Self> {
            use std::sync::atomic::Ordering;
            use windows_sys::Win32::Foundation::CloseHandle;
            use windows_sys::Win32::System::Memory::{MapViewOfFile, OpenFileMappingW, FILE_MAP_ALL_ACCESS};

            let name = super::commands_name(pid);
            let mapping = unsafe { OpenFileMappingW(FILE_MAP_ALL_ACCESS, 0, name.as_ptr()) };
            if mapping == 0 {
                return None;
            }
            let view = unsafe { MapViewOfFile(mapping, FILE_MAP_ALL_ACCESS, 0, 0, size_of::<CommandsBlock>()) };
            unsafe { CloseHandle(mapping) };
            let block = view.Value.cast::<CommandsBlock>();
            if block.is_null() {
                return None;
            }
            let ring = Self { view, block };
            (unsafe { &*ring.block }.version.load(Ordering::Acquire) == super::COMMANDS_VERSION)
                .then_some(ring)
        }

        /// Queues the command, published with the basis its producer read,
        /// and returns its result id at once. A queue that stays full or an
        /// oversized command still returns its id, with the result ring
        /// carrying the failure.
        pub fn submit(&self, basis: &CommandBasis, command: &UnitCommand) -> Result<u64, String> {
            use std::sync::atomic::Ordering;

            let block = unsafe { &*self.block };
            if block.version.load(Ordering::Acquire) != super::COMMANDS_VERSION {
                return Err("the in-game command ring changed under the session".to_owned());
            }
            let id = block.next_id.fetch_add(1, Ordering::Relaxed) + 1;
            let Some(record) = super::record_of(id, command) else {
                super::publish_result(block, id, command_result::REJECTED, 0);
                return Ok(id);
            };
            // The basis travels ahead of the record, so the consumer never
            // sees a command without a basis.
            super::publish_basis(block, basis);
            let deadline = std::time::Instant::now() + PUSH_WAIT;
            while !super::try_push(block, record) {
                if std::time::Instant::now() >= deadline {
                    super::publish_result(block, id, command_result::UNAVAILABLE, 0);
                    return Ok(id);
                }
                std::thread::yield_now();
            }
            Ok(id)
        }

        /// The published outcome of `id`, or `None` while it has none.
        pub fn result(&self, id: u64) -> Option<(u32, u32)> {
            super::read_result(unsafe { &*self.block }, id)
        }
    }

    impl Drop for GameCommands {
        fn drop(&mut self) {
            unsafe { windows_sys::Win32::System::Memory::UnmapViewOfFile(self.view) };
        }
    }

    /// Executes one drained record on the game thread through the game's own
    /// submission functions: select the actors, then enqueue the packet or
    /// call the target-order builder.
    /// Unit ids are resolved through the pool of the frame just read, and a
    /// uniqueness mismatch stops the command: it must not reach whatever
    /// unit took the slot since.
    pub(crate) unsafe fn execute(
        memory: &InProcessMemory,
        layout: &Layout,
        basis: &CommandBasis,
        record: &super::CommandRecord,
    ) -> u32 {
        let command = unit_command(record);
        if command::check(&command).is_err() {
            return command_result::REJECTED;
        }
        if command::is_game_command(command.kind) {
            return execute_game(memory, layout, basis, &command, super::record_text_ptr(record));
        }
        let (select_rva, enqueue_rva, target_order_rva) = match (
            layout.commands.select_units,
            layout.commands.enqueue,
            layout.commands.target_order_builder,
        ) {
            (Some(select), Some(enqueue), Some(target)) => (select, enqueue, target),
            _ => return command_result::UNAVAILABLE,
        };
        let module = memory.main_module();
        let cunit = &layout.cunit;
        let capacity = module
            .range_address(cunit.capacity_pointer_rva, 4)
            .and_then(|address| read_u32(memory, address))
            .unwrap_or(0) as usize;
        let slots = capacity.min(cunit.region_bytes / cunit.record_stride);

        // The uniqueness byte is the accuracy device: the command goes to the
        // record only while it is still the unit the caller saw.
        let resolve = |id: u64| -> Option<(usize, UnitFacts)> {
            let slot = usize::from(unit_slot(id));
            if slot >= slots {
                return None;
            }
            let address = basis.pool_base as usize + slot * cunit.record_stride;
            let uniqueness = read_u8(memory, address.checked_add(cunit.uniqueness_identifier_offset)?)?;
            if uniqueness != (id >> 16) as u8 {
                return None;
            }
            let unit_type = read_u16(memory, address.checked_add(cunit.type_offset)?)?;
            let x = read_u16(memory, address.checked_add(cunit.position_x_offset)?)? as i16;
            let y = read_u16(memory, address.checked_add(cunit.position_y_offset)?)? as i16;
            let status = read_u32(memory, address.checked_add(cunit.status_flags_offset)?)?;
            Some((
                address.checked_add(cunit.selection_ptr_adjustment)?,
                UnitFacts { unit_type, x: i32::from(x), y: i32::from(y), status },
            ))
        };

        let target = if command.target == 0 {
            None
        } else {
            let Some((pointer, facts)) = resolve(command.target) else {
                return command_result::NOT_EXECUTED;
            };
            Some(TargetFacts {
                unit_type: facts.unit_type,
                x: facts.x,
                y: facts.y,
                selection_pointer: pointer as u64,
            })
        };

        // Resolve every actor first: a command with a missing unit runs none
        // of its parts.
        let mut actors: Vec<(usize, UnitFacts)> = Vec::with_capacity(command.units.len());
        for id in &command.units {
            let Some(actor) = resolve(*id) else {
                return command_result::NOT_EXECUTED;
            };
            actors.push(actor);
        }

        // A command over several units is one order for the whole selection,
        // as the official optimizer runs its groups: the kinds that name a
        // unit read the first actor's record, the others none of it. One
        // unit alone keeps its own record.
        let facts = if actors.len() == 1 || command::reads_front_unit(command.kind) {
            actors[0].1
        } else {
            UnitFacts { unit_type: 0, x: 0, y: 0, status: 0 }
        };
        let Ok(operation) = command::build(&command, &facts, target.as_ref()) else {
            return command_result::REJECTED;
        };
        let Some(select_address) = module.range_address(select_rva, 1) else {
            return command_result::UNAVAILABLE;
        };
        let Some(enqueue_address) = module.range_address(enqueue_rva, 1) else {
            return command_result::UNAVAILABLE;
        };
        let Some(target_order_address) = module.range_address(target_order_rva, 1) else {
            return command_result::UNAVAILABLE;
        };
        let select: unsafe extern "C" fn(u32, *const usize) = std::mem::transmute(select_address);
        let enqueue: unsafe extern "C" fn(*const u8, u32) = std::mem::transmute(enqueue_address);
        let target_order: unsafe extern "C" fn(u16, u16, usize, u16, u8, u8) =
            std::mem::transmute(target_order_address);
        // A merge selects its partner too, which takes one selection place.
        let per_selection = if matches!(operation, command::Operation::Merge { .. }) {
            SELECTION_MAX - 1
        } else {
            SELECTION_MAX
        };
        for group in actors.chunks(per_selection) {
            let mut pointers: Vec<usize> = group.iter().map(|&(pointer, _)| pointer).collect();
            if let command::Operation::Merge { partner_selection_pointer, .. } = operation {
                pointers.push(partner_selection_pointer as usize);
            }
            select(pointers.len() as u32, pointers.as_ptr());
            match operation {
                command::Operation::Merge { opcode, .. } => enqueue(&opcode, 1),
                command::Operation::Packet { bytes, len } => enqueue(bytes.as_ptr(), len as u32),
                command::Operation::TargetOrder { x, y, target_selection_pointer, order, queued } => {
                    target_order(
                        x,
                        y,
                        target_selection_pointer as usize,
                        command::MARKER_NONE,
                        order,
                        u8::from(queued),
                    );
                }
                command::Operation::SmartTarget { x, y, target_selection_pointer, queued } => {
                    let Some(wire) = encode_target(memory, layout, target_selection_pointer) else {
                        return command_result::UNAVAILABLE;
                    };
                    let mut bytes = [0u8; 10];
                    bytes[0] = 0x14;
                    bytes[1..3].copy_from_slice(&x.to_le_bytes());
                    bytes[3..5].copy_from_slice(&y.to_le_bytes());
                    bytes[5..7].copy_from_slice(&wire.to_le_bytes());
                    bytes[7..9].copy_from_slice(&command::MARKER_NONE.to_le_bytes());
                    bytes[9] = u8::from(queued);
                    enqueue(bytes.as_ptr(), bytes.len() as u32);
                }
                command::Operation::UnloadUnit { target_selection_pointer } => {
                    let Some(wire) = encode_target(memory, layout, target_selection_pointer) else {
                        return command_result::UNAVAILABLE;
                    };
                    let [low, high] = wire.to_le_bytes();
                    enqueue([0x29, low, high].as_ptr(), 3);
                }
            }
        }
        command_result::OK
    }

    /// The wire id the game's packets name `selection_pointer`'s unit by.
    unsafe fn encode_target(memory: &InProcessMemory, layout: &Layout, selection_pointer: u64) -> Option<u16> {
        let address = layout.commands.target_encoder.and_then(|rva| memory.main_module().range_address(rva, 1))?;
        let encode: unsafe extern "C" fn(usize) -> u32 = std::mem::transmute(address);
        u16::try_from(encode(selection_pointer as usize)).ok()
    }

    /// The end code the game menu's quit passes to `end_game`.
    const QUIT_MISSION: u32 = 4;
    /// The game's own frame delays (ms) per speed, which a negative speed
    /// restores; the second set the game keeps is three times these.
    const ORIGINAL_SPEED_DELAYS: [u32; 7] = [167, 111, 83, 67, 56, 48, 42];
    const ALT_SPEED_MULTIPLIER: u32 = 3;
    /// `print_text`'s player for a line from no one.
    const PRINT_FROM_NO_ONE: u32 = 16;
    /// The screen update interval no update ever waits out.
    const NO_DRAW_INTERVAL: u32 = u32::MAX;
    /// The game's own screen update cap while SET_GUI 0 holds it: the
    /// interval, the enable byte at bit 32, and bit 40 set while one is kept.
    static KEPT_DRAW_LIMIT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    const KEPT: u64 = 1 << 40;

    /// BWAPI's setGUI. Its huge frame skip would keep the game loop stepping
    /// frames without returning to the window messages the frame hook runs
    /// on, so the screen updates stop instead through the game loop's own
    /// update cap, set to an interval no update waits out. Turning them on
    /// again puts the game's cap back.
    unsafe fn set_gui(enabled: usize, interval: usize, on: bool) {
        let enabled = enabled as *mut u8;
        let interval = interval as *mut u32;
        let held = std::ptr::read_volatile(enabled) == 1 && std::ptr::read_volatile(interval) == NO_DRAW_INTERVAL;
        if !on && !held {
            let kept = KEPT | u64::from(std::ptr::read_volatile(enabled)) << 32 | u64::from(std::ptr::read_volatile(interval));
            KEPT_DRAW_LIMIT.store(kept, std::sync::atomic::Ordering::Relaxed);
            std::ptr::write_volatile(interval, NO_DRAW_INTERVAL);
            std::ptr::write_volatile(enabled, 1);
        } else if on && held {
            // Without a kept cap (the game reinstalls its own when a game
            // loop starts) the cap goes off.
            let kept = KEPT_DRAW_LIMIT.swap(0, std::sync::atomic::Ordering::Relaxed);
            let (cap, every) = if kept & KEPT != 0 { ((kept >> 32) as u8, kept as u32) } else { (0, NO_DRAW_INTERVAL) };
            std::ptr::write_volatile(enabled, cap);
            std::ptr::write_volatile(interval, every);
        }
    }

    /// The game's string: data, length, capacity, all pointer-wide, then a
    /// 16-byte inline buffer. The top capacity bit marks a buffer the caller
    /// owns, so the game neither frees nor grows it.
    const CALLER_OWNED: usize = 1 << (usize::BITS - 1);
    #[repr(C)]
    struct GameString {
        data: *const u8,
        length: usize,
        capacity: usize,
        inline_buffer: [u8; 16],
    }

    /// Runs a game command on the game thread the way BWAPI's GameImpl does:
    /// pause, resume, restart, alliance, vision and minimap pings as command
    /// packets, leave by ending the game loop, speed and frame skip through
    /// the game's delay tables, text through the game's message and chat
    /// functions, replay vision and the map file through the game's own
    /// fields.
    unsafe fn execute_game(
        memory: &InProcessMemory,
        layout: &Layout,
        basis: &CommandBasis,
        command: &UnitCommand,
        text: *const u8,
    ) -> u32 {
        use bwrapi_engine::command::game_command_kind::*;

        // BWAPI's server sets the map outside a match too.
        if command.kind == SET_MAP {
            return set_map(memory, layout, &command.text);
        }
        let module = memory.main_module();
        let in_game = module
            .range_address(layout.scalars.session_is_in_game_offset, 4)
            .and_then(|address| read_u32(memory, address))
            .is_some_and(|value| value != 0);
        if !in_game {
            return command_result::UNAVAILABLE;
        }
        let address = |rva: Option<usize>, bytes: usize| rva.and_then(|rva| module.range_address(rva, bytes));
        let commands = &layout.commands;
        let enqueue = |bytes: &[u8]| {
            let Some(enqueue) = address(commands.enqueue, 1) else {
                return command_result::UNAVAILABLE;
            };
            let enqueue: unsafe extern "C" fn(*const u8, u32) = std::mem::transmute(enqueue);
            enqueue(bytes.as_ptr(), bytes.len() as u32);
            command_result::OK
        };
        if let Some(opcode) = command::game_packet(command.kind) {
            return enqueue(&[opcode]);
        }
        let replay = module
            .range_address(layout.scalars.session_is_replay_offset, 4)
            .and_then(|address| read_u32(memory, address))
            == Some(1);
        // Player ids are the engine's (the identity table's actual ids); the
        // game's alliance and vision masks are indexed by identity slot.
        let player = command.x as u8;
        match command.kind {
            PING_MINIMAP => return enqueue(&command::minimap_ping_packet(command.x, command.y)),
            // The game takes cheats outside network matches only.
            USE_CHEAT if replay || in_network_match(memory, layout) => return command_result::REJECTED,
            USE_CHEAT => return enqueue(&command::cheat_packet(command.x as u32)),
            // BWAPI: no alliances in a replay, and none with oneself.
            SET_ALLIES if replay || basis.local_player >= 8 || player == basis.local_player => {
                return command_result::REJECTED;
            }
            SET_VISION if !replay && (basis.local_player >= 8 || player == basis.local_player) => {
                return command_result::REJECTED;
            }
            SET_ALLIES | SET_VISION if !replay => {
                let Some(state) = control_state(memory, layout, basis.local_player) else {
                    return command_result::UNAVAILABLE;
                };
                let Some(slot) = state.target_slots[usize::from(player)] else {
                    return command_result::NOT_EXECUTED;
                };
                let slot = usize::from(slot);
                return if command.kind == SET_ALLIES {
                    enqueue(&command::alliance_packet(state.alliance_mask, slot, command.y as u8))
                } else {
                    enqueue(&command::vision_packet(state.vision_mask, slot, command.y != 0))
                };
            }
            SET_VISION => {
                let (Some(replay_vision), Some(local_vision)) =
                    (address(commands.replay_vision, 1), address(commands.local_vision, 1))
                else {
                    return command_result::UNAVAILABLE;
                };
                let Some(bindings) = read_identities(memory, layout).and_then(|raw| scalars::read_player_slot_bindings(&raw))
                else {
                    return command_result::UNAVAILABLE;
                };
                let Some(binding) = bindings.iter().find(|binding| binding.actual_player_id() == player) else {
                    return command_result::NOT_EXECUTED;
                };
                let bit = 1u8 << binding.slot_ordinal;
                let current = std::ptr::read_volatile(replay_vision as *const u8);
                let next = if command.y != 0 { current | bit } else { current & !bit };
                std::ptr::write_volatile(replay_vision as *mut u8, next);
                std::ptr::write_volatile(local_vision as *mut u8, next);
                return command_result::OK;
            }
            // BWAPI: replays only.
            SET_REVEAL_ALL if !replay => return command_result::REJECTED,
            SET_REVEAL_ALL => {
                let Some(reveal) = address(commands.replay_reveal_all, 4) else {
                    return command_result::UNAVAILABLE;
                };
                std::ptr::write_volatile(reveal as *mut u32, command.x as u32);
                return command_result::OK;
            }
            _ => {}
        }
        match command.kind {
            LEAVE => {
                let Some(end_game) = address(commands.end_game, 1) else {
                    return command_result::UNAVAILABLE;
                };
                let end_game: unsafe extern "C" fn(u32) = std::mem::transmute(end_game);
                end_game(QUIT_MISSION);
            }
            SET_SPEED => {
                let (Some(recompute), Some(alt)) = (
                    address(commands.recompute_turn_durations, 1),
                    address(commands.alt_speed_modifiers, 4 * ORIGINAL_SPEED_DELAYS.len()),
                ) else {
                    return command_result::UNAVAILABLE;
                };
                let delays = if command.x < 0 { ORIGINAL_SPEED_DELAYS } else { [command.x as u32; 7] };
                let recompute: unsafe extern "C" fn(*const u32) = std::mem::transmute(recompute);
                recompute(delays.as_ptr());
                for (index, delay) in delays.iter().enumerate() {
                    std::ptr::write_volatile((alt as *mut u32).add(index), delay.wrapping_mul(ALT_SPEED_MULTIPLIER));
                }
            }
            SET_GUI => {
                let (Some(enabled), Some(interval)) =
                    (address(commands.draw_limit_enabled, 1), address(commands.draw_limit_interval, 4))
                else {
                    return command_result::UNAVAILABLE;
                };
                set_gui(enabled, interval, command.x != 0);
            }
            SET_FRAME_SKIP => {
                let Some(step) = address(commands.step_game_frames, 4) else {
                    return command_result::UNAVAILABLE;
                };
                std::ptr::write_volatile(step as *mut i32, command.x);
            }
            SET_SCREEN_POSITION => {
                let (Some(move_screen), Some(max_x), Some(max_y)) = (
                    address(commands.move_screen, 1),
                    address(commands.screen_max_x, 4).and_then(|address| read_u32(memory, address)),
                    address(commands.screen_max_y, 4).and_then(|address| read_u32(memory, address)),
                ) else {
                    return command_result::UNAVAILABLE;
                };
                // The game moves the screen in steps of 8 pixels, within the map.
                let x = (command.x.max(0) as u32).min(max_x) & !7;
                let y = (command.y.max(0) as u32).min(max_y) & !7;
                let move_screen: unsafe extern "C" fn(u32, u32) = std::mem::transmute(move_screen);
                move_screen(x, y);
            }
            PRINT_TEXT => {
                let Some(print) = address(commands.print_text, 1) else {
                    return command_result::UNAVAILABLE;
                };
                let print: unsafe extern "C" fn(*const u8, u32, u32) = std::mem::transmute(print);
                print(text, PRINT_FROM_NO_ONE, 0);
            }
            SEND_TEXT if !in_network_match(memory, layout) => {
                // Outside a network match there is no chat to send to; the
                // line shows as the local player's, as the game shows typed
                // single-player text.
                let Some(print) = address(commands.print_text, 1) else {
                    return command_result::UNAVAILABLE;
                };
                let print: unsafe extern "C" fn(*const u8, u32, u32) = std::mem::transmute(print);
                let player = if basis.local_player < 8 { u32::from(basis.local_player) } else { PRINT_FROM_NO_ONE };
                print(text, player, 0);
            }
            SEND_TEXT => {
                let (Some(send), Some(chat)) = (address(commands.send_text, 1), commands.chat_context) else {
                    return command_result::UNAVAILABLE;
                };
                let word = |address: usize| read_word(memory, layout, address);
                let context = address(Some(chat.pointer), layout.pointer_bytes)
                    .and_then(word)
                    .and_then(|root| ((root as u64 ^ chat.xor_key) as usize).checked_add(chat.offset))
                    .and_then(word)
                    .filter(|context| *context != 0 && word(*context).is_some());
                let Some(context) = context else {
                    return command_result::UNAVAILABLE;
                };
                let channel: &[u8] = if command.type_id == 1 { b"InGameAllies" } else { b"InGameAll" };
                let mut recipient = GameString {
                    data: std::ptr::null(),
                    length: channel.len(),
                    capacity: CALLER_OWNED | 15,
                    inline_buffer: [0; 16],
                };
                recipient.inline_buffer[..channel.len()].copy_from_slice(channel);
                recipient.data = recipient.inline_buffer.as_ptr();
                let line = GameString {
                    data: text,
                    length: command.text.len(),
                    capacity: CALLER_OWNED | super::COMMAND_TEXT_MAX,
                    inline_buffer: [0; 16],
                };
                let send: member_fn!(type (usize, *const GameString, *const GameString)) = std::mem::transmute(send);
                send(context, &recipient, &line);
            }
            _ => return command_result::REJECTED,
        }
        command_result::OK
    }

    /// BWAPI's setMap: a non-empty name of a file the game can open, copied
    /// into the BWGame map file name the next restart loads.
    unsafe fn set_map(memory: &InProcessMemory, layout: &Layout, path: &[u8]) -> u32 {
        use bwrapi_engine::decode::bwgame::{BWGAME_MAP_FILE_NAME_BYTES, BWGAME_MAP_FILE_NAME_OFFSET};
        let Ok(name) = std::str::from_utf8(path) else {
            return command_result::REJECTED;
        };
        if path.len() >= BWGAME_MAP_FILE_NAME_BYTES || std::fs::File::open(name).is_err() {
            return command_result::REJECTED;
        }
        let Some(field) = bwgame_root(memory, layout)
            .and_then(|root| root.checked_add(BWGAME_MAP_FILE_NAME_OFFSET))
            .filter(|field| memory.read_into(*field, &mut [0; BWGAME_MAP_FILE_NAME_BYTES]).is_ok())
        else {
            return command_result::UNAVAILABLE;
        };
        std::ptr::write_bytes(field as *mut u8, 0, BWGAME_MAP_FILE_NAME_BYTES);
        std::ptr::copy_nonoverlapping(path.as_ptr(), field as *mut u8, path.len());
        command_result::OK
    }

    /// The BWGame root: `read(pointer) ^ xor_key`, as the engine reads it.
    fn bwgame_root(memory: &InProcessMemory, layout: &Layout) -> Option<usize> {
        let pointer = memory.main_module().range_address(layout.menu_root.pointer, layout.pointer_bytes)?;
        let root = (read_word(memory, layout, pointer)? as u64 ^ layout.menu_root.xor_key) as usize;
        (root != 0).then_some(root)
    }

    fn read_identities(memory: &InProcessMemory, layout: &Layout) -> Option<Vec<u8>> {
        let mut raw = vec![0; scalars::PLAYER_IDENTITY_TABLE_BYTES];
        let address = memory.main_module().range_address(layout.scalars.player_identity_table_offset, raw.len())?;
        memory.read_into(address, &mut raw).ok()?;
        Some(raw)
    }

    /// The local player's alliance row and vision mask with every player's
    /// slot, read as BWAPI reads them right before it sends a change.
    fn control_state(memory: &InProcessMemory, layout: &Layout, local_player: u8) -> Option<bwgame::BwgameCommandControlState> {
        let identities = read_identities(memory, layout)?;
        let bindings = scalars::read_player_slot_bindings(&identities)?;
        let mut raw = vec![0; bwgame::BWGAME_METADATA_READ_BYTES];
        memory.read_into(bwgame_root(memory, layout)?, &mut raw).ok()?;
        bwgame::read_bwgame_command_control_state(&raw, &bindings, &identities, local_player)
    }

    fn in_network_match(memory: &InProcessMemory, layout: &Layout) -> bool {
        memory
            .main_module()
            .range_address(layout.scalars.session_network_in_match_offset, 1)
            .and_then(|address| read_u8(memory, address))
            .is_some_and(|value| value != 0)
    }

    /// A pointer-wide word of the game.
    fn read_word(memory: &InProcessMemory, layout: &Layout, address: usize) -> Option<usize> {
        let mut raw = [0; 8];
        let raw = &mut raw[..layout.pointer_bytes];
        memory.read_into(address, raw).ok()?;
        bwrapi_layout::read_pointer(raw, 0, layout.pointer_bytes)
    }

    fn read_u8(memory: &InProcessMemory, address: usize) -> Option<u8> {
        let mut raw = [0; 1];
        memory.read_into(address, &mut raw).ok()?;
        Some(raw[0])
    }

    fn read_u16(memory: &InProcessMemory, address: usize) -> Option<u16> {
        let mut raw = [0; 2];
        memory.read_into(address, &mut raw).ok()?;
        Some(u16::from_le_bytes(raw))
    }

    fn read_u32(memory: &InProcessMemory, address: usize) -> Option<u32> {
        let mut raw = [0; 4];
        memory.read_into(address, &mut raw).ok()?;
        Some(u32::from_le_bytes(raw))
    }
}

#[cfg(windows)]
pub use shared::GameCommands;
#[cfg(windows)]
pub(crate) use shared::{create_commands, execute};

#[cfg(test)]
mod tests {
    use super::*;
    use bwrapi_engine::command;

    fn block() -> Box<CommandsBlock> {
        let block = Box::new(unsafe { std::mem::zeroed::<CommandsBlock>() });
        init(&block);
        block
    }

    fn record(id: u64, kind: u32) -> CommandRecord {
        let mut record = CommandRecord { id, kind, unit_count: 1, ..CommandRecord::default() };
        record.units[0] = 1;
        record
    }

    #[test]
    fn block_layout_is_pinned() {
        use std::mem::{offset_of, size_of};
        assert_eq!(size_of::<CommandRecord>(), 240);
        assert_eq!(offset_of!(CommandRecord, id), 0);
        assert_eq!(offset_of!(CommandRecord, units), 40);
        assert_eq!(offset_of!(CommandRecord, unit_count), 232);
        assert_eq!(size_of::<CommandSlot>(), 248);
        assert_eq!(offset_of!(CommandSlot, command), 8);
        assert_eq!(size_of::<ResultSlot>(), 16);
        // The result id closes the slot.
        assert_eq!(offset_of!(ResultSlot, id), 8);
        // The basis sequence closes the slot.
        assert_eq!(offset_of!(BasisSlot, sequence), 0);
        assert_eq!(size_of::<BasisSlot>(), 40);
        assert_eq!(offset_of!(CommandsBlock, next_id), 8);
        assert_eq!(offset_of!(CommandsBlock, tail), 16);
        assert_eq!(offset_of!(CommandsBlock, head), 24);
        assert_eq!(offset_of!(CommandsBlock, basis), 32);
        assert_eq!(offset_of!(CommandsBlock, slots), 72);
        assert_eq!(offset_of!(CommandsBlock, results), 72 + COMMAND_RING_CAPACITY * 248);
        assert_eq!(
            size_of::<CommandsBlock>(),
            72 + COMMAND_RING_CAPACITY * 248 + RESULT_RING_CAPACITY * 16
        );
    }

    #[test]
    fn the_basis_travels_with_the_commands_that_read_it() {
        let block = block();
        assert!(take_basis(&block).is_none());
        let basis = CommandBasis {
            root: 0x1000,
            pool_base: 0x2000,
            local_player: u8::MAX,
            game_frame: 900,
            match_id: 3,
        };
        publish_basis(&block, &basis);
        assert_eq!(take_basis(&block), Some(basis));
        let newer = CommandBasis { root: 0x3000, game_frame: 901, ..basis };
        publish_basis(&block, &newer);
        assert_eq!(take_basis(&block), Some(newer));
    }

    #[test]
    fn commands_name_is_per_process() {
        let name = String::from_utf16(&commands_name(4321)).unwrap();
        assert_eq!(name, "Local\\bwrapi_game_commands_4321\0");
    }

    #[test]
    fn records_round_trip_through_the_ring() {
        let block = block();
        assert!(try_push(&block, record(1, command::command_kind::MOVE)));
        let popped = pop(&block).unwrap();
        assert_eq!((popped.id, popped.kind), (1, command::command_kind::MOVE));
        assert!(pop(&block).is_none());
        assert!(!has_commands(&block));
    }

    #[test]
    fn a_full_queue_refuses_until_the_consumer_advances() {
        let block = block();
        for index in 0..COMMAND_RING_CAPACITY as u64 {
            assert!(try_push(&block, record(index, 0)), "push {index} of a full-capacity run");
        }
        assert!(!try_push(&block, record(999, 0)));
        assert!(has_commands(&block));
        assert_eq!(pop(&block).unwrap().id, 0);
        // The freed slot serves queue position `capacity`.
        assert!(try_push(&block, record(999, 0)));
        for index in 1..COMMAND_RING_CAPACITY as u64 {
            assert_eq!(pop(&block).unwrap().id, index);
        }
        assert_eq!(pop(&block).unwrap().id, 999);
        assert!(pop(&block).is_none());
    }

    #[test]
    fn concurrent_producers_deliver_every_record_once() {
        // The address travels as a plain integer; the ring's own sequence
        // atomics order every access to the block.
        let block = block();
        let raw = (&*block as *const CommandsBlock) as usize;
        std::thread::scope(|scope| {
            for producer in 0..8u64 {
                scope.spawn(move || {
                    let block = unsafe { &*(raw as *const CommandsBlock) };
                    for index in 0..100u64 {
                        let id = producer * 100 + index;
                        while !try_push(block, record(id, 0)) {
                            std::thread::yield_now();
                        }
                    }
                });
            }
            let consumer = unsafe { &*(raw as *const CommandsBlock) };
            let mut seen = std::collections::HashSet::new();
            while seen.len() < 800 {
                match pop(consumer) {
                    Some(record) => {
                        assert!(seen.insert(record.id), "record {} delivered twice", record.id);
                    }
                    None => std::thread::yield_now(),
                }
            }
        });
        assert!(pop(&block).is_none());
    }

    #[test]
    fn results_carry_their_id_and_forget_overwritten_ones() {
        let block = block();
        assert!(read_result(&block, 7).is_none());
        publish_result(&block, 7, command_result::OK, 42);
        assert_eq!(read_result(&block, 7), Some((command_result::OK, 42)));
        publish_result(&block, 7, command_result::REJECTED, 43);
        assert_eq!(read_result(&block, 7), Some((command_result::REJECTED, 43)));
        // The same slot serves id + capacity; the old id reads as absent.
        let overwritten = 7 + RESULT_RING_CAPACITY as u64;
        publish_result(&block, overwritten, command_result::OK, 50);
        assert!(read_result(&block, 7).is_none());
        assert_eq!(read_result(&block, overwritten), Some((command_result::OK, 50)));
    }

    #[test]
    fn records_and_commands_convert_without_loss() {
        let command = UnitCommand {
            kind: command::command_kind::GATHER,
            queued: true,
            tile: false,
            type_id: 9,
            slot: 2,
            x: 30,
            y: 40,
            target: 5,
            units: vec![1, 2, 3],
            text: Vec::new(),
        };
        let record = record_of(11, &command).unwrap();
        assert_eq!((record.id, record.flags, record.unit_count), (11, 1, 3));
        assert_eq!(unit_command(&record), command);
        let mut oversized = command;
        oversized.units = (1..=COMMAND_MAX_UNITS as u64 + 1).collect();
        assert!(record_of(12, &oversized).is_none());
    }

    #[test]
    fn text_travels_nul_terminated_up_to_its_limit() {
        let longest = vec![b'a'; COMMAND_TEXT_MAX];
        let command = UnitCommand {
            kind: command::game_command_kind::PRINT_TEXT,
            text: longest.clone(),
            ..UnitCommand::default()
        };
        let record = record_of(1, &command).unwrap();
        assert_eq!(unit_command(&record).text, longest);
        let raw = unsafe { std::slice::from_raw_parts(record_text_ptr(&record), COMMAND_TEXT_MAX + 1) };
        assert_eq!(raw[COMMAND_TEXT_MAX], 0);
        let too_long = UnitCommand { text: vec![b'a'; COMMAND_TEXT_MAX + 1], ..command };
        assert!(record_of(2, &too_long).is_none());
    }
}
