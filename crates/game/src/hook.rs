//! Runs inside the game. The window hook installs the frame hook once; after
//! that `on_frame` runs on the game thread once for every new game frame.
//!
//! The game's code is never changed and no debug register is used: the game
//! image cannot be made writable, and the game exits when its threads carry
//! hardware breakpoints. Instead a watcher thread sees the frame counter
//! advance and posts a message to the game window, whose procedure runs on
//! the game thread between two loop steps, so the game state is consistent
//! there and the game waits while the callback runs. Function pointers the
//! game keeps in its own writable data are the one thing replaced: the hook
//! table (`slots`) swaps each slot for a handler of this DLL and refits it
//! at every frame boundary. The players' resource page is made read-only
//! during a match so `resources` sees the game's own writes to it.

use std::mem::size_of;
use std::panic::{self, AssertUnwindSafe};
use std::ptr::{null, null_mut};
use std::sync::atomic::Ordering::{AcqRel, Acquire, Relaxed, Release};
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicPtr, AtomicU32, AtomicU64};
use std::sync::{Mutex, Once};

use bwrapi_engine::decode::bwgame::BWGAME_FRAME_COUNT_OFFSET;
use bwrapi_engine::{CommandBasis, Engine, Frame, Table};
use bwrapi_layout::{read_pointer, Layout};
use bwrapi_memory::{InProcessMemory, ProcessMemory};
use windows_sys::Win32::Foundation::{HWND, INVALID_HANDLE_VALUE, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::{
    GetModuleHandleExW, GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GET_MODULE_HANDLE_EX_FLAG_PIN,
};
use windows_sys::Win32::System::Memory::{
    CreateFileMappingW, MapViewOfFile, FILE_MAP_ALL_ACCESS, PAGE_READWRITE,
};
use windows_sys::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};
use windows_sys::Win32::System::Threading::{
    CreateWaitableTimerExW, GetCurrentProcessId, GetCurrentThreadId, SetWaitableTimer,
    WaitForSingleObject, CREATE_WAITABLE_TIMER_HIGH_RESOLUTION, INFINITE, TIMER_ALL_ACCESS,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, CallWindowProcW, IsWindow, PostMessageW, RegisterWindowMessageW, CWPSTRUCT,
    GWLP_WNDPROC, WNDPROC,
};
// The window procedure slot is pointer-wide; 32-bit Windows names its setter
// without the Ptr suffix.
#[cfg(target_pointer_width = "64")]
use windows_sys::Win32::UI::WindowsAndMessaging::SetWindowLongPtrW as set_window_procedure;
#[cfg(target_pointer_width = "32")]
use windows_sys::Win32::UI::WindowsAndMessaging::SetWindowLongW as set_window_procedure;

use crate::publish::Publisher;
use crate::{
    callback, chat, commands, commands::CommandsBlock, frames_name, hook_state, players_digest,
    slots, status_name, units_digest, FramesBlock, FRAME_RING_CAPACITY, FRAMES_VERSION, StatusBlock,
    STATUS_VERSION,
};

/// How often the watcher looks at the frame counter. A frame at the fastest
/// speed is 42 ms; a quarter millisecond keeps the callback within a loop
/// step of the frame change.
const WATCH_PERIOD_100NS: i64 = 2_500;

struct Game {
    status: &'static StatusBlock,
    frames: &'static FramesBlock,
    /// Command ring the frame callback drains; `None` when its mapping
    /// could not be created, which leaves reads working.
    commands: Option<&'static CommandsBlock>,
    /// Frame publication for readers in other processes; `None` when its
    /// mapping could not be created, which leaves them reading from outside.
    publisher: Option<Publisher>,
    /// Reads the game memory on the game thread; the lock only marks the
    /// single reader, it never contends.
    engine: Mutex<Engine>,
    /// Match the ring last recorded; its first frame is timed separately.
    ring_match_id: AtomicU32,
    memory: InProcessMemory,
    layout: &'static Layout,
    window: HWND,
    message: u32,
    /// A frame message is posted and not handled yet.
    posted: AtomicBool,
    /// When the watcher saw the frame that the posted message is for.
    seen_qpc: AtomicU64,
    /// The turn-send count the watcher last saw change, and the QPC of the
    /// change; the count is `u32::MAX` before the first observation.
    turns_seen: AtomicU32,
    turn_qpc: AtomicU64,
    /// The recorder stream while chat capture is on; only the game thread
    /// uses it.
    chat: Mutex<chat::ChatWatch>,
}

