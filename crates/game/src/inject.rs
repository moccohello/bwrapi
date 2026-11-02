//! Outside the game: loads DLLs into a running game through a window hook
//! and reads the status blocks they publish.
//!
//! A hook DLL loads only into a process of its own pointer width, and only a
//! process of that width can load it to install the hook. A game of the other
//! width (an x86 game from a 64-bit launcher) is injected by the
//! `bwrapi-inject.exe` beside the DLL, which is built for that width.

use std::mem::size_of;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::Command;
use std::sync::atomic::Ordering::{Acquire, Release, Relaxed};

use windows_sys::Win32::Foundation::{CloseHandle, FreeLibrary, GetLastError, HMODULE, HWND, LPARAM};
use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, IsWow64Process, OpenProcess, CREATE_NO_WINDOW, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows_sys::Win32::System::Memory::{
    MapViewOfFile, OpenFileMappingW, UnmapViewOfFile, FILE_MAP_ALL_ACCESS, FILE_MAP_READ,
    MEMORY_MAPPED_VIEW_ADDRESS,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowTextW, GetWindowThreadProcessId, SendMessageTimeoutW, SetWindowsHookExW,
    UnhookWindowsHookEx, SMTO_BLOCK, WH_CALLWNDPROC, WM_NULL,
};

use crate::{
    frames_name, hook_state, status_name, FramesBlock, FRAME_RING_CAPACITY, FRAMES_VERSION,
    GameStatus, StatusBlock, HOOK_EXPORT,
};

const LOAD_TIMEOUT_MS: u32 = 5000;

/// The injector beside each `bwrapi.dll`, built with that DLL's pointer width:
/// `bwrapi-inject <pid> <dll> [export]`.
pub const INJECT_HELPER: &str = "bwrapi-inject.exe";

/// Loads `dll_path` (`bwrapi.dll`) into game process `pid` and installs the
/// frame hook. The DLL pins itself, so it stays after the window hook is
/// removed. Does nothing when the hook is already installed.
pub fn inject(pid: u32, dll_path: &Path) -> Result<(), String> {
    if GameStatus::open(pid).is_some_and(|status| status.hook_state == hook_state::INSTALLED) {
        return Ok(());
    }
    if !matches_game(pid)? {
        return inject_through_helper(pid, dll_path, None);
    }
    let (window, thread_id) = game_window(pid)?;
    hook_window(window, thread_id, dll_path, HOOK_EXPORT)?;
    match GameStatus::open(pid) {
        Some(status) if status.hook_state == hook_state::INSTALLED => Ok(()),
        Some(status) => Err(format!("frame hook not installed: hook_state={}", status.hook_state)),
        None => Err("the DLL did not start in the game".to_owned()),
    }
}

/// Loads a module DLL into game process `pid` the same way, so its hook
/// export runs once on the game thread and initializes there. The module
/// uses the public ABI, so `bwrapi.dll` must sit in the game already; it is
/// injected first, from the module's own directory, when it is not.
pub fn inject_module(pid: u32, dll_path: &Path, export: &str) -> Result<(), String> {
    if !matches_game(pid)? {
        return inject_through_helper(pid, dll_path, Some(export));
    }
    let bwrapi = dll_path
        .parent()
        .map(|directory| directory.join("bwrapi.dll"))
        .unwrap_or_else(|| Path::new("bwrapi.dll").to_path_buf());
    inject(pid, &bwrapi)?;
    let (window, thread_id) = game_window(pid)?;
    hook_window(window, thread_id, dll_path, export)
}

/// Whether game `pid` has the pointer width of this process, so this
/// process can load the game's DLLs and hook its window.
pub fn matches_game(pid: u32) -> Result<bool, String> {
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process == 0 {
        return Err(format!("open process {pid}: {}", unsafe { GetLastError() }));
    }
    let mut game = 0;
    let queried = unsafe { IsWow64Process(process, &mut game) };
    let error = unsafe { GetLastError() };
    unsafe { CloseHandle(process) };
    if queried == 0 {
        return Err(format!("query process {pid}: {error}"));
    }
    let mut own = 0;
    unsafe { IsWow64Process(GetCurrentProcess(), &mut own) };
    Ok(game == own)
}

