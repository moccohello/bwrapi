//! In-game half of `bwrapi.dll`. Injected into a running game, the frame
//! hook runs [`on_frame`](hook) on the game thread once for every new game
//! frame and, through [`callback`], the frame callback a module registered
//! via the public ABI. Outside the game, [`inject`] loads the DLL,
//! [`GameStatus::open`] reads what it publishes, and `GamePublication`
//! receives the frames it reads for readers in other processes.

use std::sync::atomic::{AtomicU32, AtomicU64};

use bwrapi_engine::tables::source;

/// The game's member functions, which take `this` first: in ecx on x86
/// (thiscall), the platform C convention on x64. `member_fn!(type (args) ->
/// ret)` names a function pointer type; `member_fn!(unsafe fn ...)` defines
/// a handler the game calls as a member function. The game's other
/// functions take the platform C convention (`extern "C"`, cdecl on x86).
#[cfg(target_pointer_width = "64")]
#[allow(unused_macros)]
macro_rules! member_fn {
    (type ($($arg:ty),*) $(-> $ret:ty)?) => { unsafe extern "C" fn($($arg),*) $(-> $ret)? };
    ($(#[$meta:meta])* unsafe fn $name:ident($($arg:ident: $ty:ty),*) $(-> $ret:ty)? $body:block) => {
        $(#[$meta])* unsafe extern "C" fn $name($($arg: $ty),*) $(-> $ret)? $body
    };
}
#[cfg(target_pointer_width = "32")]
#[allow(unused_macros)]
macro_rules! member_fn {
    (type ($($arg:ty),*) $(-> $ret:ty)?) => { unsafe extern "thiscall" fn($($arg),*) $(-> $ret)? };
    ($(#[$meta:meta])* unsafe fn $name:ident($($arg:ident: $ty:ty),*) $(-> $ret:ty)? $body:block) => {
        $(#[$meta])* unsafe extern "thiscall" fn $name($($arg: $ty),*) $(-> $ret)? $body
    };
}

pub mod callback;
mod chat;
mod commands;
mod digest;
pub mod draw;
pub mod frame_module;
mod slots;

#[cfg(windows)]
mod hook;
#[cfg(windows)]
mod inject;
#[cfg(any(windows, test))]
#[cfg_attr(not(windows), allow(dead_code))]
mod instruction;
#[cfg(windows)]
mod publish;
#[cfg(windows)]
mod resources;

pub use callback::FrameCallback;
pub use chat::{chat_kind, chat_line, ChatLine};
pub use digest::{players_digest, units_digest};
pub use draw::{DrawError, DrawShape};
#[cfg(windows)]
pub use commands::GameCommands;
#[cfg(windows)]
pub use hook::callwndproc;
#[cfg(windows)]
pub use inject::{inject, inject_module, matches_game, GameFrameSlot, GameFrames, INJECT_HELPER};
#[cfg(windows)]
pub use publish::GamePublication;

/// Export of [`callwndproc`] the injector looks up in `bwrapi.dll`.
pub const HOOK_EXPORT: &str = "bwr_callwndproc";

/// Registers the process-wide frame callback; `None` removes it. Fails
/// while the frame hook is not installed, because then nothing would invoke
/// it.
#[cfg(windows)]
pub fn set_frame_callback(
    callback: Option<FrameCallback>,
    user: *mut std::ffi::c_void,
) -> Result<(), String> {
    if !hook::installed() {
        return Err("the frame hook is not installed".to_owned());
    }
    callback::set(callback, user);
    Ok(())
}

/// Turns the chat capture of the in-game server on or off: while it is on, a
/// line the local player types is taken instead of sent, and the lines the
/// game records as received are kept, both for [`next_chat`]. Fails while the
/// frame hook is not installed, because then nothing would take them.
#[cfg(windows)]
pub fn set_chat_capture(enable: bool) -> Result<(), String> {
    if !hook::installed() {
        return Err("the frame hook is not installed".to_owned());
    }
    chat::set_capture(enable);
    Ok(())
}

/// Draws `shapes` (with the UTF-8 `text` their text shapes name and the RGBA
/// of each text color code) into every screen update until the next call;
/// empty `shapes` draw nothing. Fails while the frame hook is not installed,
/// because then nothing would draw them.
#[cfg(windows)]
pub fn set_draw(shapes: &[DrawShape], text: &[u8], text_colors: &[u32; draw::TEXT_COLORS]) -> Result<(), DrawError> {
    if !hook::installed() {
        return Err(DrawError::NoFrameHook);
    }
    draw::set(shapes, text, text_colors)
}

/// The turn-send count the frame hook's watcher last saw change and the QPC
/// of the change; `None` while no frame hook runs. The game table's
/// send-time column carries the stamp.
#[cfg(windows)]
pub fn turn_watch() -> Option<(u32, u64)> {
    hook::turn_watch()
}

/// This match's repair and refund totals the in-game DLL counted; `None`
/// while it watches no match. The players table carries them.
#[cfg(windows)]
pub fn resource_totals() -> Option<[bwrapi_engine::ResourceTotals; bwrapi_engine::RESOURCE_SLOTS]> {
    resources::totals()
}

/// The oldest chat line capture kept and nobody took yet.
pub fn next_chat() -> Option<ChatLine> {
    chat::next()
}

pub const STATUS_VERSION: u32 = 2;

/// Sources the in-game frame callback reads: every 1.0 source.
pub const SOURCES: u64 = source::PLAYERS
    | source::UNITS
    | source::ORDERS
    | source::UNIT_COUNTS
    | source::UPGRADES
    | source::TECHS
    | source::BULLETS
    | source::VISION
    | source::VIEWPORT
    | source::SELECTION;

/// [`GameStatus::hook_state`] values.
pub mod hook_state {
    pub const NOT_INSTALLED: u32 = 0;
    pub const INSTALLED: u32 = 1;
}

/// Status block shared as `Local\bwrapi_game_<pid>`. The game thread is the
/// only writer of the per-frame fields; readers copy it into [`GameStatus`].
#[repr(C)]
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) struct StatusBlock {
    pub version: AtomicU32,
    pub hook_state: AtomicU32,
    pub install_thread_id: AtomicU32,
    pub step_thread_id: AtomicU32,
    pub callbacks: AtomicU64,
    pub last_game_frame: AtomicU32,
    pub skipped_frames: AtomicU32,
    pub last_frame_qpc: AtomicU64,
    pub max_gap_qpc: AtomicU64,
    pub max_latency_qpc: AtomicU64,
    pub qpc_frequency: AtomicU64,
    pub renders: AtomicU64,
    pub last_render_qpc: AtomicU64,
    pub max_render_qpc: AtomicU64,
    pub render_thread_id: AtomicU32,
    pub reserved: AtomicU32,
}

/// Copy of the status block.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GameStatus {
    pub version: u32,
    pub hook_state: u32,
    /// Thread that installed the hook (the game window's thread).
    pub install_thread_id: u32,
    /// Thread of the last frame callback.
    pub step_thread_id: u32,
    /// Frame callbacks run.
    pub callbacks: u64,
    /// Game frame at the last callback; `u32::MAX` outside a match.
    pub last_game_frame: u32,
    /// Frames that advanced without a callback of their own.
    pub skipped_frames: u32,
    pub last_frame_qpc: u64,
    /// Longest interval between two frame callbacks within a match.
    pub max_gap_qpc: u64,
    /// Longest delay from seeing a new frame to its callback.
    pub max_latency_qpc: u64,
    pub qpc_frequency: u64,
    /// Screen updates the game drew through the render hook.
    pub renders: u64,
    /// Time the render hook spent adding the drawn shapes, last and longest.
    pub last_render_qpc: u64,
    pub max_render_qpc: u64,
    /// Thread of the last screen update.
    pub render_thread_id: u32,
    pub reserved: u32,
}

#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn status_name(pid: u32) -> Vec<u16> {
    format!("Local\\bwrapi_game_{pid}").encode_utf16().chain(Some(0)).collect()
}

pub const FRAMES_VERSION: u32 = 2;
pub const FRAME_RING_CAPACITY: usize = 64;

/// One slot of the frame ring shared as `Local\bwrapi_game_frames_<pid>`. The
/// game thread is the only writer; it publishes a slot by writing
/// [`FrameSlot::game_frame`] last.
#[repr(C)]
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) struct FrameSlot {
    pub units_digest: AtomicU64,
    pub players_digest: AtomicU64,
    /// Acquire time of the frame, in microseconds.
    pub acquire_us: AtomicU32,
    pub unit_rows: AtomicU32,
    /// Written last, with `Release`: a reader that sees the frame number it
    /// wants here holds a complete slot.
    pub game_frame: AtomicU32,
}

