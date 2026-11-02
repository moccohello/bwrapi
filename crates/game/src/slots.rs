//! The hook table: function-pointer slots in memory the game itself writes
//! (the chat dialog's event handler, the renderer's vtable pointer), replaced
//! with handlers of this DLL while the game's code is not touched. Each entry
//! knows how to find its slot, the handler that belongs in it, and the game
//! function it displaced, which its handler calls through. [`sync`] walks the
//! table on the game thread, so it does not matter when the game builds the
//! object a slot lives in.

#![cfg_attr(all(not(windows), not(test)), allow(dead_code))]

use std::sync::atomic::{AtomicUsize, Ordering::Release};

use bwrapi_layout::{read_pointer, Layout};
use bwrapi_memory::{InProcessMemory, ProcessMemory};

/// One function-pointer slot of the game's writable memory the table
/// replaces: how to find the slot, the handler that belongs in it, and the
/// game function it displaced — the value its handler calls through for
/// everything it does not take over.
pub(crate) struct Slot {
    /// Locates the slot; `None` while the object holding it does not exist.
    find: fn(&InProcessMemory, &Layout) -> Option<usize>,
    /// The value the slot must hold, given the value it holds now: the
    /// address of a handler (a function address is not a constant, so the
    /// static table names it through a function), or of a vtable copy built
    /// from the game's vtable the slot points at.
    hook: fn(usize) -> usize,
    /// The game function the slot held when it was last replaced.
    original: AtomicUsize,
}

/// Fits every slot to its handler on the game thread: a slot whose value is
/// not the handler holds a game function (the object was just built, or
/// rebuilt), which is saved and replaced; a slot that cannot be found is
/// skipped.
pub(crate) fn sync(memory: &InProcessMemory, layout: &Layout, slots: &[Slot]) {
    for slot in slots {
        fit(slot, memory, layout);
    }
}

fn fit(slot: &Slot, memory: &InProcessMemory, layout: &Layout) {
    let width = layout.pointer_bytes;
    let Some(address) = (slot.find)(memory, layout) else { return };
    let Some(current) = read_slot(memory, address, width) else { return };
    let hook = (slot.hook)(current);
    if current == hook {
        return;
    }
    slot.original.store(current, Release);
    // The slot is writable game memory and this runs on the game thread,
    // between the game's own uses of it.
    write_slot(address, hook, width);
}

fn read_slot(memory: &InProcessMemory, address: usize, width: usize) -> Option<usize> {
    let mut raw = [0; 8];
    memory.read_into(address, &mut raw[..width]).ok()?;
    read_pointer(&raw, 0, width)
}

fn write_slot(address: usize, value: usize, width: usize) {
    match width {
        4 => unsafe { std::ptr::write_volatile(address as *mut u32, value as u32) },
        _ => unsafe { std::ptr::write_volatile(address as *mut u64, value as u64) },
    }
}

/// The table's game-facing entries.
#[cfg(windows)]
mod game {
    use std::panic::{self, AssertUnwindSafe};
    use std::sync::atomic::{AtomicUsize, Ordering::Acquire};

    use bwrapi_layout::{read_pointer, ChatLayout, Layout};
    use bwrapi_memory::{InProcessMemory, ProcessMemory};

    use super::Slot;
    use crate::chat::{capturing, chat_kind, push, typed_line, ChatLine};
    use crate::hook;

    /// The hook table.
    pub(crate) static TABLE: [Slot; 2] = [
        Slot { find: chat_event_slot, hook: |_| chat_event as *const () as usize, original: AtomicUsize::new(0) },
        Slot {
            find: crate::draw::renderer_slot,
            hook: crate::draw::renderer_vtable,
            original: AtomicUsize::new(0),
        },
    ];

    /// The chat dialog's event-handler slot: the match's dialog object
    /// holds the handler its events go to at `dialog_event`. The dialog
    /// global is null outside a match, so the entry is skipped there.
    fn chat_event_slot(memory: &InProcessMemory, layout: &Layout) -> Option<usize> {
        let chat = layout.chat.as_ref()?;
        let at = memory.main_module().range_address(chat.dialog, layout.pointer_bytes)?;
        let mut raw = [0; 8];
        memory.read_into(at, &mut raw[..layout.pointer_bytes]).ok()?;
        let dialog = read_pointer(&raw, 0, layout.pointer_bytes).filter(|dialog| *dialog != 0)?;
        dialog.checked_add(chat.dialog_event)
    }

    /// The chat dialog's event handler the table installs. While capture is
    /// on, an Enter with a typed line is taken for the server — the line is
    /// queued and the box closed without sending it — and the event counts
    /// as handled; everything else runs the saved game handler unchanged.
    unsafe extern "C" fn chat_event(dialog: usize, event: usize) -> u32 {
        let taken = hook::game_state().is_some_and(|(memory, layout)| {
            panic::catch_unwind(AssertUnwindSafe(|| {
                takes_enter_line(memory, layout, dialog, event)
            }))
            .unwrap_or(false)
        });
        if taken {
            return 1;
        }
        let original = TABLE[0].original.load(Acquire);
        let original: unsafe extern "C" fn(usize, usize) -> u32 =
            unsafe { std::mem::transmute(original) };
        original(dialog, event)
    }

