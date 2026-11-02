//! Public C ABI 1.0 over the engine. The C header and the C#/Python
//! declarations are generated from the engine's table definitions
//! ([`generate`]). The `bwrapi.dll` built from this crate is also the DLL
//! injected into the game: it carries the frame hook and the shared blocks
//! of the `bwrapi_game` half, so an in-game module reaches one copy of them
//! through this ABI instead of linking its own.
//!
//! A session is used by one thread at a time. A frame stays valid until
//! `bwr_release`, which any thread may call; frames do not change while held.
//!
//! The doc comments of the exported functions, of the `#[repr(C)]` types and
//! their fields, and of the [`BwrStatus`] variants are the C ABI contract:
//! [`generate`] copies them into the header, the bindings and
//! `docs/C-ABI.md`.

use std::cell::RefCell;
use std::ffi::{c_char, CStr, CString};
use std::sync::Arc;

use bwrapi_engine::{tables::TABLES, Engine, Error, Frame};

mod command;
pub mod generate;
#[cfg(windows)]
mod ring;

pub use command::{BwrCommand, BwrResult};

pub const ABI_MAJOR: u32 = 1;
pub const ABI_MINOR: u32 = 0;

/// What a call returns; `bwr_last_error` gives the message of a failure.
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BwrStatus {
    /// Success.
    Ok = 0,
    /// No match is running; for `bwr_submit`, no frame of a match was
    /// acquired; for the in-game calls, this process runs no frame hook.
    NotInGame = 1,
    /// `bwr_step` saw no newer game frame before its timeout.
    Timeout = 2,
    /// The game process exited or cannot be read; `bwr_open` on a system
    /// other than Windows; a failed `bwr_inject`.
    Gone = 3,
    /// A bad argument: a NULL pointer, a wrong `size` field, a malformed
    /// command, or a call that needs a `bwr_open_self` session.
    Invalid = 4,
    /// The game build has no layout in this `bwrapi.dll`.
    UnsupportedBuild = 5,
    /// The game runs no in-game DLL, so nothing executes commands.
    NoCommands = 6,
}

/// Frame header followed by one table pointer per source, in source bit
/// order; a pointer is NULL when the frame does not carry that source.
/// A table is `{ size_t rows; const T* column...; }`: one array of `rows`
/// cells per column, in the column order of its source.
#[repr(C)]
pub struct BwrFrame {
    /// `sizeof(BwrFrame)`.
    pub size: u32,
    /// `BWR_FRAME_*` bits.
    pub flags: u32,
    /// Increases by one with every frame this session acquires.
    pub sequence: u64,
    /// `BWR_SOURCE_*` bits of the tables this frame carries: the subscribed
    /// sources that could be read.
    pub sources: u64,
    /// Increases when a new match starts.
    pub match_id: u32,
    /// Game frame number; `UINT32_MAX` when unknown.
    pub game_frame: u32,
    /// Map width in build tiles; 0 unless the frame was read with
    /// `BWR_SOURCE_VISION`.
    pub map_width: u16,
    /// Map height in build tiles; 0 as `map_width`.
    pub map_height: u16,
    /// Local player id; 255 when unknown (replays, observers).
    pub local_player: u8,
    /// Always 0.
    pub reserved: [u8; 3],
    /// Each points at `{ size_t rows; const T* column... }`.
    pub tables: [*const usize; TABLE_COUNT],
}

pub const TABLE_COUNT: usize = 19;
const _: () = assert!(TABLES.len() == TABLE_COUNT);

pub struct BwrSession {
    engine: Engine,
    error: CString,
    /// The shared command ring of the game's in-game DLL. A command queued
    /// on it runs at the next frame boundary, or at this one when queued
    /// inside a frame callback. `None` when the game runs no in-game DLL,
    /// which leaves `bwr_submit` refused.
    #[cfg(windows)]
    ring: Option<bwrapi_game::GameCommands>,
    /// Whether `bwr_open_self` made this session, the only kind allowed a
    /// frame callback.
    is_self: bool,
}