/// Frame ring block shared as `Local\bwrapi_game_frames_<pid>`: the header,
/// then the last [`FRAME_RING_CAPACITY`] frames at `game_frame % capacity`.
/// The ring is a comparison device: the frame hook reads and publishes a
/// slot only while a reader holds the mapping open with `publish` set.
#[repr(C)]
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) struct FramesBlock {
    pub version: AtomicU32,
    /// Whether a reader asked for the per-frame publish; `GameFrames::open`
    /// sets it, dropping the reader clears it.
    pub publish: AtomicU32,
    /// Acquire time of the first frame of a match, which also reads the match
    /// tables; kept out of the ring.
    pub first_acquire_us: AtomicU32,
    pub slots: [FrameSlot; FRAME_RING_CAPACITY],
}

#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn frames_name(pid: u32) -> Vec<u16> {
    format!("Local\\bwrapi_game_frames_{pid}").encode_utf16().chain(Some(0)).collect()
}

/// Control block of the frame publication (`bwrapi_engine::publish`).
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn publish_name(pid: u32) -> Vec<u16> {
    format!("Local\\bwrapi_game_publish_{pid}").encode_utf16().chain(Some(0)).collect()
}

/// Data mapping of publication generation `generation`.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn publish_data_name(pid: u32, generation: u32) -> Vec<u16> {
    format!("Local\\bwrapi_game_publish_{pid}_{generation}").encode_utf16().chain(Some(0)).collect()
}