    /// Whether the handler takes the line: capture is on, the event is a
    /// character input of Enter, and the open box holds a line, which is
    /// queued for the server while the box closes without sending it. The
    /// game's handler clears the box only after it sends, so the line is
    /// read before the game's function runs.
    fn takes_enter_line(memory: &InProcessMemory, layout: &Layout, dialog: usize, event: usize) -> bool {
        let Some(chat) = layout.chat.as_ref() else { return false };
        if !capturing() || !is_enter(memory, chat, event) {
            return false;
        }
        let Some((_, text)) = typed_line(memory, layout, chat) else { return false };
        if text.is_empty() {
            return false;
        }
        let Some(close) = memory.main_module().range_address(chat.close, 1) else { return false };
        push(ChatLine { kind: chat_kind::SENT, player: u32::MAX, text });
        let close: unsafe extern "C" fn(usize, usize, u8) = unsafe { std::mem::transmute(close) };
        // Send 0 drops the line, the way Escape does.
        unsafe { close(dialog, event, 0) };
        true
    }

    /// Whether the event is a character input of Enter or newline — the
    /// keys the game's own handler sends a line on.
    fn is_enter(memory: &InProcessMemory, chat: &ChatLayout, event: usize) -> bool {
        let field = |offset: usize| {
            event.checked_add(offset).and_then(|at| {
                let mut raw = [0; 2];
                memory.read_into(at, &mut raw).ok().map(|()| u16::from_le_bytes(raw))
            })
        };
        field(chat.event_kind) == Some(chat.event_kind_char)
            && matches!(field(chat.event_key), Some(0x0d) | Some(0x0a))
    }
}

#[cfg(windows)]
pub(crate) use game::TABLE;

#[cfg(test)]
mod tests {
    use super::*;
    use bwrapi_memory::ProcessModule;
    use std::sync::atomic::Ordering;

    /// Pointer values only: the core moves them, it never calls them.
    const HOOK: usize = 0x1000;
    const FIRST_GAME_FUNCTION: usize = 0x2000;
    const REBUILT_GAME_FUNCTION: usize = 0x3000;

    /// Where each test's slot lives: a plain local variable standing in for
    /// the game's writable memory.
    static SLOT64: AtomicUsize = AtomicUsize::new(0);
    static SLOT32: AtomicUsize = AtomicUsize::new(0);

    fn found64(_: &InProcessMemory, _: &Layout) -> Option<usize> {
        let address = SLOT64.load(Ordering::Relaxed);
        (address != 0).then_some(address)
    }

    fn found32(_: &InProcessMemory, _: &Layout) -> Option<usize> {
        let address = SLOT32.load(Ordering::Relaxed);
        (address != 0).then_some(address)
    }

    fn memory() -> InProcessMemory {
        InProcessMemory::with_module(ProcessModule { base_address: 0, size: usize::MAX })
    }

    #[test]
    fn fitting_saves_the_game_function_installs_and_keeps_the_hook() {
        let slot = Slot { find: found64, hook: |_| HOOK, original: AtomicUsize::new(0) };
        let mut cell: usize = FIRST_GAME_FUNCTION;
        SLOT64.store(std::ptr::addr_of_mut!(cell) as usize, Ordering::Relaxed);

        // The first fit saves the game's function and installs the hook.
        sync(&memory(), &Layout::X64, std::slice::from_ref(&slot));
        assert_eq!(cell, HOOK);
        assert_eq!(slot.original.load(Ordering::Relaxed), FIRST_GAME_FUNCTION);

        // A slot already holding the hook is left alone; the saved function
        // stands.
        sync(&memory(), &Layout::X64, std::slice::from_ref(&slot));
        assert_eq!(cell, HOOK);
        assert_eq!(slot.original.load(Ordering::Relaxed), FIRST_GAME_FUNCTION);

        // The game rebuilt the object and put its own function back: the
        // new value is saved and the hook reinstalled.
        cell = REBUILT_GAME_FUNCTION;
        sync(&memory(), &Layout::X64, std::slice::from_ref(&slot));
        assert_eq!(cell, HOOK);
        assert_eq!(slot.original.load(Ordering::Relaxed), REBUILT_GAME_FUNCTION);

        // A slot that cannot be found is skipped.
        SLOT64.store(0, Ordering::Relaxed);
        cell = FIRST_GAME_FUNCTION;
        sync(&memory(), &Layout::X64, std::slice::from_ref(&slot));
        assert_eq!(cell, FIRST_GAME_FUNCTION);
        assert_eq!(slot.original.load(Ordering::Relaxed), REBUILT_GAME_FUNCTION);
    }

    #[test]
    fn the_same_core_fits_the_x86_pointer_width() {
        let slot = Slot { find: found32, hook: |_| HOOK, original: AtomicUsize::new(0) };
        let mut cell: u32 = FIRST_GAME_FUNCTION as u32;
        SLOT32.store(std::ptr::addr_of_mut!(cell) as usize, Ordering::Relaxed);
        sync(&memory(), &Layout::X86, std::slice::from_ref(&slot));
        assert_eq!(cell, HOOK as u32);
        assert_eq!(slot.original.load(Ordering::Relaxed), FIRST_GAME_FUNCTION);
    }
}