impl BwrSession {
    pub fn new(engine: Engine) -> Self {
        Self {
            engine,
            error: CString::default(),
            #[cfg(windows)]
            ring: None,
            is_self: false,
        }
    }
}

/// What `bwr_acquire` hands out; `view` comes first so the frame pointer is
/// the lease pointer.
#[repr(C)]
struct Lease {
    view: BwrFrame,
    _frame: Arc<Frame>,
    _tables: Vec<Box<[usize]>>,
}

thread_local! {
    static OPEN_ERROR: RefCell<CString> = RefCell::new(CString::default());
}

fn message(text: impl ToString) -> CString {
    CString::new(text.to_string().replace('\0', " ")).unwrap_or_default()
}

fn status(error: &Error) -> BwrStatus {
    match error {
        Error::NotInGame => BwrStatus::NotInGame,
        Error::Timeout => BwrStatus::Timeout,
        Error::Gone | Error::Memory(_) => BwrStatus::Gone,
        Error::UnsupportedBuild => BwrStatus::UnsupportedBuild,
    }
}

fn lease(frame: Arc<Frame>) -> Box<Lease> {
    let mut tables = Vec::new();
    let mut pointers = [std::ptr::null(); TABLE_COUNT];
    for (index, info) in TABLES.iter().enumerate() {
        if frame.sources & 1 << index == 0 {
            continue;
        }
        let table = frame.table(index).expect("a table per source");
        let view: Box<[usize]> = std::iter::once(table.rows())
            .chain((0..info.columns.len()).map(|column| table.column_ptr(column) as usize))
            .collect();
        pointers[index] = view.as_ptr();
        tables.push(view);
    }
    Box::new(Lease {
        view: BwrFrame {
            size: std::mem::size_of::<BwrFrame>() as u32,
            flags: frame.flags,
            sequence: frame.sequence,
            sources: frame.sources,
            match_id: frame.match_id,
            game_frame: frame.game_frame,
            map_width: frame.map_width,
            map_height: frame.map_height,
            local_player: frame.local_player,
            reserved: [0; 3],
            tables: pointers,
        },
        _frame: frame,
        _tables: tables,
    })
}

/// ABI version of this `bwrapi.dll`: `major << 16 | minor`. Compare the
/// major number with the header's `BWR_ABI_MAJOR`.
#[no_mangle]
pub extern "C" fn bwr_abi_version() -> u32 {
    ABI_MAJOR << 16 | ABI_MINOR
}

/// Opens a session on game `pid`. When the game runs the in-game DLL the
/// session copies the frames its game thread publishes and `bwr_submit`
/// queues on its command ring; otherwise it reads the game memory from this
/// process and commands are refused. The tables are the same either way;
/// frames the in-game DLL publishes are taken at the frame boundary, while
/// reading from this process can see a frame the game is still updating
/// (`BWR_FRAME_TORN` only reports a frame number that changed during the read).
/// `flags` is reserved; pass 0. No match needs to run yet. Fails with
/// `BWR_GONE` when the process cannot be opened and `BWR_UNSUPPORTED_BUILD`
/// when its build has no layout; `bwr_last_error(NULL)` gives the message.
/// The session is used by one thread at a time and closed with `bwr_close`.
///
/// # Safety
/// `out` must be valid for one pointer write.
#[no_mangle]
pub unsafe extern "C" fn bwr_open(pid: u32, flags: u32, out: *mut *mut BwrSession) -> BwrStatus {
    let _ = flags;
    if out.is_null() {
        return BwrStatus::Invalid;
    }
    *out = std::ptr::null_mut();
    let fail = |status, text: &str| {
        OPEN_ERROR.with(|error| *error.borrow_mut() = message(text));
        status
    };
    #[cfg(windows)]
    {
        let mut engine = match Engine::open(pid) {
            Ok(engine) => engine,
            Err(error) => return fail(status(&error), &error.to_string()),
        };
        // A game running the in-game DLL publishes the frames its game
        // thread reads; copying those replaces decoding the game from here.
        if let Some(publication) = bwrapi_game::GamePublication::open(pid) {
            engine.read_published(Box::new(publication));
        }
        let mut session = BwrSession::new(engine);
        session.ring = ring::open(pid);
        *out = Box::into_raw(Box::new(session));
        BwrStatus::Ok
    }
    #[cfg(not(windows))]
    {
        let _ = pid;
        fail(BwrStatus::Gone, "reading a game process requires Windows")
    }
}