/// Auto-reset event the game sets for reader slot `slot` after each
/// publication.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn publish_event_name(pid: u32, slot: usize) -> Vec<u16> {
    format!("Local\\bwrapi_game_publish_{pid}_event_{slot}").encode_utf16().chain(Some(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::{offset_of, size_of};

    #[test]
    fn status_block_and_copy_share_one_layout() {
        assert_eq!(size_of::<StatusBlock>(), 96);
        assert_eq!(size_of::<GameStatus>(), 96);
        let block = [
            offset_of!(StatusBlock, callbacks),
            offset_of!(StatusBlock, last_game_frame),
            offset_of!(StatusBlock, skipped_frames),
            offset_of!(StatusBlock, last_frame_qpc),
            offset_of!(StatusBlock, max_latency_qpc),
            offset_of!(StatusBlock, qpc_frequency),
            offset_of!(StatusBlock, renders),
            offset_of!(StatusBlock, max_render_qpc),
            offset_of!(StatusBlock, render_thread_id),
        ];
        let copy = [
            offset_of!(GameStatus, callbacks),
            offset_of!(GameStatus, last_game_frame),
            offset_of!(GameStatus, skipped_frames),
            offset_of!(GameStatus, last_frame_qpc),
            offset_of!(GameStatus, max_latency_qpc),
            offset_of!(GameStatus, qpc_frequency),
            offset_of!(GameStatus, renders),
            offset_of!(GameStatus, max_render_qpc),
            offset_of!(GameStatus, render_thread_id),
        ];
        assert_eq!(block, [16, 24, 28, 32, 48, 56, 64, 80, 88]);
        assert_eq!(block, copy);
    }

    #[test]
    fn status_name_is_per_process() {
        let name = String::from_utf16(&status_name(4321)).unwrap();
        assert_eq!(name, "Local\\bwrapi_game_4321\0");
    }

    #[test]
    fn frames_block_publishes_the_game_frame_last() {
        assert_eq!(size_of::<FrameSlot>(), 32);
        assert_eq!(offset_of!(FrameSlot, game_frame), 24);
        assert_eq!(size_of::<FramesBlock>(), 16 + FRAME_RING_CAPACITY * 32);
        assert_eq!(offset_of!(FramesBlock, publish), 4);
        assert_eq!(offset_of!(FramesBlock, first_acquire_us), 8);
        assert_eq!(offset_of!(FramesBlock, slots), 16);
    }

    #[test]
    fn frames_name_is_per_process() {
        let name = String::from_utf16(&frames_name(4321)).unwrap();
        assert_eq!(name, "Local\\bwrapi_game_frames_4321\0");
    }

    #[test]
    fn publication_names_are_per_process_generation_and_reader() {
        let names = [publish_name(4321), publish_data_name(4321, 2), publish_event_name(4321, 15)];
        let names: Vec<String> = names.iter().map(|name| String::from_utf16(name).unwrap()).collect();
        assert_eq!(
            names,
            [
                "Local\\bwrapi_game_publish_4321\0",
                "Local\\bwrapi_game_publish_4321_2\0",
                "Local\\bwrapi_game_publish_4321_event_15\0",
            ]
        );
    }
}
