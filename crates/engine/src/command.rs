//! Unit commands and the packets the game submits for them. Pure encoding,
//! shared by the in-game executor and the FFI surface so both name commands
//! and build packets the same way.

/// Unit command kinds; the numbers are the public ABI command kind values
/// (BWAPI's unit command order), so the FFI surface and the in-game ring
/// cannot drift.
pub mod command_kind {
    pub const MOVE: u32 = 0;
    pub const SMART_TARGET_UNIT: u32 = 1;
    pub const ATTACK_UNIT: u32 = 2;
    pub const ATTACK_POSITION: u32 = 3;
    pub const SET_RALLY_POSITION: u32 = 4;
    pub const BUILD: u32 = 5;
    pub const BUILD_ADDON: u32 = 6;
    pub const TRAIN: u32 = 7;
    pub const RESEARCH: u32 = 8;
    pub const UPGRADE: u32 = 9;
    pub const LIFT: u32 = 10;
    pub const LAND: u32 = 11;
    pub const STOP: u32 = 12;
    pub const HOLD_POSITION: u32 = 13;
    pub const MORPH: u32 = 14;
    pub const SET_RALLY_UNIT: u32 = 15;
    pub const PATROL: u32 = 16;
    pub const FOLLOW: u32 = 17;
    pub const GATHER: u32 = 18;
    pub const RETURN_CARGO: u32 = 19;
    pub const REPAIR: u32 = 20;
    pub const BURROW: u32 = 21;
    pub const UNBURROW: u32 = 22;
    pub const CLOAK: u32 = 23;
    pub const DECLOAK: u32 = 24;
    pub const SIEGE: u32 = 25;
    pub const UNSIEGE: u32 = 26;
    pub const LOAD: u32 = 27;
    pub const UNLOAD: u32 = 28;
    pub const UNLOAD_ALL: u32 = 29;
    pub const UNLOAD_ALL_POSITION: u32 = 30;
    pub const RIGHT_CLICK_POSITION: u32 = 31;
    pub const HALT_CONSTRUCTION: u32 = 32;
    pub const CANCEL_CONSTRUCTION: u32 = 33;
    pub const CANCEL_ADDON: u32 = 34;
    pub const CANCEL_TRAIN: u32 = 35;
    pub const CANCEL_TRAIN_SLOT: u32 = 36;
    pub const CANCEL_MORPH: u32 = 37;
    pub const CANCEL_RESEARCH: u32 = 38;
    pub const CANCEL_UPGRADE: u32 = 39;
    pub const USE_TECH: u32 = 40;
    pub const USE_TECH_POSITION: u32 = 41;
    pub const USE_TECH_UNIT: u32 = 42;
    pub const PLACE_COP: u32 = 43;
}

/// Commands to the game itself rather than to units: they name no units.
/// The numbers follow the unit kinds in the same public ABI kind space.
pub mod game_command_kind {
    pub const PAUSE: u32 = 100;
    pub const RESUME: u32 = 101;
    pub const RESTART: u32 = 102;
    pub const LEAVE: u32 = 103;
    /// `x`: frame delay in ms for every game speed; negative restores the
    /// game's own delays.
    pub const SET_SPEED: u32 = 104;
    /// `x`: logical frames per screen update, at least 1.
    pub const SET_FRAME_SKIP: u32 = 105;
    /// `text`: a line shown in the game's message area.
    pub const PRINT_TEXT: u32 = 106;
    /// `text`: a chat line from the local player; `type_id` 1 to allies only.
    pub const SEND_TEXT: u32 = 107;
    /// `x`, `y`: map pixels of the screen's top-left corner.
    pub const SET_SCREEN_POSITION: u32 = 108;
    /// `x`: player id; `y`: 0 not allied, 1 allied, 2 allied victory.
    /// Matches only, never the local player.
    pub const SET_ALLIES: u32 = 109;
    /// `x`: player id; `y`: 1 shares the local player's vision with it, 0
    /// stops. In a replay it shows or hides that player's vision instead.
    pub const SET_VISION: u32 = 110;
    /// `x`, `y`: map pixels of the ping.
    pub const PING_MINIMAP: u32 = 111;
    /// `x`: 1 reveals the whole map of a replay, 0 restores its vision.
    pub const SET_REVEAL_ALL: u32 = 112;
    /// `text`: map file the next restart loads.
    pub const SET_MAP: u32 = 113;
    /// `x`: 0 stops the game's screen updates, 1 lets them run again.
    pub const SET_GUI: u32 = 114;
    /// `x`: the whole set of the game's cheat flags to hold; single-player
    /// matches only.
    pub const USE_CHEAT: u32 = 115;
}

/// Whether `kind` is one of [`game_command_kind`].
pub fn is_game_command(kind: u32) -> bool {
    (game_command_kind::PAUSE..=game_command_kind::USE_CHEAT).contains(&kind)
}

/// Longest text a command carries, in UTF-8 bytes; the in-game ring keeps it
/// NUL-terminated where a command keeps its unit ids.
pub const COMMAND_TEXT_MAX: usize = COMMAND_MAX_UNITS * 8 - 1;

/// Longest packet the builders emit.
pub const PACKET_BYTES: usize = 12;
/// The `marker` argument of the target-order call when it carries no unit
/// type: the game's "unit type none".
pub const MARKER_NONE: u16 = 0x00e4;

/// A unit command as the C ABI hands it over. `kind` is a
/// [`command_kind`] value; `x`/`y` are pixels (build tiles for Build);
/// `type_id` and `slot` are read only by the kinds that take them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UnitCommand {
    pub kind: u32,
    pub queued: bool,
    /// Caller's hint that x/y are build tiles.
    pub tile: bool,
    pub type_id: u32,
    pub slot: u32,
    pub x: i32,
    pub y: i32,
    /// Target unit id, 0 for none.
    pub target: u64,
    /// Acting unit ids.
    pub units: Vec<u64>,
    /// Text of the text kinds, UTF-8 without NUL.
    pub text: Vec<u8>,
}

/// CUnit facts of one unit, read from its live record by the executor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitFacts {
    pub unit_type: u16,
    pub x: i32,
    pub y: i32,
    /// The record's status flags ([`STATUS_BURROWED`], [`STATUS_CLOAKED`]).
    pub status: u32,
}

/// CUnit status flag bits the toggle techs read.
pub const STATUS_BURROWED: u32 = 0x10;
pub const STATUS_CLOAKED: u32 = 0x200;

/// Target facts plus the native selection pointer the target-order call takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TargetFacts {
    pub unit_type: u16,
    pub x: i32,
    pub y: i32,
    pub selection_pointer: u64,
}

