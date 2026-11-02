//! Repair and refund totals per player, counted where the game moves the
//! minerals and gas — the same instructions BWAPI 4.4 hooks. The game's code
//! cannot be patched, so the page holding the players' minerals and gas is
//! made read-only for the match: every write to it faults into the vectored
//! handler here, which runs that one instruction out of line (`instruction`)
//! with the page writable. The relocated copy then calls back into this
//! module, which puts the protection back and adds the change to the
//! player's totals when the instruction is a repair payment or a refund,
//! and the game continues after the original instruction. No single-step
//! trap is used: the x86 build's network games stop when the game thread
//! takes single-step exceptions. The page lies in the game's heap, so its
//! protection can change.
//!
//! The watch is on only while a match runs; outside a match the page keeps
//! its own protection, so loading and leaving touch nothing of this module.

use std::cell::Cell;
use std::sync::atomic::Ordering::{AcqRel, Acquire, Relaxed, Release};
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicUsize};
use std::sync::OnceLock;

use bwrapi_engine::decode::bwgame::{BWGAME_GAS_OFFSET, BWGAME_MINERALS_OFFSET};
use bwrapi_engine::{ResourceTotals, RESOURCE_SLOTS};
use bwrapi_layout::Layout;
use windows_sys::Win32::Foundation::EXCEPTION_ACCESS_VIOLATION;
use windows_sys::Win32::System::Diagnostics::Debug::{
    AddVectoredExceptionHandler, FlushInstructionCache, CONTEXT, EXCEPTION_POINTERS,
};
use windows_sys::Win32::System::Memory::{
    VirtualAlloc, VirtualProtect, VirtualQuery, MEMORY_BASIC_INFORMATION, MEM_COMMIT, MEM_RESERVE,
    PAGE_EXECUTE_READ, PAGE_EXECUTE_READWRITE, PAGE_READONLY,
};
use windows_sys::Win32::System::Threading::GetCurrentProcess;

use crate::instruction::{exit_code, relocatable_length, relocated_code};

const PAGE: usize = 0x1000;
const WIDE: bool = cfg!(target_pointer_width = "64");
/// Bytes from the BWGame root the watched arrays span: minerals then gas.
const WATCHED_BYTES: usize = BWGAME_GAS_OFFSET + RESOURCE_SLOTS * 4;
const EXCEPTION_CONTINUE_EXECUTION: i32 = -1;
const EXCEPTION_CONTINUE_SEARCH: i32 = 0;
/// Executable memory for the relocated instructions: the shared exit, then
/// one fixed slot per game instruction that wrote the page. A match sees a
/// handful of such instructions.
const CODE_BYTES: usize = 0x10000;
const EXIT_BYTES: usize = 0x80;
const SLOT_BYTES: usize = 0x40;
const SLOTS: usize = (CODE_BYTES - EXIT_BYTES) / SLOT_BYTES;
/// The longest x86 instruction.
const LONGEST: usize = 15;

/// The watched pages, or 0 while no match runs.
static FIRST_PAGE: AtomicUsize = AtomicUsize::new(0);
static END_PAGE: AtomicUsize = AtomicUsize::new(0);
static ROOT: AtomicUsize = AtomicUsize::new(0);
/// The pages' own protection, put back when the watch ends.
static OWN_PROTECTION: AtomicUsize = AtomicUsize::new(0);
/// Set when a write the handler cannot relocate left the pages writable;
/// the next frame boundary protects them again.
static OPEN: AtomicBool = AtomicBool::new(false);
/// The counted instructions and the relocation memory, set up when the
/// handler is registered.
static SITES: OnceLock<Sites> = OnceLock::new();
/// Game instruction addresses with a relocated copy, in slot order.
static RELOCATED: [AtomicUsize; SLOTS] = [const { AtomicUsize::new(0) }; SLOTS];
static RELOCATED_COUNT: AtomicUsize = AtomicUsize::new(0);
static RELOCATING: AtomicBool = AtomicBool::new(false);
/// This match's totals by BWGame player slot: repaired minerals, repaired
/// gas, refunded minerals, refunded gas.
static TOTALS: [[AtomicI32; 4]; RESOURCE_SLOTS] = [const { [const { AtomicI32::new(0) }; 4] }; RESOURCE_SLOTS];

struct Sites {
    module: usize,
    repairs: &'static [usize],
    refunds: &'static [usize],
    /// The relocation memory, 0 when it could not be allocated.
    code: usize,
}