/// Opens a session on the calling process: the game thread reads its own
/// memory at the frame boundary instead of through another process, and
/// `bwr_submit` queues on the in-game command ring when this process runs
/// one. This is the only session `bwr_set_frame_callback`,
/// `bwr_capture_chat` and `bwr_draw` accept. The session type and lifetime
/// rules are those of [`bwr_open`]; failures read with `bwr_last_error(NULL)`.
///
/// # Safety
/// `out` must be valid for one pointer write.
#[no_mangle]
pub unsafe extern "C" fn bwr_open_self(out: *mut *mut BwrSession) -> BwrStatus {
    if out.is_null() {
        return BwrStatus::Invalid;
    }
    *out = std::ptr::null_mut();
    match Engine::in_process() {
        Ok(engine) => {
            let mut session = BwrSession::new(engine);
            session.is_self = true;
            // Reading works without commands; a missing ring (this process
            // runs no in-game DLL) only leaves `bwr_submit` refused.
            #[cfg(windows)]
            {
                session.ring = bwrapi_game::GameCommands::open(std::process::id());
            }
            *out = Box::into_raw(Box::new(session));
            BwrStatus::Ok
        }
        Err(error) => {
            OPEN_ERROR.with(|slot| *slot.borrow_mut() = message(&error));
            status(&error)
        }
    }
}