/// What the executor submits for one group of actors after selecting them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    /// Enqueue these bytes for the current selection.
    Packet { bytes: [u8; PACKET_BYTES], len: usize },
    /// Call the game's target-order builder for the current selection.
    TargetOrder { x: u16, y: u16, target_selection_pointer: u64, order: u8, queued: bool },
    /// Right click on a unit: packet 0x14 with the target's wire id, which the
    /// executor gets from the game's target encoder.
    SmartTarget { x: u16, y: u16, target_selection_pointer: u64, queued: bool },
    /// Unload one passenger: packet 0x29 with its wire id, as above.
    UnloadUnit { target_selection_pointer: u64 },
    /// Select the actors together with the target, then enqueue `opcode`:
    /// the archon merges pair the selected units.
    Merge { partner_selection_pointer: u64, opcode: u8 },
}

/// Shape check of a whole command before any unit record is read: required
/// fields per kind and their numeric ranges. Unit identity, ownership and
/// legality are the executor's business, not this check.
pub fn check(command: &UnitCommand) -> Result<(), &'static str> {
    use command_kind::*;
    if is_game_command(command.kind) {
        return check_game(command);
    }
    if !command.text.is_empty() {
        return Err("kind takes no text");
    }
    if command.units.is_empty() {
        return Err("command has no acting units");
    }
    if single_actor(command.kind) && command.units.len() != 1 {
        return Err("kind takes exactly one acting unit");
    }
    if command.units.len() > COMMAND_MAX_UNITS {
        return Err("command names too many units");
    }
    if !command.units.iter().chain((command.target != 0).then_some(&command.target))
        .all(|id| is_unit_id(*id))
    {
        return Err("command names a unit id that is not an engine unit id");
    }
    match command.kind {
        MOVE | ATTACK_POSITION | SET_RALLY_POSITION | PATROL | UNLOAD_ALL_POSITION
        | RIGHT_CLICK_POSITION => {
            fit_u16(command.x, "x must fit u16")?;
            fit_u16(command.y, "y must fit u16")?;
        }
        USE_TECH_POSITION => {
            fit_u16(command.x, "x must fit u16")?;
            fit_u16(command.y, "y must fit u16")?;
            fit_u8(command.type_id)?;
        }
        SMART_TARGET_UNIT | ATTACK_UNIT | SET_RALLY_UNIT | FOLLOW | GATHER | REPAIR | LOAD | UNLOAD => {
            if command.target == 0 {
                return Err("kind requires a target unit");
            }
        }
        USE_TECH_UNIT => {
            // The merges are the only unit techs that reach the game without
            // a target: the optimizer sends them for a whole selection.
            if command.target == 0 && !matches!(command.type_id, 23 | 28) {
                return Err("kind requires a target unit");
            }
            fit_u8(command.type_id)?;
        }
        BUILD => {
            fit_i16(command.x, "tile x must fit i16")?;
            fit_i16(command.y, "tile y must fit i16")?;
            fit_u16_u32(command.type_id, "type must fit u16")?;
        }
        LAND | PLACE_COP => {
            fit_i16(command.x, "tile x must fit i16")?;
            fit_i16(command.y, "tile y must fit i16")?;
        }
        BUILD_ADDON | TRAIN | MORPH => {
            fit_u16_u32(command.type_id, "type must fit u16")?;
        }
        RESEARCH | UPGRADE | USE_TECH => {
            fit_u8(command.type_id)?;
        }
        CANCEL_TRAIN_SLOT => {
            if command.slot > u32::from(u8::MAX) {
                return Err("slot must fit u8");
            }
        }
        LIFT | STOP | HOLD_POSITION | RETURN_CARGO | BURROW | UNBURROW | CLOAK | DECLOAK | SIEGE
        | UNSIEGE | UNLOAD_ALL | HALT_CONSTRUCTION | CANCEL_CONSTRUCTION | CANCEL_ADDON
        | CANCEL_TRAIN | CANCEL_MORPH | CANCEL_RESEARCH | CANCEL_UPGRADE => {}
        _ => return Err("command kind is not supported"),
    }
    if command.queued && !can_queue(command.kind) {
        return Err("kind cannot queue");
    }
    Ok(())
}

/// Kinds that act through one unit: they place, produce or research
/// something that belongs to that unit alone. Everything else the official
/// BWAPI optimizer groups over a selection.
fn single_actor(kind: u32) -> bool {
    use command_kind::*;
    matches!(kind, BUILD | BUILD_ADDON | RESEARCH | UPGRADE | LAND | PLACE_COP)
}

/// Kinds whose grouped execution names a unit, as the official optimizer's
/// flush does: the first unit of the selection picks the order variant. Every
/// other kind runs for the whole selection with no unit at all.
pub fn reads_front_unit(kind: u32) -> bool {
    use command_kind::*;
    matches!(kind, LOAD | ATTACK_UNIT | TRAIN | UNLOAD_ALL | CANCEL_MORPH | USE_TECH)
}

/// Kinds BWAPI lets the queue flag add to the unit's orders.
fn can_queue(kind: u32) -> bool {
    use command_kind::*;
    matches!(
        kind,
        MOVE | SMART_TARGET_UNIT | ATTACK_UNIT | ATTACK_POSITION | PATROL | FOLLOW | GATHER | REPAIR
            | RETURN_CARGO | LOAD | UNLOAD_ALL | UNLOAD_ALL_POSITION | RIGHT_CLICK_POSITION | STOP
            | HOLD_POSITION
    )
}

fn check_game(command: &UnitCommand) -> Result<(), &'static str> {
    use game_command_kind::*;
    if !command.units.is_empty() || command.target != 0 {
        return Err("game commands name no units");
    }
    let text = matches!(command.kind, PRINT_TEXT | SEND_TEXT | SET_MAP);
    if text != !command.text.is_empty() {
        return Err(if text { "kind requires text" } else { "kind takes no text" });
    }
    if command.text.len() > COMMAND_TEXT_MAX || command.text.contains(&0) {
        return Err("text is too long or contains NUL");
    }
    if command.kind == SEND_TEXT && (std::str::from_utf8(&command.text).is_err() || command.type_id > 1) {
        return Err("chat text must be UTF-8 and type_id 0 or 1");
    }
    if command.kind == SET_FRAME_SKIP && command.x < 1 {
        return Err("frame skip must be at least 1");
    }
    match command.kind {
        SET_ALLIES if !(0..8).contains(&command.x) || !(0..=2).contains(&command.y) => {
            Err("alliance takes a player id 0..8 and a mode 0..=2")
        }
        SET_VISION if !(0..8).contains(&command.x) || !(0..=1).contains(&command.y) => {
            Err("vision takes a player id 0..8 and 0 or 1")
        }
        SET_REVEAL_ALL if !(0..=1).contains(&command.x) => Err("reveal all takes 0 or 1"),
        SET_GUI if !(0..=1).contains(&command.x) => Err("gui takes 0 or 1"),
        _ => Ok(()),
    }
}

