//! Command submission: the C types and their conversion to engine commands.

use bwrapi_engine::command::UnitCommand;

/// `BwrCommand.flags` bits.
/// - `QUEUED`: Adds the order after the unit's queued orders; only the kinds
///   marked "Queues" take it.
/// - `TILE`: Marks x/y as build tiles for the caller's own bookkeeping; the
///   kind alone decides how x/y are read.
pub const COMMAND_FLAGS: [(&str, u32); 2] = [("QUEUED", 1), ("TILE", 2)];

/// `BwrCommand.kind` values of the unit commands, in kind order (BWAPI's
/// UnitCommandTypes). Each names the fields it reads besides `units` (1..24
/// acting unit ids); x/y are map pixels (0..65535) unless they say build
/// tiles. "Queues" marks the kinds that take `BWR_COMMAND_FLAG_QUEUED`, "one
/// unit" the kinds that take exactly one acting unit.
/// - `MOVE`: x, y. Queues.
/// - `SMART_TARGET_UNIT`: target; a right click on it. Queues.
/// - `ATTACK_UNIT`: target. Queues.
/// - `ATTACK_POSITION`: x, y. Queues.
/// - `SET_RALLY_POSITION`: x, y.
/// - `BUILD`: type_id (building unit type); x, y the top-left build tile.
///   One unit.
/// - `BUILD_ADDON`: type_id (addon unit type). One unit.
/// - `TRAIN`: type_id (unit type).
/// - `RESEARCH`: type_id (tech type). One unit.
/// - `UPGRADE`: type_id (upgrade type). One unit.
/// - `LIFT`: No fields.
/// - `LAND`: x, y the top-left build tile. One unit.
/// - `STOP`: No fields. Queues.
/// - `HOLD_POSITION`: No fields. Queues.
/// - `MORPH`: type_id (unit type).
/// - `SET_RALLY_UNIT`: target.
/// - `PATROL`: x, y. Queues.
/// - `FOLLOW`: target. Queues.
/// - `GATHER`: target, a mineral field or gas building. Queues.
/// - `RETURN_CARGO`: No fields. Queues.
/// - `REPAIR`: target. Queues.
/// - `BURROW`: No fields.
/// - `UNBURROW`: No fields.
/// - `CLOAK`: No fields.
/// - `DECLOAK`: No fields.
/// - `SIEGE`: No fields.
/// - `UNSIEGE`: No fields.
/// - `LOAD`: target, the transport or Bunker to enter or the unit to take
///   in. Queues.
/// - `UNLOAD`: target, the passenger.
/// - `UNLOAD_ALL`: No fields. Queues.
/// - `UNLOAD_ALL_POSITION`: x, y. Queues.
/// - `RIGHT_CLICK_POSITION`: x, y. Queues.
/// - `HALT_CONSTRUCTION`: No fields.
/// - `CANCEL_CONSTRUCTION`: No fields.
/// - `CANCEL_ADDON`: No fields.
/// - `CANCEL_TRAIN`: Cancels the last unit of the training queue.
/// - `CANCEL_TRAIN_SLOT`: slot, the training queue slot from 0.
/// - `CANCEL_MORPH`: No fields.
/// - `CANCEL_RESEARCH`: No fields.
/// - `CANCEL_UPGRADE`: No fields.
/// - `USE_TECH`: type_id, a tech without a target: 0 Stim Packs, 5 Tank
///   Siege Mode, 9 Personnel Cloaking, 10 Cloaking Field, 11 Burrowing.
/// - `USE_TECH_POSITION`: type_id (tech type), x, y.
/// - `USE_TECH_UNIT`: type_id (tech type), target. Archon Warp (23) and
///   Dark Archon Meld (28) may leave target 0 to merge the acting units.
/// - `PLACE_COP`: x, y the top-left build tile. One unit.
pub const COMMAND_KINDS: [(&str, u32); 44] = {
    use bwrapi_engine::command::command_kind::*;
    [
        ("MOVE", MOVE), ("SMART_TARGET_UNIT", SMART_TARGET_UNIT), ("ATTACK_UNIT", ATTACK_UNIT),
        ("ATTACK_POSITION", ATTACK_POSITION), ("SET_RALLY_POSITION", SET_RALLY_POSITION), ("BUILD", BUILD),
        ("BUILD_ADDON", BUILD_ADDON), ("TRAIN", TRAIN), ("RESEARCH", RESEARCH), ("UPGRADE", UPGRADE),
        ("LIFT", LIFT), ("LAND", LAND), ("STOP", STOP), ("HOLD_POSITION", HOLD_POSITION), ("MORPH", MORPH),
        ("SET_RALLY_UNIT", SET_RALLY_UNIT), ("PATROL", PATROL), ("FOLLOW", FOLLOW), ("GATHER", GATHER),
        ("RETURN_CARGO", RETURN_CARGO), ("REPAIR", REPAIR), ("BURROW", BURROW), ("UNBURROW", UNBURROW),
        ("CLOAK", CLOAK), ("DECLOAK", DECLOAK), ("SIEGE", SIEGE), ("UNSIEGE", UNSIEGE), ("LOAD", LOAD),
        ("UNLOAD", UNLOAD), ("UNLOAD_ALL", UNLOAD_ALL), ("UNLOAD_ALL_POSITION", UNLOAD_ALL_POSITION),
        ("RIGHT_CLICK_POSITION", RIGHT_CLICK_POSITION), ("HALT_CONSTRUCTION", HALT_CONSTRUCTION),
        ("CANCEL_CONSTRUCTION", CANCEL_CONSTRUCTION), ("CANCEL_ADDON", CANCEL_ADDON),
        ("CANCEL_TRAIN", CANCEL_TRAIN), ("CANCEL_TRAIN_SLOT", CANCEL_TRAIN_SLOT),
        ("CANCEL_MORPH", CANCEL_MORPH), ("CANCEL_RESEARCH", CANCEL_RESEARCH),
        ("CANCEL_UPGRADE", CANCEL_UPGRADE), ("USE_TECH", USE_TECH), ("USE_TECH_POSITION", USE_TECH_POSITION),
        ("USE_TECH_UNIT", USE_TECH_UNIT), ("PLACE_COP", PLACE_COP),
    ]
};