/// Registers `callback` on `session`, which must come from `bwr_open_self`:
/// the frame hook calls it once per game frame on the game thread, and the
/// game waits until it returns. Inside the callback the session may
/// `bwr_acquire`, `bwr_release`, `bwr_submit` and `bwr_poll_result`; what
/// `bwr_submit` queues there runs at the same frame boundary, right after
/// the callback returns. `NULL` removes the callback; a later registration
/// replaces the earlier one. Fails with `BWR_INVALID` for a `bwr_open`
/// session and with `BWR_NOT_IN_GAME` when this process runs no frame hook,
/// so nothing would call it.
///
/// # Safety
/// `session` is from `bwr_open` or `bwr_open_self`; the callback, when not
/// NULL, stays valid until a later `bwr_set_frame_callback` on this
/// process.
#[no_mangle]
pub unsafe extern "C" fn bwr_set_frame_callback(
    session: *mut BwrSession,
    callback: Option<extern "C" fn(*mut std::ffi::c_void)>,
    user: *mut std::ffi::c_void,
) -> BwrStatus {
    let Some(session) = session.as_mut() else {
        return BwrStatus::Invalid;
    };
    if !session.is_self {
        return BwrStatus::Invalid;
    }
    #[cfg(windows)]
    match bwrapi_game::set_frame_callback(callback, user) {
        Ok(()) => BwrStatus::Ok,
        Err(text) => {
            session.error = message(text);
            BwrStatus::NotInGame
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (callback, user);
        session.error = message("frame callbacks need the Windows frame hook");
        BwrStatus::NotInGame
    }
}

/// Closes `session`; NULL does nothing. Frames acquired from it stay valid
/// until they are released.
///
/// # Safety
/// `session` is NULL or from `bwr_open`/`bwr_open_self`, and not used afterwards.
#[no_mangle]
pub unsafe extern "C" fn bwr_close(session: *mut BwrSession) {
    if !session.is_null() {
        drop(Box::from_raw(session));
    }
}

/// Selects the sources (`BWR_SOURCE_*` bits) the following `bwr_acquire`
/// calls read; a new session reads none. Fewer sources make a faster read.
///
/// # Safety
/// `session` is from `bwr_open` or `bwr_open_self`.
#[no_mangle]
pub unsafe extern "C" fn bwr_subscribe(session: *mut BwrSession, sources: u64) -> BwrStatus {
    match session.as_mut() {
        Some(session) => {
            session.engine.subscribe(sources);
            BwrStatus::Ok
        }
        None => BwrStatus::Invalid,
    }
}

/// Reads the subscribed sources of the current game frame into a new frame
/// and writes it to `*out` (NULL on failure). The frame and its tables stay
/// valid and unchanged until `bwr_release`; several frames may be held at
/// once. `sources` names the tables it carries: a subscribed source that
/// could not be read is missing and its table pointer is NULL. Does not wait
/// for a new game frame (see `bwr_step`). On a game running the in-game DLL
/// the frame is the copy its game thread published at the frame boundary;
/// right after a subscription change the call waits briefly for the new
/// sources. Fails with `BWR_NOT_IN_GAME` outside a match and `BWR_GONE` when
/// the process exited.
///
/// # Safety
/// `session` is from `bwr_open` or `bwr_open_self`; `out` is valid for one pointer write.
#[no_mangle]
pub unsafe extern "C" fn bwr_acquire(session: *mut BwrSession, out: *mut *const BwrFrame) -> BwrStatus {
    let (Some(session), false) = (session.as_mut(), out.is_null()) else {
        return BwrStatus::Invalid;
    };
    *out = std::ptr::null();
    // The in-game watcher's turn-send stamp and the repair and refund totals
    // reach the tables only in the process that runs the frame hook.
    #[cfg(windows)]
    if session.is_self {
        session.engine.set_turn_watch(bwrapi_game::turn_watch());
        session.engine.set_resource_totals(bwrapi_game::resource_totals());
    }
    match session.engine.acquire() {
        Ok(frame) => {
            *out = Box::into_raw(lease(frame)).cast::<BwrFrame>();
            BwrStatus::Ok
        }
        Err(error) => {
            session.error = message(&error);
            status(&error)
        }
    }
}

/// Waits up to `timeout_ms` for a game frame newer than the one this session
/// acquired last, or a new match; `bwr_acquire` then reads it. Returns
/// `BWR_OK` at once when there already is one, `BWR_TIMEOUT` when none came,
/// `BWR_NOT_IN_GAME` outside a match and `BWR_GONE` when the process exited.
/// The game may have stepped more than one frame since; compare
/// `game_frame`. A frame callback runs once per game frame and needs no step.
///
/// # Safety
/// `session` is from `bwr_open` or `bwr_open_self`.
#[no_mangle]
pub unsafe extern "C" fn bwr_step(session: *mut BwrSession, timeout_ms: u32) -> BwrStatus {
    let Some(session) = session.as_mut() else {
        return BwrStatus::Invalid;
    };
    match session.engine.step(std::time::Duration::from_millis(u64::from(timeout_ms))) {
        Ok(()) => BwrStatus::Ok,
        Err(error) => {
            session.error = message(&error);
            status(&error)
        }
    }
}

/// Releases a frame from `bwr_acquire`; NULL does nothing. Any thread may
/// call it, also after `bwr_close`: `session` is not read.
///
/// # Safety
/// `frame` is NULL or from `bwr_acquire` and not released before.
#[no_mangle]
pub unsafe extern "C" fn bwr_release(_session: *mut BwrSession, frame: *const BwrFrame) {
    if !frame.is_null() {
        drop(Box::from_raw(frame.cast_mut().cast::<Lease>()));
    }
}

/// Queues `count` commands and returns at once; the ids written to `out_ids`
/// (one per command) name them in `bwr_poll_result`. Needs a game running
/// the in-game DLL (else `BWR_NO_COMMANDS`) and an acquired frame in a match
/// (else `BWR_NOT_IN_GAME`). `BWR_INVALID` queues none: NULL `commands` or
/// `out_ids`, `count` 0, a `size` other than `sizeof(BwrCommand)`, a unit
/// kind with `unit_count` 0, or NULL `units` with `unit_count` above 0.
/// Units are named by the ids of the last acquired frame. Commands queue on
/// the game's command ring and run at its next frame boundary; inside a
/// frame callback that is this very boundary. What the game thread finds
/// wrong with a command (fields its kind does not take, more than 24 units)
/// ends in its result, not in the status. A command the ring did not take
/// gets id 0.
///
/// # Safety
/// `session` is from `bwr_open` or `bwr_open_self`; `commands` holds `count` commands whose
/// `units` arrays are valid; `out_ids` is valid for `count` writes.
#[no_mangle]
pub unsafe extern "C" fn bwr_submit(
    session: *mut BwrSession,
    commands: *const BwrCommand,
    count: u32,
    out_ids: *mut u64,
) -> BwrStatus {
    let Some(session) = session.as_mut() else {
        return BwrStatus::Invalid;
    };
    if commands.is_null() || out_ids.is_null() || count == 0 {
        return BwrStatus::Invalid;
    }
    let Some(parsed) = std::slice::from_raw_parts(commands, count as usize)
        .iter()
        .map(|command| command::convert(command))
        .collect::<Option<Vec<_>>>()
    else {
        return BwrStatus::Invalid;
    };
    #[cfg(windows)]
    {
        let Some(ring) = session.ring.as_ref() else {
            session.error = message("this game runs no in-game DLL: inject bwrapi.dll to issue commands");
            return BwrStatus::NoCommands;
        };
        let Some(basis) = session.engine.command_basis() else {
            return BwrStatus::NotInGame;
        };
        for (index, command) in parsed.iter().enumerate() {
            let id = match ring.submit(&basis, command) {
                Ok(id) => id,
                Err(text) => {
                    session.error = message(text);
                    0
                }
            };
            *out_ids.add(index) = id;
        }
        BwrStatus::Ok
    }
    #[cfg(not(windows))]
    {
        let _ = (session, parsed);
        BwrStatus::NoCommands
    }
}

/// Current result of a command id from `bwr_submit`, without waiting. A
/// queued command reads `BWR_RESULT_PENDING` until the frame boundary
/// executes it; id 0 reads `BWR_RESULT_UNAVAILABLE`. The game keeps the
/// results of the last 1024 command ids; an older id reads
/// `BWR_RESULT_PENDING` again. Fails with `BWR_NO_COMMANDS` when the game
/// runs no in-game DLL.
///
/// # Safety
/// `session` is from `bwr_open` or `bwr_open_self`; `out` is valid for one write.
#[no_mangle]
pub unsafe extern "C" fn bwr_poll_result(session: *mut BwrSession, id: u64, out: *mut BwrResult) -> BwrStatus {
    let (Some(session), false) = (session.as_mut(), out.is_null()) else {
        return BwrStatus::Invalid;
    };
    #[cfg(windows)]
    {
        let Some(ring) = session.ring.as_ref() else {
            session.error = message("this game runs no in-game DLL: inject bwrapi.dll to issue commands");
            return BwrStatus::NoCommands;
        };
        *out = ring::poll(ring, id);
        BwrStatus::Ok
    }
    #[cfg(not(windows))]
    {
        let _ = (session, id);
        BwrStatus::NoCommands
    }
}

/// Loads the module DLL at `module` (UTF-8 path) into running game `pid`
/// through a window hook: `export_name` names the module's `WH_CALLWNDPROC`
/// hook procedure, which runs on the game thread and installs the module
/// there. The hook is removed after its first message, so a module that
/// stays must pin itself in memory. The `bwrapi.dll` beside the module is
/// loaded first when the game does not run one yet. Loading the same file again
/// does not load a second copy. The module must have the game's pointer
/// width; a game of the other width is loaded by the `bwrapi-inject.exe`
/// beside the module. Fails with `BWR_INVALID` for a NULL or non-UTF-8
/// argument and with `BWR_GONE` when the load fails; the message is read
/// with `bwr_last_error(NULL)`.
///
/// # Safety
/// `module` and `export_name` are NUL-terminated strings.
#[no_mangle]
pub unsafe extern "C" fn bwr_inject(pid: u32, module: *const c_char, export: *const c_char) -> BwrStatus {
    if module.is_null() || export.is_null() {
        return BwrStatus::Invalid;
    }
    let (Ok(module), Ok(export)) = (CStr::from_ptr(module).to_str(), CStr::from_ptr(export).to_str()) else {
        return BwrStatus::Invalid;
    };
    #[cfg(windows)]
    let result = bwrapi_game::inject_module(pid, std::path::Path::new(module), export);
    #[cfg(not(windows))]
    let result: Result<(), String> = {
        let _ = (pid, module, export);
        Err("injecting into a game requires Windows".to_owned())
    };
    match result {
        Ok(()) => BwrStatus::Ok,
        Err(error) => {
            OPEN_ERROR.with(|slot| *slot.borrow_mut() = message(&error));
            BwrStatus::Gone
        }
    }
}

/// Message of the session's last failure, or of the calling thread's last
/// `bwr_open`, `bwr_open_self` or `bwr_inject` failure when `session` is
/// NULL; empty when there was none. UTF-8, valid until the next call on the
/// same session or thread.
///
/// # Safety
/// `session` is NULL or from `bwr_open`/`bwr_open_self`.
#[no_mangle]
pub unsafe extern "C" fn bwr_last_error(session: *const BwrSession) -> *const c_char {
    match session.as_ref() {
        Some(session) => session.error.as_ptr(),
        None => OPEN_ERROR.with(|error| error.borrow().as_ptr()),
    }
}

/// `BwrChat.kind` values; 0 is no line.
/// - `SENT`: A line the local player typed, kept instead of sent.
/// - `RECEIVED`: A line the game recorded as received.
pub const CHAT_KINDS: [(&str, u32); 2] = [("SENT", 1), ("RECEIVED", 2)];
#[cfg(windows)]
const _: () = assert!(
    bwrapi_game::chat_kind::SENT == CHAT_KINDS[0].1 && bwrapi_game::chat_kind::RECEIVED == CHAT_KINDS[1].1
);

/// One chat line from `bwr_next_chat`: NUL-terminated UTF-8, cut to fit.
#[repr(C)]
pub struct BwrChat {
    /// `BWR_CHAT_*`; 0 when no line waits.
    pub kind: u32,
    /// The game player number a received line came from; meaningless for a
    /// sent line.
    pub player: u32,
    /// NUL-terminated UTF-8.
    pub text: [u8; 256],
}

fn copy_text(out: &mut [u8], text: &[u8]) {
    let length = text.len().min(out.len() - 1);
    out[..length].copy_from_slice(&text[..length]);
    out[length..].fill(0);
}

/// Turns the in-game chat capture on (`enable` 1) or off (0) on `session`,
/// which must come from `bwr_open_self`. While it is on, a line the local
/// player types into the chat box is not sent but kept, and the lines the
/// game records as received are kept with their sending player;
/// `bwr_next_chat` takes them. Turning it off drops what was not taken.
/// Fails with `BWR_NOT_IN_GAME` when this process runs no frame hook.
///
/// # Safety
/// `session` is from `bwr_open` or `bwr_open_self`.
#[no_mangle]
pub unsafe extern "C" fn bwr_capture_chat(session: *mut BwrSession, enable: u32) -> BwrStatus {
    let Some(session) = session.as_mut() else {
        return BwrStatus::Invalid;
    };
    if !session.is_self {
        return BwrStatus::Invalid;
    }
    #[cfg(windows)]
    match bwrapi_game::set_chat_capture(enable != 0) {
        Ok(()) => BwrStatus::Ok,
        Err(text) => {
            session.error = message(text);
            BwrStatus::NotInGame
        }
    }
    #[cfg(not(windows))]
    {
        let _ = enable;
        session.error = message("chat capture needs the Windows frame hook");
        BwrStatus::NotInGame
    }
}

/// Takes the oldest kept chat line into `out`; `out->kind` is 0 when none
/// waits.
///
/// # Safety
/// `session` is from `bwr_open` or `bwr_open_self`; `out` is valid for one write.
#[no_mangle]
pub unsafe extern "C" fn bwr_next_chat(session: *mut BwrSession, out: *mut BwrChat) -> BwrStatus {
    let (Some(_), Some(out)) = (session.as_mut(), out.as_mut()) else {
        return BwrStatus::Invalid;
    };
    #[cfg(windows)]
    let (kind, player, text) = match bwrapi_game::next_chat() {
        Some(line) => (line.kind, line.player, line.text),
        None => (0, u32::MAX, Vec::new()),
    };
    #[cfg(not(windows))]
    let (kind, player, text) = (0, u32::MAX, Vec::<u8>::new());
    out.kind = kind;
    out.player = player;
    copy_text(&mut out.text, &text);
    BwrStatus::Ok
}

/// `BwrShape.kind` values, BWAPI's shape types.
/// - `TEXT`: Text at (`x1`, `y1`).
/// - `BOX`: Rectangle from (`x1`, `y1`) to (`x2`, `y2`).
/// - `TRIANGLE`: Corners (`x1`, `y1`), (`x2`, `y2`), (`extra1`, `extra2`).
/// - `CIRCLE`: Center (`x1`, `y1`), radius `extra1`.
/// - `ELLIPSE`: Center (`x1`, `y1`), radii `extra1` (x) and `extra2` (y).
/// - `DOT`: One pixel at (`x1`, `y1`).
/// - `LINE`: From (`x1`, `y1`) to (`x2`, `y2`).
pub const SHAPE_KINDS: [(&str, u8); 7] =
    [("TEXT", 1), ("BOX", 2), ("TRIANGLE", 3), ("CIRCLE", 4), ("ELLIPSE", 5), ("DOT", 6), ("LINE", 7)];
/// `BwrShape.coordinates` values, BWAPI's coordinate types.
/// - `SCREEN`: Pixels from the top-left of the game screen.
/// - `MAP`: Map pixels.
/// - `MOUSE`: Pixels from the cursor.
pub const SHAPE_COORDINATES: [(&str, u8); 3] = [("SCREEN", 1), ("MAP", 2), ("MOUSE", 3)];
#[cfg(windows)]
const _: () = assert!(
    bwrapi_game::draw::shape_kind::TEXT == SHAPE_KINDS[0].1
        && bwrapi_game::draw::shape_kind::LINE == SHAPE_KINDS[6].1
        && bwrapi_game::draw::coordinates::MOUSE == SHAPE_COORDINATES[2].1
);

/// One shape of [`BwrDrawList`]: BWAPI's shape, with its corners, radii and
/// third triangle corner in `x1`..`extra2` as BWAPI keeps them, its color, and
/// for text the size and the UTF-8 bytes `text_offset .. text_offset +
/// text_len` of the list's text.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BwrShape {
    /// `BWR_SHAPE_*`.
    pub kind: u8,
    /// `BWR_COORDINATES_*`.
    pub coordinates: u8,
    /// 1 fills a box, triangle, circle or ellipse; 0 draws its outline.
    pub solid: u8,
    /// Text size, 0 small .. 3 huge.
    pub text_size: u8,
    /// First corner, center or position x.
    pub x1: i32,
    /// First corner, center or position y.
    pub y1: i32,
    /// Second corner x of a box, triangle or line.
    pub x2: i32,
    /// Second corner y of a box, triangle or line.
    pub y2: i32,
    /// Third triangle corner x, or the x radius of a circle or ellipse.
    pub extra1: i32,
    /// Third triangle corner y, or the y radius of an ellipse.
    pub extra2: i32,
    /// Color bytes R, G, B, A in memory order.
    pub rgba: u32,
    /// First byte of a text shape's UTF-8 in the list's `text`.
    pub text_offset: u32,
    /// Bytes of a text shape's UTF-8.
    pub text_len: u32,
}