/// The command packet a pause, resume or restart submits; `None` for the
/// other game commands, which are not packets.
pub fn game_packet(kind: u32) -> Option<u8> {
    use game_command_kind::*;
    match kind {
        PAUSE => Some(0x10),
        RESUME => Some(0x11),
        RESTART => Some(0x08),
        _ => None,
    }
}

/// The cheat packet: the game's whole cheat flag set.
pub fn cheat_packet(flags: u32) -> [u8; 5] {
    let [a, b, c, d] = flags.to_le_bytes();
    [0x12, a, b, c, d]
}

/// The alliance packet: the local player's whole row of 12 two-bit modes
/// (`alliances`, slot-indexed as the game keeps it) with `slot` set to
/// `mode`. BWAPI ORs an allied mode into the old bits; clearing them first
/// keeps a change between allied and allied victory from reaching mode 3.
pub fn alliance_packet(alliances: u32, slot: usize, mode: u8) -> [u8; 5] {
    let shift = slot * 2;
    let row = (alliances & !(3 << shift)) | u32::from(mode & 3) << shift;
    let [a, b, c, d] = row.to_le_bytes();
    [0x0e, a, b, c, d]
}

/// The shared-vision packet: the local player's 16-bit slot mask with
/// `slot` set or cleared.
pub fn vision_packet(vision: u16, slot: usize, enabled: bool) -> [u8; 3] {
    let bit = 1u16 << slot;
    let [low, high] = if enabled { vision | bit } else { vision & !bit }.to_le_bytes();
    [0x0d, low, high]
}

/// The minimap ping packet. BWAPI narrows the coordinates to the packet's
/// i16 fields without a range check.
pub fn minimap_ping_packet(x: i32, y: i32) -> [u8; 5] {
    let [x0, x1] = (x as i16).to_le_bytes();
    let [y0, y1] = (y as i16).to_le_bytes();
    [0x58, x0, x1, y0, y1]
}

/// Builds the operation for one acting unit from its live facts. Commands
/// already accepted by [`check`] only fail here when a value the record
/// supplies (the unit's type) has no encoding.
pub fn build(
    command: &UnitCommand,
    source: &UnitFacts,
    target: Option<&TargetFacts>,
) -> Result<Operation, &'static str> {
    use command_kind::*;
    let target = || target.ok_or("kind requires a target unit");
    let queued = command.queued;
    match command.kind {
        MOVE => order_position(command.x, command.y, 6, queued),
        ATTACK_POSITION => order_position(
            command.x,
            command.y,
            if source.unit_type == ZERG_INFESTED_TERRAN { 8 } else { 14 },
            queued,
        ),
        SET_RALLY_POSITION => order_position(command.x, command.y, 40, false),
        PATROL => order_position(command.x, command.y, 152, queued),
        UNLOAD_ALL_POSITION => order_position(command.x, command.y, 112, queued),
        UNLOAD_ALL if source.unit_type == TERRAN_BUNKER => short(&[0x28, 0]),
        UNLOAD_ALL => order_position(source.x, source.y, 112, queued),
        RIGHT_CLICK_POSITION => {
            let mut bytes = [0u8; PACKET_BYTES];
            bytes[0] = 0x14;
            bytes[1..3].copy_from_slice(&fit_u16(command.x, "x must fit u16")?.to_le_bytes());
            bytes[3..5].copy_from_slice(&fit_u16(command.y, "y must fit u16")?.to_le_bytes());
            bytes[7..9].copy_from_slice(&MARKER_NONE.to_le_bytes());
            bytes[9] = u8::from(queued);
            packet(bytes, 10)
        }
        USE_TECH_POSITION => order_position(command.x, command.y, tech_order(command.type_id, true)?, false),
        STOP => match source.unit_type {
            81 | 83 => short(&[0x1c]),
            72 | 82 => short(&[0x1b]),
            _ => short(&[0x1a, u8::from(queued)]),
        },
        HALT_CONSTRUCTION => short(&[0x1a, 0]),
        HOLD_POSITION => short(&[0x2b, u8::from(queued)]),
        RETURN_CARGO => short(&[0x1e, u8::from(queued)]),
        BURROW => short(&[0x2c, 0]),
        UNBURROW => short(&[0x2d, 0]),
        CLOAK => short(&[0x21, 0]),
        DECLOAK => short(&[0x22, 0]),
        SIEGE => short(&[0x26, 0]),
        UNSIEGE => short(&[0x25, 0]),
        LIFT => {
            let mut bytes = [0u8; PACKET_BYTES];
            bytes[0] = 0x2f;
            bytes[1..3].copy_from_slice(&fit_i16(source.x, "x must fit i16")?.to_le_bytes());
            bytes[3..5].copy_from_slice(&fit_i16(source.y, "y must fit i16")?.to_le_bytes());
            packet(bytes, 5)
        }
        BUILD => {
            let order = if source.unit_type == ZERG_NYDUS_CANAL && command.type_id == u32::from(ZERG_NYDUS_CANAL) {
                0x2e
            } else if is_addon(command.type_id) {
                0x24
            } else {
                placement_order(command.type_id)?
            };
            placement(order, command.x, command.y, command.type_id)
        }
        // The addon sits at a fixed offset from its producer's centre.
        BUILD_ADDON => placement(
            0x24,
            (source.x - 64).unsigned_abs() as i32 / 32 + 4,
            (source.y - 48).unsigned_abs() as i32 / 32 + 1,
            command.type_id,
        ),
        // A flying building lands as its own type.
        LAND => placement(0x47, command.x, command.y, u32::from(source.unit_type)),
        PLACE_COP => placement(155, command.x, command.y, u32::from(source.unit_type)),
        TRAIN => {
            let type_id = fit_u16_u32(command.type_id, "type must fit u16")?;
            // A larva, egg or lurker egg morphs; a hatchery line morphs in
            // place; a carrier or reaver builds its subordinate directly.
            match source.unit_type {
                72 | 81 | 82 | 83 => short(&[0x27]),
                35 | 38 | 43 => typed_u16(0x23, type_id),
                131 | 132 | 141 | 143 => typed_u16(0x35, type_id),
                _ => typed_u16(0x1f, type_id),
            }
        }
        MORPH => {
            let type_id = fit_u16_u32(command.type_id, "type must fit u16")?;
            known_type(type_id)?;
            typed_u16(if is_building(type_id) { 0x35 } else { 0x23 }, type_id)
        }
        RESEARCH => typed_packet(0x30, command.type_id),
        UPGRADE => typed_packet(0x32, command.type_id),
        CANCEL_CONSTRUCTION => short(&[0x18]),
        CANCEL_ADDON => short(&[0x34]),
        CANCEL_TRAIN => short(&[0x20, 0xfe, 0]),
        CANCEL_TRAIN_SLOT => short(&[0x20, u8::try_from(command.slot).map_err(|_| "slot must fit u8")?, 0]),
        CANCEL_MORPH => {
            known_type(source.unit_type)?;
            short(&[if is_building(source.unit_type) { 0x18 } else { 0x19 }])
        }
        CANCEL_RESEARCH => short(&[0x31]),
        CANCEL_UPGRADE => short(&[0x33]),
        USE_TECH => match command.type_id {
            0 => short(&[0x36]),
            // The tank's mode is its unit type.
            5 => short(&[if matches!(source.unit_type, 25 | 30) { 0x25 } else { 0x26 }, 0]),
            // Personnel cloaking and cloaking field toggle on the unit's
            // cloak; a burrowed unit counts as not cloaked, as in BWAPI.
            9 | 10 => {
                let cloaked = source.status & STATUS_CLOAKED != 0 && source.status & STATUS_BURROWED == 0;
                short(&[if cloaked { 0x22 } else { 0x21 }, 0])
            }
            11 => short(&[if source.status & STATUS_BURROWED != 0 { 0x2d } else { 0x2c }, 0]),
            _ => Err("use_tech without a target requires a no-target tech"),
        },
        // Archon warp and dark archon meld: with a partner each selects the
        // pair, without one (the optimizer's grouped form) the bare opcode
        // goes to the whole selection.
        USE_TECH_UNIT if matches!(command.type_id, 23 | 28) => {
            let opcode = if command.type_id == 23 { 0x2a } else { 0x5a };
            match target() {
                Ok(partner) => Ok(Operation::Merge {
                    partner_selection_pointer: partner.selection_pointer,
                    opcode,
                }),
                Err(_) => short(&[opcode]),
            }
        }
        USE_TECH_UNIT => target_order(target()?, tech_order(command.type_id, false)?, false),
        ATTACK_UNIT => target_order(target()?, attack_order(source.unit_type), queued),
        SET_RALLY_UNIT => target_order(target()?, 39, false),
        FOLLOW => target_order(target()?, 49, queued),
        GATHER => target_order(target()?, 79, queued),
        REPAIR => target_order(target()?, 34, queued),
        LOAD => {
            let order = match source.unit_type {
                TERRAN_BUNKER => 95,
                unit_type if provides_cargo(unit_type) => 94,
                // A passenger entering its carrier is the game's smart click.
                _ => return smart_target(target()?, queued),
            };
            target_order(target()?, order, queued)
        }
        SMART_TARGET_UNIT => smart_target(target()?, queued),
        UNLOAD => Ok(Operation::UnloadUnit { target_selection_pointer: target()?.selection_pointer }),
        _ => Err("command kind is not supported"),
    }
}

