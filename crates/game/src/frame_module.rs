//! Status block of the frame-module example: what its ABI frame callback
//! saw, published as `Local\bwrapi_module_<pid>` so `module_status` reads it
//! the way `status` reads the hook's own block.

use std::sync::atomic::{AtomicU32, AtomicU64};

pub const MODULE_STATUS_VERSION: u32 = 1;

/// [`ModuleStatusBlock::state`] values.
pub mod module_state {
    /// Installed, not registered yet.
    pub const STARTING: u32 = 0;
    /// The frame callback is registered and running.
    pub const RUNNING: u32 = 1;
    /// `bwrapi.dll` was not there or refused the registration.
    pub const NO_ABI: u32 = 2;
}

/// Status block shared as `Local\bwrapi_module_<pid>`. The game thread is
/// the only writer of the per-frame fields; readers copy it into
/// [`ModuleStatus`].
#[repr(C)]
pub struct ModuleStatusBlock {
    pub version: AtomicU32,
    pub state: AtomicU32,
    /// How long the callback holds the game, in milliseconds.
    pub wait_ms: AtomicU32,
    pub callbacks: AtomicU64,
    /// Game frame of the last callback; `u32::MAX` when it read none.
    pub last_game_frame: AtomicU32,
    pub last_unit_rows: AtomicU32,
    pub qpc_frequency: AtomicU64,
    pub last_interval_qpc: AtomicU64,
    pub max_interval_qpc: AtomicU64,
}

/// Copy of the module status block.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ModuleStatus {
    pub version: u32,
    pub state: u32,
    pub wait_ms: u32,
    pub callbacks: u64,
    pub last_game_frame: u32,
    pub last_unit_rows: u32,
    pub qpc_frequency: u64,
    pub last_interval_qpc: u64,
    pub max_interval_qpc: u64,
}

pub fn module_status_name(pid: u32) -> Vec<u16> {
    format!("Local\\bwrapi_module_{pid}").encode_utf16().chain(Some(0)).collect()
}

#[cfg(windows)]
impl ModuleStatus {
    /// Copy of the module status of game `pid`; `None` before the module
    /// created it.
    pub fn open(pid: u32) -> Option<Self> {
        use std::mem::size_of;
        use std::sync::atomic::Ordering::{Acquire, Relaxed};
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::Memory::{
            MapViewOfFile, OpenFileMappingW, UnmapViewOfFile, FILE_MAP_READ,
        };

        let name = module_status_name(pid);
        let mapping = unsafe { OpenFileMappingW(FILE_MAP_READ, 0, name.as_ptr()) };
        if mapping == 0 {
            return None;
        }
        let view =
            unsafe { MapViewOfFile(mapping, FILE_MAP_READ, 0, 0, size_of::<ModuleStatusBlock>()) };
        unsafe { CloseHandle(mapping) };
        let block = view.Value.cast::<ModuleStatusBlock>();
        if block.is_null() {
            return None;
        }
        // `callbacks` is published last by the game thread.
        let callbacks = unsafe { (*block).callbacks.load(Acquire) };
        let status = ModuleStatus {
            version: unsafe { (*block).version.load(Relaxed) },
            state: unsafe { (*block).state.load(Relaxed) },
            wait_ms: unsafe { (*block).wait_ms.load(Relaxed) },
            callbacks,
            last_game_frame: unsafe { (*block).last_game_frame.load(Relaxed) },
            last_unit_rows: unsafe { (*block).last_unit_rows.load(Relaxed) },
            qpc_frequency: unsafe { (*block).qpc_frequency.load(Relaxed) },
            last_interval_qpc: unsafe { (*block).last_interval_qpc.load(Relaxed) },
            max_interval_qpc: unsafe { (*block).max_interval_qpc.load(Relaxed) },
        };
        unsafe { UnmapViewOfFile(view) };
        Some(status)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::{offset_of, size_of};

    #[test]
    fn module_status_block_and_copy_share_one_layout() {
        assert_eq!(size_of::<ModuleStatusBlock>(), 56);
        assert_eq!(size_of::<ModuleStatus>(), 56);
        for (block, copy) in [
            (offset_of!(ModuleStatusBlock, state), offset_of!(ModuleStatus, state)),
            (offset_of!(ModuleStatusBlock, wait_ms), offset_of!(ModuleStatus, wait_ms)),
            (offset_of!(ModuleStatusBlock, callbacks), offset_of!(ModuleStatus, callbacks)),
            (offset_of!(ModuleStatusBlock, last_game_frame), offset_of!(ModuleStatus, last_game_frame)),
            (offset_of!(ModuleStatusBlock, last_unit_rows), offset_of!(ModuleStatus, last_unit_rows)),
            (offset_of!(ModuleStatusBlock, qpc_frequency), offset_of!(ModuleStatus, qpc_frequency)),
            (offset_of!(ModuleStatusBlock, last_interval_qpc), offset_of!(ModuleStatus, last_interval_qpc)),
            (offset_of!(ModuleStatusBlock, max_interval_qpc), offset_of!(ModuleStatus, max_interval_qpc)),
        ] {
            assert_eq!(block, copy);
        }
        assert_eq!(offset_of!(ModuleStatusBlock, callbacks), 16);
        assert_eq!(offset_of!(ModuleStatusBlock, qpc_frequency), 32);
    }

    #[test]
    fn module_status_name_is_per_process() {
        let name = String::from_utf16(&module_status_name(4321)).unwrap();
        assert_eq!(name, "Local\\bwrapi_module_4321\0");
    }
}