impl Sites {
    /// 0 repair, 2 refund (the first totals column the kind adds to).
    fn kind(&self, address: usize) -> Option<usize> {
        let rva = address.checked_sub(self.module)?;
        if self.repairs.contains(&rva) {
            Some(0)
        } else if self.refunds.contains(&rva) {
            Some(2)
        } else {
            None
        }
    }

    /// The relocated copy of the game instruction at `address`, made on its
    /// first write; `None` when it cannot run elsewhere.
    unsafe fn relocated(&self, address: usize) -> Option<usize> {
        let slot_address = |slot: usize| self.code + EXIT_BYTES + slot * SLOT_BYTES;
        let find = || (0..RELOCATED_COUNT.load(Acquire)).find(|&slot| RELOCATED[slot].load(Relaxed) == address);
        if let Some(slot) = find() {
            return Some(slot_address(slot));
        }
        if self.code == 0 {
            return None;
        }
        while RELOCATING.compare_exchange(false, true, Acquire, Relaxed).is_err() {
            std::hint::spin_loop();
        }
        let result = find().map(slot_address).or_else(|| {
            let slot = RELOCATED_COUNT.load(Relaxed);
            // The instruction may end near its region's end; read no further.
            let mut region: MEMORY_BASIC_INFORMATION = std::mem::zeroed();
            if VirtualQuery(address as _, &mut region, std::mem::size_of::<MEMORY_BASIC_INFORMATION>()) == 0 {
                return None;
            }
            let region_end = region.BaseAddress as usize + region.RegionSize;
            let bytes = std::slice::from_raw_parts(address as *const u8, LONGEST.min(region_end - address));
            let length = relocatable_length(bytes, WIDE).filter(|_| slot < SLOTS)?;
            let at = slot_address(slot);
            let code = relocated_code(&bytes[..length], at, self.code, address + length, WIDE);
            write_code(at, &code)?;
            RELOCATED[slot].store(address, Relaxed);
            RELOCATED_COUNT.store(slot + 1, Release);
            Some(at)
        });
        RELOCATING.store(false, Release);
        result
    }
}

/// Writes code into the relocation memory, which stays executable for
/// other threads running it meanwhile.
unsafe fn write_code(at: usize, code: &[u8]) -> Option<()> {
    let start = at & !(PAGE - 1);
    let end = (at + code.len() + PAGE - 1) & !(PAGE - 1);
    let mut previous = 0;
    if VirtualProtect(start as _, end - start, PAGE_EXECUTE_READWRITE, &mut previous) == 0 {
        return None;
    }
    std::ptr::copy_nonoverlapping(code.as_ptr(), at as *mut u8, code.len());
    VirtualProtect(start as _, end - start, PAGE_EXECUTE_READ, &mut previous);
    FlushInstructionCache(GetCurrentProcess(), at as _, code.len());
    Some(())
}

/// A faulted write waiting for its relocated copy to run: the instruction
/// and the arrays before it ran.
#[derive(Clone, Copy)]
struct Pending {
    address: usize,
    before: [i32; RESOURCE_SLOTS * 2],
}

thread_local! {
    static PENDING: Cell<Option<Pending>> = const { Cell::new(None) };
}

/// At the frame boundary, on the game thread: watches the arrays of the
/// match's BWGame root, or stops watching when no match runs (`None`). A new
/// root is a new match, whose totals start at zero.
pub(crate) fn watch(module: usize, layout: &Layout, root: Option<usize>) {
    let Some(sites) = layout.resource_writes else { return };
    let current = ROOT.load(Acquire);
    if root == Some(current) {
        if OPEN.swap(false, AcqRel) {
            protect(PAGE_READONLY);
        }
        return;
    }
    if current != 0 {
        stop();
    }
    let Some(root) = root else { return };
    SITES.get_or_init(|| unsafe {
        let code = VirtualAlloc(std::ptr::null(), CODE_BYTES, MEM_COMMIT | MEM_RESERVE, PAGE_EXECUTE_READ) as usize;
        let code = if code != 0 && write_code(code, &exit_code(after_write as extern "C" fn() as usize, WIDE)).is_some() { code } else { 0 };
        AddVectoredExceptionHandler(1, Some(handler));
        Sites { module, repairs: sites.repairs, refunds: sites.refunds, code }
    });
    for player in &TOTALS {
        for total in player {
            total.store(0, Relaxed);
        }
    }
    let first = root & !(PAGE - 1);
    let end = (root + WATCHED_BYTES + PAGE - 1) & !(PAGE - 1);
    let mut own = 0;
    if unsafe { VirtualProtect(first as _, end - first, PAGE_READONLY, &mut own) } == 0 {
        return;
    }
    OWN_PROTECTION.store(own as usize, Relaxed);
    FIRST_PAGE.store(first, Relaxed);
    END_PAGE.store(end, Relaxed);
    ROOT.store(root, Release);
}