fn smart_target(target: &TargetFacts, queued: bool) -> Result<Operation, &'static str> {
    let x = u16::try_from(target.x).map_err(|_| "target x must fit u16")?;
    let y = u16::try_from(target.y).map_err(|_| "target y must fit u16")?;
    Ok(Operation::SmartTarget { x, y, target_selection_pointer: target.selection_pointer, queued })
}

/// Highest number of acting units one command may name: two full selections.
pub const COMMAND_MAX_UNITS: usize = 24;

const ZERG_INFESTED_TERRAN: u16 = 50;
const ZERG_NYDUS_CANAL: u16 = 134;
const TERRAN_BUNKER: u16 = 125;

/// Engine unit id shape: `(slot + 1) | uniqueness << 16`, never 0.
fn is_unit_id(id: u64) -> bool {
    id != 0 && id >> 24 == 0 && id as u16 != 0
}

fn fit_u16(value: i32, name: &'static str) -> Result<u16, &'static str> {
    u16::try_from(value).map_err(|_| name)
}

fn fit_u16_u32(value: u32, name: &'static str) -> Result<u16, &'static str> {
    u16::try_from(value).map_err(|_| name)
}

fn fit_u8(value: u32) -> Result<u8, &'static str> {
    u8::try_from(value).map_err(|_| "type must fit u8")
}

fn fit_i16(value: i32, name: &'static str) -> Result<i16, &'static str> {
    i16::try_from(value).map_err(|_| name)
}

/// Order issued to the current selection for a bare position, as the game's
/// own right-click wire format: position, no target, unit-type marker.
fn order_position(x: i32, y: i32, order: u8, queued: bool) -> Result<Operation, &'static str> {
    let mut bytes = [0u8; PACKET_BYTES];
    bytes[0] = 0x15;
    bytes[1..3].copy_from_slice(&fit_u16(x, "x must fit u16")?.to_le_bytes());
    bytes[3..5].copy_from_slice(&fit_u16(y, "y must fit u16")?.to_le_bytes());
    bytes[7..9].copy_from_slice(&MARKER_NONE.to_le_bytes());
    bytes[9] = order;
    bytes[10] = u8::from(queued);
    packet(bytes, 11)
}

/// A packet that is just its bytes.
fn short(bytes: &[u8]) -> Result<Operation, &'static str> {
    let mut packet_bytes = [0u8; PACKET_BYTES];
    packet_bytes[..bytes.len()].copy_from_slice(bytes);
    packet(packet_bytes, bytes.len())
}

/// `[opcode, type lo, type hi]` packets of the morph and train buttons.
fn typed_u16(opcode: u8, type_id: u16) -> Result<Operation, &'static str> {
    let [low, high] = type_id.to_le_bytes();
    short(&[opcode, low, high])
}

/// The building placement packet: order, tile and the type placed.
fn placement(order: u8, tile_x: i32, tile_y: i32, type_id: u32) -> Result<Operation, &'static str> {
    let mut bytes = [0u8; PACKET_BYTES];
    bytes[0] = 0x0c;
    bytes[1] = order;
    bytes[2..4].copy_from_slice(&fit_i16(tile_x, "tile x must fit i16")?.to_le_bytes());
    bytes[4..6].copy_from_slice(&fit_i16(tile_y, "tile y must fit i16")?.to_le_bytes());
    bytes[6..8].copy_from_slice(&fit_u16_u32(type_id, "type must fit u16")?.to_le_bytes());
    packet(bytes, 8)
}