/// Kinds of the commands to the game itself, after the unit kinds. They name
/// no units (`unit_count` 0, `target` 0); text is UTF-8 of at most 191 bytes.
/// - `PAUSE`: Pauses the game.
/// - `RESUME`: Resumes a paused game.
/// - `RESTART`: Restarts the match.
/// - `LEAVE`: Leaves the match.
/// - `SET_SPEED`: x, the frame delay in ms for every game speed; negative
///   restores the game's own delays.
/// - `SET_FRAME_SKIP`: x, logical frames per screen update, at least 1.
/// - `PRINT_TEXT`: text, a line shown in the game's message area.
/// - `SEND_TEXT`: text, a chat line from the local player; type_id 1 sends
///   it to allies only, 0 to everyone.
/// - `SET_SCREEN_POSITION`: x, y, the map pixels of the screen's top-left
///   corner.
/// - `SET_ALLIES`: x, a player id 0..7; y, 0 not allied, 1 allied, 2 allied
///   victory. Matches only, never the local player.
/// - `SET_VISION`: x, a player id 0..7; y, 1 shares the local player's
///   vision with it, 0 stops. In a replay it shows or hides that player's
///   vision instead.
/// - `PING_MINIMAP`: x, y, the map pixels of the ping.
/// - `SET_REVEAL_ALL`: x, 1 reveals the whole map of a replay, 0 restores
///   its vision. Replays only.
/// - `SET_MAP`: text, the map file the next restart loads; also outside a
///   match.
/// - `SET_GUI`: x, 0 stops the game's screen updates, 1 lets them run again.
/// - `USE_CHEAT`: x, the whole set of the game's cheat flags to hold;
///   single-player matches only.
pub const GAME_COMMAND_KINDS: [(&str, u32); 16] = {
    use bwrapi_engine::command::game_command_kind::*;
    [
        ("PAUSE", PAUSE), ("RESUME", RESUME), ("RESTART", RESTART), ("LEAVE", LEAVE),
        ("SET_SPEED", SET_SPEED), ("SET_FRAME_SKIP", SET_FRAME_SKIP), ("PRINT_TEXT", PRINT_TEXT),
        ("SEND_TEXT", SEND_TEXT), ("SET_SCREEN_POSITION", SET_SCREEN_POSITION), ("SET_ALLIES", SET_ALLIES),
        ("SET_VISION", SET_VISION), ("PING_MINIMAP", PING_MINIMAP), ("SET_REVEAL_ALL", SET_REVEAL_ALL),
        ("SET_MAP", SET_MAP), ("SET_GUI", SET_GUI), ("USE_CHEAT", USE_CHEAT),
    ]
};

