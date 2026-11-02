//! Frame-module example, built as `frame_module.dll`: injected beside
//! `bwrapi.dll` (see `module_status`), it receives one callback per game
//! frame through the public ABI only — the functions are resolved from the
//! already-loaded `bwrapi.dll`, so the module carries no engine of its own
//! — and publishes what each callback saw as `Local\bwrapi_module_<pid>`.
//!
//! The wait mode is fixed by the module file name: a build named
//! `frame_module_wait` holds the game for 100 ms inside every callback, so
//! the game's frame progress slows down by that much while the callback
//! runs — the synchronous contract in practice.
#![cfg(windows)]

use std::ffi::c_void;
use std::ptr::{null, null_mut};
use std::sync::atomic::Ordering::{Acquire, Relaxed, Release};
use std::sync::atomic::{AtomicPtr, AtomicU64};
use std::sync::Once;

use bwrapi::BwrFrame;
use bwrapi_engine::tables::source;
use windows_sys::Win32::Foundation::{INVALID_HANDLE_VALUE, LPARAM, LRESULT, MAX_PATH, WPARAM};
use windows_sys::Win32::System::LibraryLoader::{
    GetModuleFileNameW, GetModuleHandleExW, GetModuleHandleW, GetProcAddress,
    GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GET_MODULE_HANDLE_EX_FLAG_PIN,
};
use windows_sys::Win32::System::Memory::{
    CreateFileMappingW, MapViewOfFile, FILE_MAP_ALL_ACCESS, PAGE_READWRITE,
};
use windows_sys::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};
use windows_sys::Win32::System::Threading::{GetCurrentProcessId, Sleep};
use windows_sys::Win32::UI::WindowsAndMessaging::CallNextHookEx;

use bwrapi_game::frame_module::{module_state, module_status_name, ModuleStatusBlock, MODULE_STATUS_VERSION};

/// How long the wait build holds the game inside the callback.
const WAIT_BUILD_MS: u32 = 100;

/// `bwrapi.dll` functions the module calls; resolved at install, from the
/// DLL the injector loaded first.
struct Abi {
    open_self: unsafe extern "C" fn(*mut *mut c_void) -> u32,
    subscribe: unsafe extern "C" fn(*mut c_void, u64) -> u32,
    set_frame_callback:
        unsafe extern "C" fn(*mut c_void, Option<extern "C" fn(*mut c_void)>, *mut c_void) -> u32,
    acquire: unsafe extern "C" fn(*mut c_void, *mut *const BwrFrame) -> u32,
    release: unsafe extern "C" fn(*mut c_void, *const BwrFrame),
    close: unsafe extern "C" fn(*mut c_void),
}

static ABI: AtomicPtr<Abi> = AtomicPtr::new(null_mut());
static SESSION: AtomicPtr<c_void> = AtomicPtr::new(null_mut());
static STATUS: AtomicPtr<ModuleStatusBlock> = AtomicPtr::new(null_mut());
static LAST_QPC: AtomicU64 = AtomicU64::new(0);

/// `WH_CALLWNDPROC` entry the injector loads this module through; the first
/// message installs from the game thread.
#[no_mangle]
pub unsafe extern "system" fn frame_module_callwndproc(
    code: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    static INSTALL: Once = Once::new();
    if code >= 0 && lparam != 0 {
        INSTALL.call_once(|| {
            let _ = std::panic::catch_unwind(install);
        });
    }
    CallNextHookEx(0, code, wparam, lparam)
}

fn install() {
    unsafe {
        // The injector removes its hook right after the first message, but
        // the callback keeps running from bwrapi.dll, so this module must
        // never unload.
        let mut module = 0;
        if GetModuleHandleExW(
            GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_PIN,
            frame_module_callwndproc as *const () as *const u16,
            &mut module,
        ) == 0
        {
            return;
        }
        let Some(status) = create_status() else { return };
        STATUS.store(std::ptr::from_ref(status).cast_mut(), Release);

        let mut name = [0u16; MAX_PATH as usize];
        let length = GetModuleFileNameW(module, name.as_mut_ptr(), MAX_PATH);
        let wait_build = String::from_utf16_lossy(&name[..length as usize]).contains("frame_module_wait");
        status.wait_ms.store(if wait_build { WAIT_BUILD_MS } else { 0 }, Relaxed);

        let Some(abi) = resolve_abi() else {
            status.state.store(module_state::NO_ABI, Release);
            return;
        };
        ABI.store(std::ptr::from_ref(abi).cast_mut(), Release);

        let mut session = null_mut();
        if (abi.open_self)(&mut session) != 0 || session.is_null() {
            status.state.store(module_state::NO_ABI, Release);
            return;
        }
        (abi.subscribe)(session, source::UNITS);
        if (abi.set_frame_callback)(session, Some(on_frame), null_mut()) != 0 {
            status.state.store(module_state::NO_ABI, Release);
            (abi.close)(session);
            return;
        }
        SESSION.store(session, Release);
        status.state.store(module_state::RUNNING, Release);
    }
}