/// This match's totals; `None` while no match is watched.
pub(crate) fn totals() -> Option<[ResourceTotals; RESOURCE_SLOTS]> {
    if ROOT.load(Acquire) == 0 {
        return None;
    }
    Some(std::array::from_fn(|slot| {
        let [repaired_minerals, repaired_gas, refunded_minerals, refunded_gas] =
            TOTALS[slot].each_ref().map(|total| total.load(Relaxed));
        ResourceTotals { repaired_minerals, repaired_gas, refunded_minerals, refunded_gas }
    }))
}

fn stop() {
    protect(OWN_PROTECTION.load(Relaxed) as u32);
    ROOT.store(0, Release);
    FIRST_PAGE.store(0, Relaxed);
    END_PAGE.store(0, Relaxed);
}

fn protect(protection: u32) {
    let (first, end) = (FIRST_PAGE.load(Relaxed), END_PAGE.load(Relaxed));
    let mut previous = 0;
    unsafe { VirtualProtect(first as _, end - first, protection, &mut previous) };
}

fn arrays(root: usize) -> [i32; RESOURCE_SLOTS * 2] {
    let minerals = (root + BWGAME_MINERALS_OFFSET) as *const i32;
    let gas = (root + BWGAME_GAS_OFFSET) as *const i32;
    std::array::from_fn(|index| unsafe {
        if index < RESOURCE_SLOTS {
            minerals.add(index).read_volatile()
        } else {
            gas.add(index - RESOURCE_SLOTS).read_volatile()
        }
    })
}

unsafe extern "system" fn handler(info: *mut EXCEPTION_POINTERS) -> i32 {
    let record = &*(*info).ExceptionRecord;
    let context = &mut *(*info).ContextRecord;
    let root = ROOT.load(Acquire);
    // A write (operation 1) into a watched page.
    if record.ExceptionCode != EXCEPTION_ACCESS_VIOLATION
        || root == 0
        || record.NumberParameters < 2
        || record.ExceptionInformation[0] != 1
        || !(FIRST_PAGE.load(Relaxed)..END_PAGE.load(Relaxed)).contains(&record.ExceptionInformation[1])
    {
        return EXCEPTION_CONTINUE_SEARCH;
    }
    let address = instruction(context);
    let own = OWN_PROTECTION.load(Relaxed) as u32;
    match SITES.get().and_then(|sites| sites.relocated(address)) {
        Some(relocated) => {
            PENDING.set(Some(Pending { address, before: arrays(root) }));
            protect(own);
            set_instruction(context, relocated);
        }
        // Not relocatable: the write runs in place and the pages stay
        // writable, uncounted, until the frame boundary.
        None => {
            OPEN.store(true, Release);
            protect(own);
        }
    }
    EXCEPTION_CONTINUE_EXECUTION
}

/// Called by the shared exit right after a relocated game write ran, on the
/// game's thread: counts it and protects the pages again.
extern "C" fn after_write() {
    let Some(pending) = PENDING.take() else { return };
    let root = ROOT.load(Acquire);
    if root == 0 {
        return;
    }
    if let Some(kind) = SITES.get().and_then(|sites| sites.kind(pending.address)) {
        let after = arrays(root);
        for index in 0..RESOURCE_SLOTS * 2 {
            let (slot, gas) = (index % RESOURCE_SLOTS, index / RESOURCE_SLOTS);
            let change = after[index].wrapping_sub(pending.before[index]);
            // A repair takes from the player, a refund gives back.
            let amount = if kind == 0 { change.wrapping_neg() } else { change };
            if change != 0 {
                TOTALS[slot][kind + gas].fetch_add(amount, Relaxed);
            }
        }
    }
    protect(PAGE_READONLY);
}

#[cfg(target_pointer_width = "64")]
fn instruction(context: &CONTEXT) -> usize {
    context.Rip as usize
}

#[cfg(target_pointer_width = "64")]
fn set_instruction(context: &mut CONTEXT, address: usize) {
    context.Rip = address as u64;
}

#[cfg(target_pointer_width = "32")]
fn instruction(context: &CONTEXT) -> usize {
    context.Eip as usize
}

#[cfg(target_pointer_width = "32")]
fn set_instruction(context: &mut CONTEXT, address: usize) {
    context.Eip = address as u32;
}