static GAME: AtomicPtr<Game> = AtomicPtr::new(null_mut());
/// The game window procedure the frame hook replaced.
static ORIGINAL: AtomicIsize = AtomicIsize::new(0);

/// `WH_CALLWNDPROC` hook procedure; the first message installs the frame
/// hook from the window thread. `bwrapi.dll` re-exports it as
/// [`HOOK_EXPORT`](crate::HOOK_EXPORT) for the injector.
pub unsafe extern "system" fn callwndproc(
    code: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    static INSTALL: Once = Once::new();
    if code >= 0 && lparam != 0 {
        let window = (*(lparam as *const CWPSTRUCT)).hwnd;
        INSTALL.call_once(|| {
            let _ = panic::catch_unwind(|| install(window));
        });
    }
    CallNextHookEx(0, code, wparam, lparam)
}

/// Whether the frame hook is installed, so a frame callback would run.
pub(crate) fn installed() -> bool {
    !GAME.load(Acquire).is_null()
}

/// The watcher's pairing of the turn-send count with the QPC of its last
/// change; `None` while no frame hook runs. The engine's game table names
/// the send time from it.
pub(crate) fn turn_watch() -> Option<(u32, u64)> {
    let game = GAME.load(Acquire);
    (!game.is_null()).then(|| unsafe { (*game).turn_watch() }).flatten()
}

/// The attached game's memory and layout for the hook table's handlers,
/// which the game calls without state of their own; `None` before the
/// frame hook installs.
pub(crate) fn game_state() -> Option<(&'static InProcessMemory, &'static Layout)> {
    let game = GAME.load(Acquire);
    (!game.is_null()).then(|| {
        let game = unsafe { &*game };
        (&game.memory, game.layout)
    })
}

fn install(window: HWND) {
    // The injector removes its window hook right after this message, but the
    // window procedure keeps calling into this DLL, so it must never unload.
    let mut module = 0;
    let pinned = unsafe {
        GetModuleHandleExW(
            GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_PIN,
            callwndproc as *const () as *const u16,
            &mut module,
        )
    };
    if pinned == 0 {
        return;
    }
    let Some(status) = create_status() else {
        return;
    };
    let mut frequency = 0;
    unsafe { QueryPerformanceFrequency(&mut frequency) };
    status.version.store(STATUS_VERSION, Relaxed);
    status.install_thread_id.store(unsafe { GetCurrentThreadId() }, Relaxed);
    status.qpc_frequency.store(frequency as u64, Relaxed);
    status.last_game_frame.store(u32::MAX, Relaxed);
    status.hook_state.store(install_frame_hook(status, window), Release);
}

fn install_frame_hook(status: &'static StatusBlock, window: HWND) -> u32 {
    let Ok(memory) = InProcessMemory::attach() else {
        return hook_state::NOT_INSTALLED;
    };
    let Some(layout) = Layout::detect(memory.main_module().size) else {
        return hook_state::NOT_INSTALLED;
    };
    let name: Vec<u16> = "bwrapi_game_frame".encode_utf16().chain(Some(0)).collect();
    let message = unsafe { RegisterWindowMessageW(name.as_ptr()) };
    if message == 0 {
        return hook_state::NOT_INSTALLED;
    }
    let Some(frames) = create_frames() else {
        return hook_state::NOT_INSTALLED;
    };
    let commands = commands::create_commands();
    let publisher = Publisher::create();
    let Ok(mut engine) = Engine::in_process() else {
        return hook_state::NOT_INSTALLED;
    };
    engine.subscribe(crate::SOURCES);
    let game: &'static Game = Box::leak(Box::new(Game {
        status,
        frames,
        commands,
        publisher,
        engine: Mutex::new(engine),
        ring_match_id: AtomicU32::new(0),
        memory,
        layout,
        window,
        message,
        posted: AtomicBool::new(false),
        seen_qpc: AtomicU64::new(0),
        turns_seen: AtomicU32::new(u32::MAX),
        turn_qpc: AtomicU64::new(0),
        chat: Mutex::new(chat::ChatWatch::default()),
    }));
    GAME.store(game as *const Game as *mut Game, Release);
    let original = unsafe { set_window_procedure(window, GWLP_WNDPROC, window_procedure as *const () as _) };
    if original == 0 {
        GAME.store(null_mut(), Release);
        return hook_state::NOT_INSTALLED;
    }
    ORIGINAL.store(original as isize, Release);
    slots::sync(&game.memory, game.layout, &slots::TABLE);
    std::thread::spawn(move || game.watch());
    hook_state::INSTALLED
}