fn packet(bytes: [u8; PACKET_BYTES], len: usize) -> Result<Operation, &'static str> {
    Ok(Operation::Packet { bytes, len })
}

/// `[opcode, type]` packets of the research and upgrade buttons.
fn typed_packet(opcode: u8, type_id: u32) -> Result<Operation, &'static str> {
    let mut bytes = [0u8; PACKET_BYTES];
    bytes[0] = opcode;
    bytes[1] = u8::try_from(type_id).map_err(|_| "type must fit u8")?;
    packet(bytes, 2)
}

fn target_order(target: &TargetFacts, order: u8, queued: bool) -> Result<Operation, &'static str> {
    let x = u16::try_from(target.x).map_err(|_| "target x must fit u16")?;
    let y = u16::try_from(target.y).map_err(|_| "target y must fit u16")?;
    Ok(Operation::TargetOrder { x, y, target_selection_pointer: target.selection_pointer, order, queued })
}

/// Order id an attack on a unit takes, from the attacker's type.
fn attack_order(unit_type: u16) -> u8 {
    match unit_type {
        72 | 82 => 53,
        81 | 83 => 59,
        _ if is_building(unit_type) => 19,
        _ => 8,
    }
}

/// Placement order id of a worker-built building, by producing race.
fn placement_order(unit_type: u32) -> Result<u8, &'static str> {
    match unit_type {
        106..=127 => Ok(30),
        130..=153 => Ok(25),
        154..=179 => Ok(31),
        _ => Err("build requires a building type"),
    }
}

/// Order of an ability that targets a unit or a position, by tech type;
/// `position` selects the position-targeted shape of techs that have both.
fn tech_order(tech_type: u32, position: bool) -> Result<u8, &'static str> {
    Ok(match (tech_type, position) {
        (2, _) => 122,
        (3, true) => 132,
        (4, _) => 139,
        (14, _) => 119,
        (15, _) => 144,
        (17, _) => 146,
        (19, _) => 142,
        (20, _) => 148,
        (21, _) => 137,
        (22, _) => 147,
        (25, _) => 181,
        (31, _) => 186,
        (34, true) => 177,
        (45, _) => 126,
        (1, false) => 115,
        (6, false) => 141,
        (7, false) => 143,
        (8, false) => 113,
        (12, false) => 27,
        (13, false) => 121,
        (16, false) => 145,
        (18, false) => 120,
        (24, false) => 180,
        (27, false) => 182,
        (29, false) => 184,
        (30, false) => 185,
        (34, false) => 176,
        _ => return Err("tech type does not have the requested target shape"),
    })
}

/// Whether a unit type carries passengers.
fn provides_cargo(unit_type: u16) -> bool {
    bwrapi_internal_model::unit_type_metadata::cargo_space_provided(unit_type).is_some_and(|space| space > 0)
}

fn known_type(unit_type: u16) -> Result<(), &'static str> {
    if usize::from(unit_type) < 234 { Ok(()) } else { Err("unit type is outside the known types") }
}

/// Terran addon types are placed through the addon order, not the placement
/// order of their race.
fn is_addon(unit_type: u32) -> bool {
    matches!(unit_type, 107 | 108 | 115 | 117 | 118 | 120)
}

// BWAPI 4.1.2 UnitPrototypeFlags: which unit types are buildings, one bit
// per type id.
const IS_BUILDING_BY_TYPE: [u64; 4] = [
    0x0000_0000_0000_0000,
    0xffff_fc00_0000_0000,
    0xffff_ffff_ffff_fffc,
    0x0000_0000_0040_03ff,
];

