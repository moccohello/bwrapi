//! The one frame callback the in-game DLL runs: registered through the
//! public ABI by a module inside the game, invoked by the frame hook once
//! per game frame on the game thread. The game waits while it runs, and it
//! is the only callback slot, so a later registration replaces the earlier
//! one.

use std::ffi::c_void;
use std::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};

/// Callback shape of the public ABI: bare function pointer plus user value.
pub type FrameCallback = extern "C" fn(*mut c_void);

static CALLBACK: AtomicUsize = AtomicUsize::new(0);
static USER: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());

/// Registers `callback` with `user`, replacing any earlier one; `None`
/// removes it. The user value is published first, so a callback a reader
/// can see always reads its own user value.
pub fn set(callback: Option<FrameCallback>, user: *mut c_void) {
    match callback {
        Some(callback) => {
            USER.store(user, Ordering::Release);
            CALLBACK.store(callback as usize, Ordering::Release);
        }
        None => {
            CALLBACK.store(0, Ordering::Release);
            USER.store(std::ptr::null_mut(), Ordering::Release);
        }
    }
}

/// Runs the registered callback, if any, on the calling thread.
pub fn invoke() {
    let address = CALLBACK.load(Ordering::Acquire);
    if address != 0 {
        let user = USER.load(Ordering::Acquire);
        // SAFETY: `set` published this address as a live function pointer
        // together with `user`.
        let callback: FrameCallback = unsafe { std::mem::transmute(address) };
        callback(user);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU64;

    static SEEN: AtomicU64 = AtomicU64::new(0);

    extern "C" fn mark(user: *mut c_void) {
        SEEN.store(user as u64, Ordering::SeqCst);
    }

    extern "C" fn other(user: *mut c_void) {
        SEEN.store((user as u64) + 1, Ordering::SeqCst);
    }

    // One test: the slot is process-wide, so the states run in sequence.
    #[test]
    fn registration_runs_replaces_and_clears_the_callback() {
        invoke();
        assert_eq!(SEEN.load(Ordering::SeqCst), 0);

        set(Some(mark), 0x42 as *mut c_void);
        invoke();
        assert_eq!(SEEN.load(Ordering::SeqCst), 0x42);

        set(Some(other), 7 as *mut c_void);
        invoke();
        assert_eq!(SEEN.load(Ordering::SeqCst), 8);

        set(None, std::ptr::null_mut());
        SEEN.store(0, Ordering::SeqCst);
        invoke();
        assert_eq!(SEEN.load(Ordering::SeqCst), 0);
    }
}