/// The shapes [`bwr_draw`] draws, in drawing order, their text, and the RGBA
/// of each text color code (BWAPI's Text codes; a text starts in code 2).
#[repr(C)]
pub struct BwrDrawList {
    /// `sizeof(BwrDrawList)`.
    pub size: u32,
    /// Shapes at `shapes`.
    pub count: u32,
    /// The shapes, in drawing order.
    pub shapes: *const BwrShape,
    /// UTF-8 bytes the text shapes point into; need not be NUL-terminated.
    pub text: *const u8,
    /// Bytes at `text`.
    pub text_bytes: u32,
    /// RGBA of each text color code.
    pub text_colors: [u32; 32],
}

#[cfg(windows)]
const _: () = assert!(
    std::mem::size_of::<BwrShape>() == std::mem::size_of::<bwrapi_game::DrawShape>()
        && std::mem::offset_of!(BwrShape, rgba) == std::mem::offset_of!(bwrapi_game::DrawShape, rgba)
        && std::mem::offset_of!(BwrShape, text_len) == std::mem::offset_of!(bwrapi_game::DrawShape, text_len)
);

/// Draws `list` into the game screen on `session`, which must come from
/// `bwr_open_self`: every screen update the game draws until the next call
/// shows the list, above the game screen and its console and below the
/// cursor. The list is copied before the call returns; `NULL` or an empty
/// list draws nothing. A game whose screen updates are off (`SET_GUI` 0)
/// draws nothing until they are on again. Fails with `BWR_INVALID` when a
/// text shape names bytes outside the text, and with `BWR_NOT_IN_GAME` when
/// this process runs no frame hook.
///
/// # Safety
/// `session` is from `bwr_open` or `bwr_open_self`; `list` is NULL or holds
/// `count` shapes and `text_bytes` bytes of text.
#[no_mangle]
pub unsafe extern "C" fn bwr_draw(session: *mut BwrSession, list: *const BwrDrawList) -> BwrStatus {
    let Some(session) = session.as_mut() else {
        return BwrStatus::Invalid;
    };
    if !session.is_self {
        return BwrStatus::Invalid;
    }
    let (shapes, text, colors): (&[BwrShape], &[u8], [u32; 32]) = match list.as_ref() {
        None => (&[], &[], [0; 32]),
        Some(list) if list.size as usize != std::mem::size_of::<BwrDrawList>() => return BwrStatus::Invalid,
        Some(list) => (
            slice(list.shapes, list.count as usize),
            slice(list.text, list.text_bytes as usize),
            list.text_colors,
        ),
    };
    #[cfg(windows)]
    {
        let result = bwrapi_game::set_draw(
            std::slice::from_raw_parts(shapes.as_ptr().cast::<bwrapi_game::DrawShape>(), shapes.len()),
            text,
            &colors,
        );
        match result {
            Ok(()) => BwrStatus::Ok,
            Err(error) => {
                session.error = message(error.message());
                match error {
                    bwrapi_game::DrawError::NoFrameHook => BwrStatus::NotInGame,
                    bwrapi_game::DrawError::TextOutside => BwrStatus::Invalid,
                }
            }
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (shapes, text, colors);
        session.error = message("drawing needs the Windows frame hook");
        BwrStatus::NotInGame
    }
}

/// `count` items at `data`; empty when there are none.
unsafe fn slice<'a, T>(data: *const T, count: usize) -> &'a [T] {
    if data.is_null() || count == 0 {
        &[]
    } else {
        std::slice::from_raw_parts(data, count)
    }
}

/// Window-hook entry the injector loads this DLL through; the frame hook
/// installs from the game thread on the first call. Not part of the
/// generated header: only [`bwrapi_game::inject`] looks it up.
///
/// # Safety
/// Called only by the Windows hook machinery.
#[cfg(windows)]
#[no_mangle]
pub unsafe extern "system" fn bwr_callwndproc(
    code: i32,
    wparam: usize,
    lparam: isize,
) -> isize {
    bwrapi_game::callwndproc(code, wparam, lparam)
}

#[cfg(test)]
mod tests;