/// Creates `Local\bwrapi_game_<pid>`. The mapping lives as long as the game,
/// so its handle stays open.
fn create_status() -> Option<&'static StatusBlock> {
    let name = status_name(unsafe { GetCurrentProcessId() });
    let size = size_of::<StatusBlock>();
    let mapping = unsafe {
        CreateFileMappingW(INVALID_HANDLE_VALUE, null(), PAGE_READWRITE, 0, size as u32, name.as_ptr())
    };
    if mapping == 0 {
        return None;
    }
    let view = unsafe { MapViewOfFile(mapping, FILE_MAP_ALL_ACCESS, 0, 0, size) };
    let block = view.Value.cast::<StatusBlock>();
    (!block.is_null()).then(|| unsafe { &*block })
}

/// Creates `Local\bwrapi_game_frames_<pid>` the same way.
fn create_frames() -> Option<&'static FramesBlock> {
    let name = frames_name(unsafe { GetCurrentProcessId() });
    let size = size_of::<FramesBlock>();
    let mapping = unsafe {
        CreateFileMappingW(INVALID_HANDLE_VALUE, null(), PAGE_READWRITE, 0, size as u32, name.as_ptr())
    };
    if mapping == 0 {
        return None;
    }
    let view = unsafe { MapViewOfFile(mapping, FILE_MAP_ALL_ACCESS, 0, 0, size) };
    let block = view.Value.cast::<FramesBlock>();
    let block = (!block.is_null()).then(|| unsafe { &*block })?;
    block.version.store(FRAMES_VERSION, Relaxed);
    Some(block)
}

unsafe extern "system" fn window_procedure(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let game = GAME.load(Acquire);
    if !game.is_null() && message == (*game).message {
        // Between two loop steps on the game thread nothing is freed under a
        // read, so the reads of this call copy directly.
        let _reads = bwrapi_memory::GameThreadReads::enter();
        let _ = panic::catch_unwind(AssertUnwindSafe(|| (*game).on_frame()));
        return 0;
    }
    let original: WNDPROC = std::mem::transmute(ORIGINAL.load(Acquire));
    CallWindowProcW(original, window, message, wparam, lparam)
}

/// The QPC counter the hook measures with.
pub(crate) fn qpc() -> u64 {
    let mut now = 0;
    unsafe { QueryPerformanceCounter(&mut now) };
    now as u64
}

/// Counts a screen update the render hook drew and the QPC ticks it spent
/// adding the shapes.
pub(crate) fn record_render(ticks: u64) {
    let game = GAME.load(Acquire);
    if game.is_null() {
        return;
    }
    let status = unsafe { (*game).status };
    status.render_thread_id.store(unsafe { GetCurrentThreadId() }, Relaxed);
    status.last_render_qpc.store(ticks, Relaxed);
    status.max_render_qpc.fetch_max(ticks, Relaxed);
    status.renders.fetch_add(1, Release);
}

impl Game {
    /// Watcher thread: posts one frame message per frame change. Reads only
    /// the frame counter and the turn-send count, each a single aligned u32
    /// the game writes. While the frame stands still (a paused game, the
    /// menu) it also posts, at most one message at a time, while commands
    /// wait: a paused game runs them (a resume among them), and outside a
    /// match they end as "not in a match" instead of staying queued. A reader
    /// that attaches or resubscribes is answered the same way, without
    /// waiting for the next frame.
    fn watch(&self) {
        let timer = unsafe {
            CreateWaitableTimerExW(null(), null(), CREATE_WAITABLE_TIMER_HIGH_RESOLUTION, TIMER_ALL_ACCESS)
        };
        if timer == 0 {
            return;
        }
        let due = -WATCH_PERIOD_100NS;
        let mut last = u32::MAX;
        while unsafe { IsWindow(self.window) } != 0 {
            if unsafe { SetWaitableTimer(timer, &due, 0, None, null(), 0) } == 0 {
                return;
            }
            unsafe { WaitForSingleObject(timer, INFINITE) };
            self.watch_turns();
            let frame = self.game_frame().unwrap_or(u32::MAX);
            if self.posted.load(Acquire) || (frame == last && !self.commands_pending() && !self.publish_pending()) {
                continue;
            }
            last = frame;
            self.seen_qpc.store(qpc(), Relaxed);
            self.posted.store(true, Release);
            if unsafe { PostMessageW(self.window, self.message, 0, 0) } == 0 {
                self.posted.store(false, Release);
            }
        }
    }