/// Runs the [`INJECT_HELPER`] beside `dll_path`, which has the DLL's and so
/// the game's pointer width, and returns its error output on failure.
fn inject_through_helper(pid: u32, dll_path: &Path, export: Option<&str>) -> Result<(), String> {
    let helper = dll_path.parent().unwrap_or(Path::new("")).join(INJECT_HELPER);
    let output = Command::new(&helper)
        .arg(pid.to_string())
        .arg(dll_path)
        .args(export)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|error| format!("start {}: {error}", helper.display()))?;
    if output.status.success() {
        return Ok(());
    }
    let message = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    if message.is_empty() {
        Err(format!("{} failed: {}", helper.display(), output.status))
    } else {
        Err(message)
    }
}

/// Window-hook load: the first message the game window thread receives runs
/// `export` of `dll_path` there, which installs and pins itself.
fn hook_window(window: HWND, thread_id: u32, dll_path: &Path, export: &str) -> Result<(), String> {
    let library = Library::load(dll_path)?;
    let name = format!("{export}\0");
    let hook_proc = unsafe { GetProcAddress(library.0, name.as_ptr()) }
        .ok_or_else(|| format!("{} does not export {export}", dll_path.display()))?;
    let hook = unsafe {
        SetWindowsHookExW(WH_CALLWNDPROC, Some(std::mem::transmute(hook_proc)), library.0, thread_id)
    };
    if hook == 0 {
        return Err(format!("SetWindowsHookExW failed: {}", unsafe { GetLastError() }));
    }
    let mut result = 0;
    // No SMTO_ABORTIFHUNG: a game window busy for a few seconds (a lobby
    // turn, loading) counts as hung and would fail at once; the timeout
    // bounds the wait instead.
    let sent = unsafe { SendMessageTimeoutW(window, WM_NULL, 0, 0, SMTO_BLOCK, LOAD_TIMEOUT_MS, &mut result) };
    unsafe { UnhookWindowsHookEx(hook) };
    if sent == 0 {
        return Err(format!("game window did not answer: {}", unsafe { GetLastError() }));
    }
    Ok(())
}

impl GameStatus {
    /// Copy of the status block of game `pid`; `None` before injection.
    pub fn open(pid: u32) -> Option<Self> {
        let name = status_name(pid);
        let mapping = unsafe { OpenFileMappingW(FILE_MAP_READ, 0, name.as_ptr()) };
        if mapping == 0 {
            return None;
        }
        let view = unsafe { MapViewOfFile(mapping, FILE_MAP_READ, 0, 0, size_of::<StatusBlock>()) };
        unsafe { CloseHandle(mapping) };
        let block = view.Value.cast::<StatusBlock>();
        if block.is_null() {
            return None;
        }
        let status = copy(unsafe { &*block });
        unsafe { UnmapViewOfFile(view) };
        Some(status)
    }
}

fn copy(block: &StatusBlock) -> GameStatus {
    // `callbacks` is published last by the game thread.
    let callbacks = block.callbacks.load(Acquire);
    GameStatus {
        version: block.version.load(Relaxed),
        hook_state: block.hook_state.load(Acquire),
        install_thread_id: block.install_thread_id.load(Relaxed),
        step_thread_id: block.step_thread_id.load(Relaxed),
        callbacks,
        last_game_frame: block.last_game_frame.load(Relaxed),
        skipped_frames: block.skipped_frames.load(Relaxed),
        last_frame_qpc: block.last_frame_qpc.load(Relaxed),
        max_gap_qpc: block.max_gap_qpc.load(Relaxed),
        max_latency_qpc: block.max_latency_qpc.load(Relaxed),
        qpc_frequency: block.qpc_frequency.load(Relaxed),
        renders: block.renders.load(Relaxed),
        last_render_qpc: block.last_render_qpc.load(Relaxed),
        max_render_qpc: block.max_render_qpc.load(Relaxed),
        render_thread_id: block.render_thread_id.load(Relaxed),
        reserved: 0,
    }
}

/// Copy of one slot of the frame ring.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GameFrameSlot {
    pub units_digest: u64,
    pub players_digest: u64,
    /// Acquire time of the frame, in microseconds.
    pub acquire_us: u32,
    pub unit_rows: u32,
    pub game_frame: u32,
}