/// `BwrResult.state` values.
/// - `PENDING`: Not executed yet: queued for the next frame boundary, or an
///   id older than the last 1024.
/// - `OK`: The game's command functions ran. Whether the order changes the
///   game shows in later frames.
/// - `REJECTED`: The command does not fit its kind (a missing or out-of-range
///   field, more than 24 units, a kind that cannot queue, a type the kind
///   cannot use), or the game refuses it here (cheats in a network match or
///   replay, alliances in a replay or with oneself, reveal-all outside a
///   replay, a map file that cannot be opened).
/// - `UNAVAILABLE`: Nothing could execute it: no match ran at the frame
///   boundary (or a `LEAVE` before it at the same boundary ended the match),
///   the game build has no command functions, the queue stayed full for
///   250 ms, or the id is 0.
/// - `NOT_EXECUTED`: A named unit is no longer the unit the caller saw (it
///   died, or its slot holds a new unit), or a named player is not in the
///   game; no part of the command ran.
/// - `PANIC`: Reserved.
/// - `UNKNOWN`: Reserved.
pub const RESULT_STATES: [(&str, u32); 7] = [
    ("PENDING", 0),
    ("OK", 1),
    ("REJECTED", 2),
    ("UNAVAILABLE", 3),
    ("NOT_EXECUTED", 4),
    ("PANIC", 5),
    ("UNKNOWN", 6),
];

#[cfg(windows)]
pub const RESULT_PENDING: u32 = 0;
#[cfg(windows)]
pub const RESULT_UNAVAILABLE: u32 = 3;

/// One command for `bwr_submit`. `kind` is a `BWR_COMMAND_*` value; `x`/`y`
/// are pixels (build tiles for BUILD, LAND and PLACE_COP); `type_id` and
/// `slot` are read only by the kinds that take them; `units` lists the
/// acting unit ids. A field a kind does not read is ignored.
#[repr(C)]
pub struct BwrCommand {
    /// `sizeof(BwrCommand)`.
    pub size: u32,
    /// `BWR_COMMAND_*`.
    pub kind: u32,
    /// `BWR_COMMAND_FLAG_*` bits.
    pub flags: u32,
    /// Unit type (BUILD, BUILD_ADDON, TRAIN, MORPH), tech type (RESEARCH,
    /// USE_TECH*) or upgrade type (UPGRADE), in BWAPI numbering; 1 for
    /// SEND_TEXT to allies.
    pub type_id: u32,
    /// Training queue slot of CANCEL_TRAIN_SLOT.
    pub slot: u32,
    /// Map pixels, build tiles or a value, as the kind says.
    pub x: i32,
    /// Map pixels, build tiles or a value, as the kind says.
    pub y: i32,
    /// Target unit id; 0 none.
    pub target: u64,
    /// Acting unit ids from the last acquired frame; 1..24 for unit kinds.
    pub units: *const u64,
    /// Ids at `units`; 0 for game kinds.
    pub unit_count: u32,
    /// Not read; set 0.
    pub reserved: u32,
    /// NUL-terminated UTF-8 text of the text kinds, NULL otherwise.
    pub text: *const std::ffi::c_char,
}

/// What became of a submitted command. `actions` and `reserved` are 0.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BwrResult {
    /// The command id.
    pub id: u64,
    /// `BWR_RESULT_*`.
    pub state: u32,
    /// Game frame of the frame the command was submitted against, normally
    /// the submitter's last acquired one; 0 when the result was set before
    /// the command reached the game thread.
    pub frame_hint: u32,
    /// 0.
    pub actions: u32,
    /// 0.
    pub reserved: u32,
}

impl BwrResult {
    pub fn new(id: u64, state: u32) -> Self {
        Self { id, state, frame_hint: 0, actions: 0, reserved: 0 }
    }
}

/// Converts a caller's command, or `None` when it is malformed.
///
/// # Safety
/// `command.units` is valid for `unit_count` reads when `unit_count != 0`;
/// `command.text` is NULL or NUL-terminated.
pub(crate) unsafe fn convert(command: &BwrCommand) -> Option<UnitCommand> {
    let game = bwrapi_engine::command::is_game_command(command.kind);
    if command.size as usize != std::mem::size_of::<BwrCommand>()
        || (!game && command.unit_count == 0)
        || (command.unit_count != 0 && command.units.is_null())
    {
        return None;
    }
    let units = if command.unit_count == 0 {
        Vec::new()
    } else {
        std::slice::from_raw_parts(command.units, command.unit_count as usize).to_vec()
    };
    let text = if command.text.is_null() {
        Vec::new()
    } else {
        std::ffi::CStr::from_ptr(command.text).to_bytes().to_vec()
    };
    Some(UnitCommand {
        kind: command.kind,
        queued: command.flags & 1 != 0,
        tile: command.flags & 2 != 0,
        type_id: command.type_id,
        slot: command.slot,
        x: command.x,
        y: command.y,
        target: command.target,
        units,
        text,
    })
}