    /// The same turn-send count the engine's game table reads, every period:
    /// the moment it changes is when the game sent a turn. The first
    /// observation counts as a change, so the stamp is never missing.
    fn watch_turns(&self) {
        let Some(turns) = self.module_u32(self.layout.scalars.turn_send_count_offset) else {
            return;
        };
        if turns != self.turns_seen.load(Relaxed) {
            self.turn_qpc.store(qpc(), Relaxed);
            self.turns_seen.store(turns, Release);
        }
    }

    /// The watcher's pairing of the turn-send count with the QPC of its last
    /// change; `None` before the watcher saw a count. A turn the step just
    /// sent may be newer than the watcher's last poll, so the count is looked
    /// at here too and the frame being read has a stamp for its own count.
    fn turn_watch(&self) -> Option<(u32, u64)> {
        self.watch_turns();
        let turns = self.turns_seen.load(Acquire);
        (turns != u32::MAX).then(|| (turns, self.turn_qpc.load(Relaxed)))
    }

    /// Queues the lines the recorder took since the last call while chat
    /// capture is on.
    fn watch_chat(&self) {
        let mut watch = match self.chat.lock() {
            Ok(watch) => watch,
            Err(poisoned) => poisoned.into_inner(),
        };
        watch.poll(&self.memory, self.layout, self.game_frame());
    }

    fn commands_pending(&self) -> bool {
        self.commands.is_some_and(commands::has_commands)
    }

    fn publish_pending(&self) -> bool {
        self.publisher.as_ref().is_some_and(Publisher::pending)
    }

    /// On the game thread, between two loop steps.
    fn on_frame(&self) {
        let status = self.status;
        let now = qpc();
        let seen = self.seen_qpc.load(Relaxed);
        self.posted.store(false, Release);
        status.step_thread_id.store(unsafe { GetCurrentThreadId() }, Relaxed);
        status.callbacks.fetch_add(1, Release);

        slots::sync(&self.memory, self.layout, &slots::TABLE);
        crate::resources::watch(self.memory.main_module().base_address, self.layout, self.match_root());
        self.watch_chat();
        let frame = self.game_frame().unwrap_or(u32::MAX);
        let previous = status.last_game_frame.swap(frame, AcqRel);
        let last = status.last_frame_qpc.swap(if frame == u32::MAX { 0 } else { now }, Relaxed);
        if frame == u32::MAX || frame == previous {
            // No new frame: the commands that wait run, and readers that
            // attached or resubscribed get the standing frame, or that no
            // match runs.
            let measured = match &self.publisher {
                Some(publisher) if frame == u32::MAX => {
                    publisher.publish_menu();
                    None
                }
                Some(publisher) if publisher.pending() => self.read_and_publish(),
                _ => None,
            };
            self.drain_commands(measured);
            return;
        }
        // Timings cover consecutive frames of a match; the first one after
        // the match loads would only measure the loading.
        if previous != u32::MAX && frame >= previous {
            if frame > previous.wrapping_add(1) {
                status.skipped_frames.fetch_add(frame - previous - 1, Relaxed);
            }
            status.max_latency_qpc.fetch_max(now.wrapping_sub(seen), Relaxed);
            if last != 0 {
                status.max_gap_qpc.fetch_max(now.wrapping_sub(last), Relaxed);
            }
        }
        // The registered frame callback runs here, on the game thread, and
        // the game waits until it returns; what it submits drains at this
        // frame boundary, right below.
        callback::invoke();
        let measured = self.read_and_publish();
        self.drain_commands(measured);
    }