/// Reader of the frame ring the in-game DLL publishes as
/// `Local\bwrapi_game_frames_<pid>`; holds the view until dropped. Opening
/// it is what asks the in-game DLL to read and publish a slot per frame;
/// dropped, the publish stops, so an unobserved game reads nothing extra.
pub struct GameFrames {
    view: MEMORY_MAPPED_VIEW_ADDRESS,
    block: *const FramesBlock,
}

impl GameFrames {
    /// Opens the ring of game `pid` and requests the per-frame publish;
    /// `None` before injection.
    pub fn open(pid: u32) -> Option<Self> {
        let name = frames_name(pid);
        let mapping = unsafe { OpenFileMappingW(FILE_MAP_ALL_ACCESS, 0, name.as_ptr()) };
        if mapping == 0 {
            return None;
        }
        let view =
            unsafe { MapViewOfFile(mapping, FILE_MAP_ALL_ACCESS, 0, 0, size_of::<FramesBlock>()) };
        unsafe { CloseHandle(mapping) };
        let block = view.Value.cast::<FramesBlock>();
        if block.is_null() {
            return None;
        }
        unsafe { (*block).publish.store(1, Release) };
        Some(Self { view, block })
    }

    /// The slot the game published for `game_frame`, `None` while the ring
    /// does not carry that frame.
    pub fn slot(&self, game_frame: u32) -> Option<GameFrameSlot> {
        let block = unsafe { &*self.block };
        if block.version.load(Relaxed) != FRAMES_VERSION {
            return None;
        }
        let slot = &block.slots[(game_frame as usize) % FRAME_RING_CAPACITY];
        (slot.game_frame.load(Acquire) == game_frame).then(|| GameFrameSlot {
            units_digest: slot.units_digest.load(Relaxed),
            players_digest: slot.players_digest.load(Relaxed),
            acquire_us: slot.acquire_us.load(Relaxed),
            unit_rows: slot.unit_rows.load(Relaxed),
            game_frame,
        })
    }

    /// Acquire time of the first frame of the last match.
    pub fn first_acquire_us(&self) -> u32 {
        unsafe { (*self.block).first_acquire_us.load(Relaxed) }
    }
}

impl Drop for GameFrames {
    fn drop(&mut self) {
        unsafe {
            (*self.block).publish.store(0, Release);
            UnmapViewOfFile(self.view);
        }
    }
}

struct Library(HMODULE);

impl Library {
    fn load(path: &Path) -> Result<Self, String> {
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let module = unsafe { LoadLibraryW(wide.as_ptr()) };
        if module == 0 {
            return Err(format!("load {}: {}", path.display(), unsafe { GetLastError() }));
        }
        Ok(Self(module))
    }
}

impl Drop for Library {
    fn drop(&mut self) {
        unsafe { FreeLibrary(self.0) };
    }
}

/// The game's main window ("Brood War") and its thread.
fn game_window(pid: u32) -> Result<(HWND, u32), String> {
    let mut search = Search { pid, found: None };
    unsafe { EnumWindows(Some(visit), &mut search as *mut Search as LPARAM) };
    search.found.ok_or_else(|| format!("process {pid} has no Brood War window"))
}

struct Search {
    pid: u32,
    found: Option<(HWND, u32)>,
}

unsafe extern "system" fn visit(window: HWND, lparam: LPARAM) -> i32 {
    let search = &mut *(lparam as *mut Search);
    let mut pid = 0;
    let thread_id = GetWindowThreadProcessId(window, &mut pid);
    if pid != search.pid || thread_id == 0 {
        return 1;
    }
    let mut title = [0u16; 16];
    let length = GetWindowTextW(window, title.as_mut_ptr(), title.len() as i32);
    if length > 0 && String::from_utf16_lossy(&title[..length as usize]) == "Brood War" {
        search.found = Some((window, thread_id));
        return 0;
    }
    1
}

#[cfg(test)]
mod tests {
    #[test]
    fn this_process_matches_its_own_width() {
        assert_eq!(super::matches_game(std::process::id()), Ok(true));
    }
}