fn is_building(unit_type: u16) -> bool {
    let index = usize::from(unit_type);
    index < 234 && IS_BUILDING_BY_TYPE[index / 64] & (1u64 << (index % 64)) != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(unit_type: u16) -> UnitFacts {
        UnitFacts { unit_type, x: 100, y: 200, status: 0 }
    }

    fn target() -> TargetFacts {
        TargetFacts { unit_type: 0, x: 300, y: 400, selection_pointer: 0x5000_0010 }
    }

    fn packet_of(operation: Operation) -> ([u8; PACKET_BYTES], usize) {
        match operation {
            Operation::Packet { bytes, len } => (bytes, len),
            other => panic!("expected a packet, got {other:?}"),
        }
    }

    fn bytes_of(kind: u32, source: u16, set: impl FnOnce(&mut UnitCommand)) -> Vec<u8> {
        let mut command = UnitCommand { kind, units: vec![1], target: 2, ..UnitCommand::default() };
        set(&mut command);
        assert_eq!(check(&command), Ok(()), "kind {kind}");
        let (bytes, len) = packet_of(build(&command, &facts(source), Some(&target())).unwrap());
        bytes[..len].to_vec()
    }

    fn order_of(kind: u32, source: u16, set: impl FnOnce(&mut UnitCommand)) -> (u8, bool) {
        let mut command = UnitCommand { kind, units: vec![1], target: 2, ..UnitCommand::default() };
        set(&mut command);
        assert_eq!(check(&command), Ok(()), "kind {kind}");
        match build(&command, &facts(source), Some(&target())).unwrap() {
            Operation::TargetOrder { order, queued, .. } => (order, queued),
            other => panic!("expected a target order, got {other:?}"),
        }
    }

    #[test]
    fn simple_unit_commands_encode_their_packets() {
        use command_kind::*;
        let none = |_: &mut UnitCommand| {};
        for (kind, expected) in [
            (HOLD_POSITION, vec![0x2b, 0]),
            (RETURN_CARGO, vec![0x1e, 0]),
            (BURROW, vec![0x2c, 0]),
            (UNBURROW, vec![0x2d, 0]),
            (CLOAK, vec![0x21, 0]),
            (DECLOAK, vec![0x22, 0]),
            (SIEGE, vec![0x26, 0]),
            (UNSIEGE, vec![0x25, 0]),
            (HALT_CONSTRUCTION, vec![0x1a, 0]),
            (CANCEL_CONSTRUCTION, vec![0x18]),
            (CANCEL_ADDON, vec![0x34]),
            (CANCEL_TRAIN, vec![0x20, 0xfe, 0]),
            (CANCEL_RESEARCH, vec![0x31]),
            (CANCEL_UPGRADE, vec![0x33]),
        ] {
            assert_eq!(bytes_of(kind, 7, none), expected, "kind {kind}");
        }
        assert_eq!(bytes_of(HOLD_POSITION, 7, |c| c.queued = true), vec![0x2b, 1]);
        assert_eq!(bytes_of(CANCEL_TRAIN_SLOT, 106, |c| c.slot = 3), vec![0x20, 3, 0]);
        assert_eq!(bytes_of(LIFT, 106, none), vec![0x2f, 100, 0, 200, 0]);
        assert_eq!(bytes_of(UNLOAD_ALL, TERRAN_BUNKER, none), vec![0x28, 0]);
        assert_eq!(bytes_of(CANCEL_MORPH, 106, none), vec![0x18]);
        assert_eq!(bytes_of(CANCEL_MORPH, 37, none), vec![0x19]);
        assert_eq!(bytes_of(MORPH, 37, |c| c.type_id = 38), vec![0x23, 38, 0]);
        assert_eq!(bytes_of(MORPH, 37, |c| c.type_id = 131), vec![0x35, 131, 0]);
        assert_eq!(bytes_of(USE_TECH, 7, |c| c.type_id = 0), vec![0x36]);
        assert_eq!(bytes_of(USE_TECH, 5, |c| c.type_id = 5), vec![0x26, 0]);
        assert_eq!(bytes_of(USE_TECH, 30, |c| c.type_id = 5), vec![0x25, 0]);
    }

    #[test]
    fn placement_commands_use_their_orders_and_types() {
        use command_kind::*;
        let tile = |c: &mut UnitCommand| {
            c.x = 40;
            c.y = 30;
        };
        assert_eq!(bytes_of(LAND, 106, tile), vec![0x0c, 0x47, 40, 0, 30, 0, 106, 0]);
        assert_eq!(bytes_of(PLACE_COP, 106, tile), vec![0x0c, 155, 40, 0, 30, 0, 106, 0]);
        // An addon's tile follows its producer at (100, 200): |100 - 64| / 32 + 4, |200 - 48| / 32 + 1.
        assert_eq!(bytes_of(BUILD_ADDON, 106, |c| c.type_id = 107), vec![0x0c, 0x24, 5, 0, 5, 0, 107, 0]);
    }

    #[test]
    fn position_orders_carry_their_order_ids() {
        use command_kind::*;
        let at = |c: &mut UnitCommand| {
            c.x = 50;
            c.y = 60;
        };
        let order = |kind, source, set: &dyn Fn(&mut UnitCommand)| {
            let bytes = bytes_of(kind, source, |c| {
                at(c);
                set(c)
            });
            assert_eq!(bytes[0], 0x15);
            (bytes[9], bytes[10])
        };
        assert_eq!(order(SET_RALLY_POSITION, 106, &|_| {}), (40, 0));
        assert_eq!(order(PATROL, 0, &|c| c.queued = true), (152, 1));
        assert_eq!(order(UNLOAD_ALL_POSITION, 11, &|_| {}), (112, 0));
        assert_eq!(order(USE_TECH_POSITION, 1, &|c| c.type_id = 3), (132, 0));
        // Unloading everything from a carrier unloads where it hovers: (100, 200).
        let bytes = bytes_of(UNLOAD_ALL, 11, |_| {});
        assert_eq!((&bytes[1..5], bytes[9]), (&[100u8, 0, 200, 0][..], 112));
        let click = bytes_of(RIGHT_CLICK_POSITION, 0, |c| {
            at(c);
            c.queued = true;
        });
        assert_eq!(click, vec![0x14, 50, 0, 60, 0, 0, 0, 0xe4, 0, 1]);
    }

    #[test]
    fn target_orders_carry_their_order_ids() {
        use command_kind::*;
        let none = |_: &mut UnitCommand| {};
        assert_eq!(order_of(SET_RALLY_UNIT, 106, none), (39, false));
        assert_eq!(order_of(FOLLOW, 0, |c| c.queued = true), (49, true));
        assert_eq!(order_of(REPAIR, 7, none), (34, false));
        assert_eq!(order_of(LOAD, TERRAN_BUNKER, none), (95, false));
        assert_eq!(order_of(LOAD, 11, |c| c.queued = true), (94, true));
        assert_eq!(order_of(USE_TECH_UNIT, 1, |c| c.type_id = 1), (115, false));
        assert_eq!(order_of(USE_TECH_UNIT, 1, |c| c.type_id = 2), (122, false));
    }

    #[test]
    fn unit_targets_the_game_names_by_wire_id_go_to_the_executor() {
        use command_kind::*;
        let one = |kind| UnitCommand { kind, units: vec![1], target: 2, queued: true, ..UnitCommand::default() };
        assert_eq!(
            build(&one(SMART_TARGET_UNIT), &facts(0), Some(&target())),
            Ok(Operation::SmartTarget { x: 300, y: 400, target_selection_pointer: 0x5000_0010, queued: true })
        );
        // A marine boarding is the passenger's smart click; a bunker loads by order.
        assert!(matches!(build(&one(LOAD), &facts(0), Some(&target())), Ok(Operation::SmartTarget { .. })));
        assert!(matches!(build(&one(LOAD), &facts(125), Some(&target())), Ok(Operation::TargetOrder { order: 95, .. })));
        let unload = UnitCommand { queued: false, ..one(UNLOAD) };
        assert_eq!(
            build(&unload, &facts(125), Some(&target())),
            Ok(Operation::UnloadUnit { target_selection_pointer: 0x5000_0010 })
        );
    }

    #[test]
    fn shape_checks_cover_the_new_kinds() {
        use command_kind::*;
        let one = |kind| UnitCommand { kind, units: vec![1], target: 2, ..UnitCommand::default() };
        assert_eq!(check(&UnitCommand { queued: true, ..one(SIEGE) }), Err("kind cannot queue"));
        assert_eq!(check(&UnitCommand { units: vec![1, 3], ..one(BUILD) }), Err("kind takes exactly one acting unit"));
        assert_eq!(check(&UnitCommand { target: 0, ..one(REPAIR) }), Err("kind requires a target unit"));
        assert_eq!(check(&UnitCommand { slot: 256, ..one(CANCEL_TRAIN_SLOT) }), Err("slot must fit u8"));
        assert_eq!(check(&UnitCommand { x: -1, ..one(PATROL) }), Err("x must fit u16"));
        assert_eq!(check(&UnitCommand { x: -1, y: 5, ..one(PLACE_COP) }), Ok(()));
        // The kinds the official optimizer groups name several units.
        for kind in [TRAIN, MORPH, LIFT, UNLOAD] {
            assert_eq!(check(&UnitCommand { units: vec![1, 3, 4], ..one(kind) }), Ok(()), "kind {kind}");
        }
        assert!(reads_front_unit(LOAD));
        assert!(reads_front_unit(ATTACK_UNIT));
        assert!(reads_front_unit(TRAIN));
        assert!(reads_front_unit(UNLOAD_ALL));
        assert!(reads_front_unit(CANCEL_MORPH));
        assert!(reads_front_unit(USE_TECH));
        assert!(!reads_front_unit(STOP));
        assert!(!reads_front_unit(ATTACK_POSITION));
        assert!(!reads_front_unit(USE_TECH_UNIT));
    }

    #[test]
    fn toggle_techs_follow_the_status_flags() {
        use command_kind::USE_TECH;
        let toggle = |type_id, status| {
            let command = UnitCommand { kind: USE_TECH, type_id, units: vec![1], ..UnitCommand::default() };
            let facts = UnitFacts { status, ..facts(0) };
            let (bytes, len) = packet_of(build(&command, &facts, None).unwrap());
            bytes[..len].to_vec()
        };
        for tech in [9, 10] {
            assert_eq!(toggle(tech, 0), vec![0x21, 0]);
            assert_eq!(toggle(tech, STATUS_CLOAKED), vec![0x22, 0]);
            // A burrowed unit is not cloaked, so it cloaks.
            assert_eq!(toggle(tech, STATUS_CLOAKED | STATUS_BURROWED), vec![0x21, 0]);
        }
        assert_eq!(toggle(11, 0), vec![0x2c, 0]);
        assert_eq!(toggle(11, STATUS_BURROWED), vec![0x2d, 0]);
    }

    #[test]
    fn merges_select_the_partner_and_send_the_merge_opcode() {
        use command_kind::USE_TECH_UNIT;
        for (tech, opcode) in [(23, 0x2a), (28, 0x5a)] {
            let command = UnitCommand { kind: USE_TECH_UNIT, type_id: tech, units: vec![1], target: 2, ..UnitCommand::default() };
            assert_eq!(check(&command), Ok(()));
            assert_eq!(
                build(&command, &facts(67), Some(&target())),
                Ok(Operation::Merge { partner_selection_pointer: 0x5000_0010, opcode })
            );
            // Without a target the opcode alone reaches the selection.
            let bare = UnitCommand { target: 0, ..command };
            assert_eq!(check(&bare), Ok(()));
            assert_eq!(build(&bare, &facts(67), None), short(&[opcode]));
            // Any other unit tech still requires its target.
            let other = UnitCommand { type_id: 1, target: 0, ..bare };
            assert_eq!(check(&other), Err("kind requires a target unit"));
        }
    }

    #[test]
    fn player_game_commands_check_their_arguments_and_encode_bwapi_packets() {
        use game_command_kind::*;
        let game = |kind, x, y| UnitCommand { kind, x, y, ..UnitCommand::default() };
        assert_eq!(check(&game(SET_ALLIES, 3, 2)), Ok(()));
        assert!(check(&game(SET_ALLIES, 8, 1)).is_err());
        assert!(check(&game(SET_ALLIES, 3, 3)).is_err());
        assert_eq!(check(&game(SET_VISION, 0, 1)), Ok(()));
        assert!(check(&game(SET_VISION, -1, 1)).is_err());
        assert!(check(&game(SET_VISION, 1, 2)).is_err());
        assert_eq!(check(&game(SET_REVEAL_ALL, 1, 0)), Ok(()));
        assert!(check(&game(SET_REVEAL_ALL, 2, 0)).is_err());
        assert_eq!(check(&game(PING_MINIMAP, -5, 70_000)), Ok(()));
        assert!(check(&game(SET_MAP, 0, 0)).is_err());
        assert_eq!(check(&UnitCommand { text: b"maps\\x.scm".to_vec(), ..game(SET_MAP, 0, 0) }), Ok(()));

        // Slot 0 allied, slot 2 allied victory; slot 2 drops, slot 1 becomes allied victory.
        let row = 1 | 2 << 4;
        assert_eq!(alliance_packet(row, 2, 0), [0x0e, 1, 0, 0, 0]);
        assert_eq!(alliance_packet(row, 1, 2), [0x0e, 0x29, 0, 0, 0]);
        assert_eq!(alliance_packet(row, 2, 1), [0x0e, 0x11, 0, 0, 0]);
        assert_eq!(alliance_packet(0, 11, 1), [0x0e, 0, 0, 0x40, 0]);
        assert_eq!(vision_packet(0x0001, 3, true), [0x0d, 0x09, 0]);
        assert_eq!(vision_packet(0x0109, 3, false), [0x0d, 0x01, 0x01]);
        assert_eq!(minimap_ping_packet(320, -8), [0x58, 0x40, 0x01, 0xf8, 0xff]);
    }

    #[test]
    fn game_commands_name_no_units_and_text_kinds_need_text() {
        use game_command_kind::*;
        let game = |kind| UnitCommand { kind, ..UnitCommand::default() };
        assert_eq!(check(&game(PAUSE)), Ok(()));
        assert_eq!(game_packet(PAUSE), Some(0x10));
        assert_eq!(game_packet(LEAVE), None);
        assert!(check(&UnitCommand { units: vec![1], ..game(RESUME) }).is_err());
        assert!(check(&game(PRINT_TEXT)).is_err());
        assert_eq!(check(&UnitCommand { text: b"gg".to_vec(), ..game(SEND_TEXT) }), Ok(()));
        assert!(check(&UnitCommand { text: vec![0xff], ..game(SEND_TEXT) }).is_err());
        assert!(check(&UnitCommand { text: b"x".to_vec(), ..game(PAUSE) }).is_err());
        assert!(check(&game(SET_FRAME_SKIP)).is_err());
        assert_eq!(check(&UnitCommand { x: -1, ..game(SET_SPEED) }), Ok(()));
    }

    #[test]
    fn move_and_attack_position_encode_the_order_wire_format() {
        let move_command = UnitCommand { kind: command_kind::MOVE, x: 640, y: 96, queued: true, units: vec![1], ..UnitCommand::default() };
        let mut expected = [0u8; PACKET_BYTES];
        expected[0] = 0x15;
        expected[1..3].copy_from_slice(&640u16.to_le_bytes());
        expected[3..5].copy_from_slice(&96u16.to_le_bytes());
        expected[7..9].copy_from_slice(&MARKER_NONE.to_le_bytes());
        expected[9] = 6;
        expected[10] = 1;
        assert_eq!(packet_of(build(&move_command, &facts(7), None).unwrap()), (expected, 11));

        let attack = UnitCommand { kind: command_kind::ATTACK_POSITION, x: 100, y: 100, units: vec![1], ..UnitCommand::default() };
        let (bytes, _) = packet_of(build(&attack, &facts(0), None).unwrap());
        assert_eq!(bytes[9], 14);
        let (bytes, _) = packet_of(build(&attack, &facts(ZERG_INFESTED_TERRAN), None).unwrap());
        assert_eq!(bytes[9], 8);
    }

    #[test]
    fn attack_unit_and_gather_become_target_orders() {
        let mut attack = UnitCommand { kind: command_kind::ATTACK_UNIT, target: 2, units: vec![1], ..UnitCommand::default() };
        match build(&attack, &facts(0), Some(&target())).unwrap() {
            Operation::TargetOrder { x, y, target_selection_pointer, order, queued } => {
                assert_eq!((x, y, target_selection_pointer, order, queued), (300, 400, 0x5000_0010, 8, false));
            }
            _ => panic!("expected a target order"),
        }
        for (source, order) in [(7u16, 8u8), (72, 53), (81, 59), (106, 19)] {
            match build(&attack, &facts(source), Some(&target())) {
                Ok(Operation::TargetOrder { order: got, .. }) => assert_eq!(got, order),
                _ => panic!("expected a target order"),
            }
        }
        attack.kind = command_kind::GATHER;
        attack.queued = true;
        match build(&attack, &facts(41), Some(&target())).unwrap() {
            Operation::TargetOrder { order, queued, .. } => assert_eq!((order, queued), (79, true)),
            _ => panic!("expected a target order"),
        }
    }

    #[test]
    fn train_picks_its_opcode_from_the_source_type() {
        let mut train = UnitCommand { kind: command_kind::TRAIN, type_id: 7, units: vec![1], ..UnitCommand::default() };
        for (source, opcode) in [(35u16, 0x23u8), (131, 0x35), (72, 0x27), (106, 0x1f)] {
            let (bytes, len) = packet_of(build(&train, &facts(source), None).unwrap());
            assert_eq!(bytes[0], opcode);
            // A carrier or reaver trains with the bare opcode.
            assert_eq!((len, &bytes[1..3]), if opcode == 0x27 { (1, &[0u8, 0][..]) } else { (3, &[7u8, 0][..]) });
        }
        train.type_id = 0x102;
        let (bytes, _) = packet_of(build(&train, &facts(106), None).unwrap());
        assert_eq!(&bytes[1..3], &[0x02, 0x01]);
    }

    #[test]
    fn build_picks_the_placement_order_of_the_type() {
        let mut build_command = UnitCommand { kind: command_kind::BUILD, type_id: 109, x: 40, y: 30, units: vec![1], ..UnitCommand::default() };
        let (bytes, _) = packet_of(build(&build_command, &facts(7), None).unwrap());
        assert_eq!(&bytes[..8], &[0x0c, 30, 40, 0, 30, 0, 109, 0]);

        build_command.type_id = 156;
        let (bytes, _) = packet_of(build(&build_command, &facts(64), None).unwrap());
        assert_eq!(bytes[1], 31);

        build_command.type_id = 107;
        let (bytes, _) = packet_of(build(&build_command, &facts(7), None).unwrap());
        assert_eq!(bytes[1], 0x24);

        build_command.type_id = 134;
        let (bytes, _) = packet_of(build(&build_command, &facts(134), None).unwrap());
        assert_eq!(bytes[1], 0x2e);

        build_command.type_id = 0;
        assert_eq!(build(&build_command, &facts(7), None), Err("build requires a building type"));
    }

    #[test]
    fn research_upgrade_and_stop_encode_fixed_packets() {
        let mut command = UnitCommand { kind: command_kind::RESEARCH, type_id: 2, units: vec![1], ..UnitCommand::default() };
        assert_eq!(&packet_of(build(&command, &facts(112), None).unwrap()).0[..2], &[0x30, 2]);
        command.kind = command_kind::UPGRADE;
        command.type_id = 16;
        assert_eq!(&packet_of(build(&command, &facts(122), None).unwrap()).0[..2], &[0x32, 16]);
        command.kind = command_kind::STOP;
        assert_eq!(&packet_of(build(&command, &facts(7), None).unwrap()).0[..2], &[0x1a, 0]);
        assert_eq!(&packet_of(build(&command, &facts(81), None).unwrap()).0[..1], &[0x1c]);
        command.queued = true;
        assert_eq!(&packet_of(build(&command, &facts(7), None).unwrap()).0[..2], &[0x1a, 1]);
    }

    #[test]
    fn check_rejects_shape_errors_and_kinds_outside_the_supported_set() {
        let base = UnitCommand { kind: command_kind::MOVE, x: 5, y: 5, units: vec![1], ..UnitCommand::default() };
        assert!(check(&base).is_ok());
        assert_eq!(check(&UnitCommand { units: vec![], ..base.clone() }), Err("command has no acting units"));
        assert_eq!(check(&UnitCommand { x: -1, ..base.clone() }), Err("x must fit u16"));
        assert_eq!(check(&UnitCommand { units: vec![0], ..base.clone() }), Err("command names a unit id that is not an engine unit id"));
        assert_eq!(check(&UnitCommand { units: vec![1 << 24], ..base.clone() }), Err("command names a unit id that is not an engine unit id"));
        assert_eq!(check(&UnitCommand { kind: 99, ..base.clone() }), Err("command kind is not supported"));
        let train = UnitCommand { kind: command_kind::TRAIN, type_id: 7, units: vec![1], ..UnitCommand::default() };
        assert_eq!(check(&UnitCommand { queued: true, ..train.clone() }), Err("kind cannot queue"));
        assert_eq!(check(&UnitCommand { type_id: 256, kind: command_kind::RESEARCH, units: vec![1], ..UnitCommand::default() }), Err("type must fit u8"));
        let gather = UnitCommand { kind: command_kind::GATHER, units: vec![1], ..UnitCommand::default() };
        assert_eq!(check(&gather), Err("kind requires a target unit"));
        assert!(check(&UnitCommand { target: 2, ..gather }).is_ok());
        assert_eq!(
            check(&UnitCommand { units: (1..=25u64).collect::<Vec<_>>(), kind: command_kind::STOP, ..UnitCommand::default() }),
            Err("command names too many units")
        );
    }

    #[test]
    fn build_fails_without_a_target_when_the_kind_needs_one() {
        let attack = UnitCommand { kind: command_kind::ATTACK_UNIT, target: 2, units: vec![1], ..UnitCommand::default() };
        assert_eq!(build(&attack, &facts(0), None), Err("kind requires a target unit"));
    }
}