    /// Reads the sources the attached readers subscribed (and those of the
    /// comparison ring while a reader asked for it), publishes the frame to
    /// the readers and a ring slot, and hands back what a command needs to
    /// name units: `None` outside a match, so its commands end rather than
    /// wait. With no reader of either kind a frame costs no read here.
    fn read_and_publish(&self) -> Option<CommandBasis> {
        let ring = self.frames.publish.load(Acquire) != 0;
        let readers = self.publisher.as_ref().and_then(Publisher::readers);
        if !ring && readers.is_none() {
            return None;
        }
        let wanted = readers.map_or(0, |(sources, _)| sources);
        let publish = |frame: Option<&Frame>, basis: Option<&CommandBasis>| {
            if let (Some(publisher), Some((_, change))) = (&self.publisher, readers) {
                publisher.publish(frame, wanted, basis, change);
            }
        };
        let mut engine = match self.engine.lock() {
            Ok(engine) => engine,
            Err(poisoned) => poisoned.into_inner(),
        };
        engine.subscribe(if ring { crate::SOURCES } else { 0 } | wanted);
        engine.set_turn_watch(self.turn_watch());
        engine.set_resource_totals(crate::resources::totals());
        let start = qpc();
        let Ok(frame) = engine.acquire() else {
            publish(None, None);
            return None;
        };
        let ticks = qpc().wrapping_sub(start);
        let frequency = self.status.qpc_frequency.load(Relaxed).max(1);
        let us = (ticks as u128 * 1_000_000 / u128::from(frequency)) as u32;
        if frame.game_frame == u32::MAX {
            publish(None, None);
            return None;
        }
        let basis = engine.command_basis();
        publish(Some(&frame), basis.as_ref());
        if !ring {
            return basis;
        }
        // The first read of a match also reads the match tables; the ring
        // keeps only the steady per-frame cost.
        if frame.match_id != self.ring_match_id.swap(frame.match_id, Relaxed) {
            self.frames.first_acquire_us.store(us, Relaxed);
        } else {
            let slot = &self.frames.slots[frame.game_frame as usize % FRAME_RING_CAPACITY];
            slot.units_digest.store(units_digest(&frame.units), Relaxed);
            slot.players_digest.store(players_digest(&frame.players), Relaxed);
            slot.acquire_us.store(us, Relaxed);
            slot.unit_rows.store(frame.units.rows() as u32, Relaxed);
            slot.game_frame.store(frame.game_frame, Release);
        }
        basis
    }

    /// Empties the command ring on the game thread: every command runs
    /// through the game's own functions inside this frame, or ends with why
    /// it could not. Commands carry the basis their producer read, so this
    /// needs no read of its own; what the hook itself read (the comparison
    /// ring) is the fallback.
    fn drain_commands(&self, measured: Option<CommandBasis>) {
        let Some(block) = self.commands else {
            return;
        };
        // Leaving ends the match inside the game's own call, so nothing after
        // it in this drain has a match to run in.
        let mut left = false;
        while let Some(record) = commands::pop(block) {
            let basis = commands::take_basis(block).or(measured);
            let state = match &basis {
                Some(basis) if !left => unsafe { commands::execute(&self.memory, self.layout, basis, &record) },
                _ => commands::command_result::UNAVAILABLE,
            };
            left |= record.kind == bwrapi_engine::command::game_command_kind::LEAVE && state == commands::command_result::OK;
            commands::publish_result(
                block,
                record.id,
                state,
                basis.as_ref().map(|basis| basis.game_frame).unwrap_or(0),
            );
        }
    }

    /// The same frame counter the engine reads: in-game flag, then the
    /// BWGame root's frame count.
    fn game_frame(&self) -> Option<u32> {
        self.u32_at(self.match_root()?.checked_add(BWGAME_FRAME_COUNT_OFFSET)?)
    }

    /// The BWGame root of the running match; `None` outside a match.
    fn match_root(&self) -> Option<usize> {
        if self.module_u32(self.layout.scalars.session_is_in_game_offset)? == 0 {
            return None;
        }
        let root = &self.layout.menu_root;
        let width = self.layout.pointer_bytes;
        let mut raw = [0; 8];
        let address = self.memory.main_module().range_address(root.pointer, width)?;
        self.memory.read_into(address, &mut raw[..width]).ok()?;
        let root = read_pointer(&raw, 0, width)? ^ root.xor_key as usize;
        (root != 0).then_some(root)
    }

    fn module_u32(&self, rva: usize) -> Option<u32> {
        self.u32_at(self.memory.main_module().range_address(rva, 4)?)
    }

    fn u32_at(&self, address: usize) -> Option<u32> {
        let mut raw = [0; 4];
        self.memory.read_into(address, &mut raw).ok()?;
        Some(u32::from_le_bytes(raw))
    }
}