/// The module's own status block, `Local\bwrapi_module_<pid>`.
unsafe fn create_status() -> Option<&'static ModuleStatusBlock> {
    let name = module_status_name(GetCurrentProcessId());
    let size = std::mem::size_of::<ModuleStatusBlock>();
    let mapping =
        CreateFileMappingW(INVALID_HANDLE_VALUE, null(), PAGE_READWRITE, 0, size as u32, name.as_ptr());
    if mapping == 0 {
        return None;
    }
    let view = MapViewOfFile(mapping, FILE_MAP_ALL_ACCESS, 0, 0, size);
    let block = view.Value.cast::<ModuleStatusBlock>();
    if block.is_null() {
        return None;
    }
    let mut frequency = 0;
    QueryPerformanceFrequency(&mut frequency);
    (*block).version.store(MODULE_STATUS_VERSION, Relaxed);
    (*block).qpc_frequency.store(frequency as u64, Relaxed);
    (*block).last_game_frame.store(u32::MAX, Relaxed);
    Some(&*block)
}

unsafe fn resolve_abi() -> Option<&'static Abi> {
    let name: Vec<u16> = "bwrapi.dll".encode_utf16().chain(Some(0)).collect();
    let bwrapi = GetModuleHandleW(name.as_ptr());
    if bwrapi == 0 {
        return None;
    }
    let mut name_buffer = [0u8; 32];
    let entry = |name: &str, buffer: &mut [u8]| unsafe {
        buffer[..name.len()].copy_from_slice(name.as_bytes());
        buffer[name.len()] = 0;
        GetProcAddress(bwrapi, buffer.as_ptr().cast())
    };
    let open_self = entry("bwr_open_self", &mut name_buffer)?;
    let subscribe = entry("bwr_subscribe", &mut name_buffer)?;
    let set_frame_callback = entry("bwr_set_frame_callback", &mut name_buffer)?;
    let acquire = entry("bwr_acquire", &mut name_buffer)?;
    let release = entry("bwr_release", &mut name_buffer)?;
    let close = entry("bwr_close", &mut name_buffer)?;
    Some(Box::leak(Box::new(Abi {
        open_self: std::mem::transmute(open_self),
        subscribe: std::mem::transmute(subscribe),
        set_frame_callback: std::mem::transmute(set_frame_callback),
        acquire: std::mem::transmute(acquire),
        release: std::mem::transmute(release),
        close: std::mem::transmute(close),
    })))
}

/// One per game frame, on the game thread; the game waits until it returns.
extern "C" fn on_frame(_user: *mut c_void) {
    let status = STATUS.load(Acquire);
    if status.is_null() {
        return;
    }
    let status = unsafe { &*status };
    let now = qpc();
    let (game_frame, unit_rows) = unsafe { read_frame() };
    let last = LAST_QPC.swap(now, Relaxed);
    if last != 0 {
        let interval = now.wrapping_sub(last);
        status.last_interval_qpc.store(interval, Relaxed);
        status.max_interval_qpc.fetch_max(interval, Relaxed);
    }
    status.last_game_frame.store(game_frame, Relaxed);
    status.last_unit_rows.store(unit_rows, Relaxed);
    status.callbacks.fetch_add(1, Release);

    let wait = status.wait_ms.load(Relaxed);
    if wait != 0 {
        unsafe { Sleep(wait) };
    }
}

/// Reads this frame through the ABI: its number and the unit table's rows.
unsafe fn read_frame() -> (u32, u32) {
    let session = SESSION.load(Acquire);
    let abi = ABI.load(Acquire);
    if session.is_null() || abi.is_null() {
        return (u32::MAX, 0);
    }
    let mut frame: *const BwrFrame = null();
    if ((*abi).acquire)(session, &mut frame) != 0 || frame.is_null() {
        return (u32::MAX, 0);
    }
    let game_frame = (*frame).game_frame;
    let units = (*frame).tables[source::UNITS.trailing_zeros() as usize];
    let rows = (!units.is_null()).then(|| (*units) as u32).unwrap_or(0);
    ((*abi).release)(session, frame);
    (game_frame, rows)
}

fn qpc() -> u64 {
    let mut now = 0;
    unsafe { QueryPerformanceCounter(&mut now) };
    now as u64
}
