//! Session, map and per-slot BWGame tables: game, forces, start locations
//! and unit availability.

use super::players::{read_identities, slot_ids};
use super::u32_at;
use crate::columns::{Cell, Text, Text256, Text32};
use crate::decode::bwgame::{
    canonical_start_tile, BWGAME_COUNTDOWN_SECONDS_OFFSET, BWGAME_ELAPSED_TIME_OFFSET, BWGAME_FORCE_NAMES_OFFSET,
    BWGAME_FORCE_NAME_BYTES, BWGAME_MAP_FILE_NAME_BYTES, BWGAME_MAP_FILE_NAME_OFFSET, BWGAME_MAP_TITLE_BYTES,
    BWGAME_MAP_TITLE_OFFSET, BWGAME_PLAYABLE_PLAYER_COUNT, BWGAME_PLAYER_UNIT_AVAILABILITY_OFFSET,
    BWGAME_START_POSITIONS_OFFSET, BWGAME_UNIT_AVAILABILITY_TYPE_COUNT,
};
use crate::decode::map::{dimensions, BWGAME_MAP_DIMENSIONS_BYTES, BWGAME_MAP_WIDTH_OFFSET};
use crate::decode::scalars::{
    read_session_runtime_inputs, GAME_SPEED_PRIMARY_DELAYS_BYTES, SESSION_NETWORK_IN_MATCH_BYTES,
    SESSION_PAUSED_BYTES,
};
use crate::engine::{State, Target};
use crate::tables::*;

/// Game type u16 inside the replay header (x64 only: the header RVA is
/// unresolved on x86, which keeps the type unknown).
const GAME_TYPE_HEADER_OFFSET: usize = 0x14 + 0x28;
const FORCES: usize = 4;
const START_POSITION_BYTES: usize = BWGAME_PLAYABLE_PLAYER_COUNT * 4;
/// The BWGame span holding map names, force names, countdown and elapsed time.
const GAME_SPAN_START: usize = BWGAME_MAP_FILE_NAME_OFFSET;
const GAME_SPAN_END: usize = BWGAME_ELAPSED_TIME_OFFSET + 4;

/// Trimmed UTF-8 text of a NUL-terminated field; empty or invalid is unknown.
fn text<const N: usize>(raw: &[u8]) -> Text<N> {
    let end = raw.iter().position(|byte| *byte == 0).unwrap_or(raw.len());
    match std::str::from_utf8(&raw[..end]).map(str::trim) {
        Ok(value) if !value.is_empty() => Text::from_bytes(value.as_bytes()),
        _ => Text::default(),
    }
}

fn file_name(path: &[u8]) -> Text256 {
    let end = path.iter().position(|byte| *byte == 0).unwrap_or(path.len());
    let path = &path[..end];
    let start = path.iter().rposition(|byte| matches!(byte, b'\\' | b'/')).map_or(0, |at| at + 1);
    text(&path[start..])
}

/// Build-tile start location of each BWGame slot. A slot whose position is
/// unused, off the tile grid, outside the map, unverifiable without the map
/// size, or a repeat of an earlier slot is dropped alone.
pub(super) fn start_tiles(map_size: &[u8], positions: &[u8]) -> [Option<(i16, i16)>; BWGAME_PLAYABLE_PLAYER_COUNT] {
    let dimensions = dimensions(map_size);
    let mut tiles = [None; BWGAME_PLAYABLE_PLAYER_COUNT];
    for (slot, position) in positions.chunks_exact(4).take(BWGAME_PLAYABLE_PLAYER_COUNT).enumerate() {
        let x = i16::from_le_bytes([position[0], position[1]]);
        let y = i16::from_le_bytes([position[2], position[3]]);
        let tile = canonical_start_tile(x, y, dimensions)
            .and_then(|(tile_x, tile_y)| Some((i16::try_from(tile_x).ok()?, i16::try_from(tile_y).ok()?)))
            .filter(|tile| !tiles[..slot].contains(&Some(*tile)));
        tiles[slot] = tile;
    }
    tiles
}

fn module_byte(target: &Target, rva: usize, bytes: usize) -> Option<u8> {
    let mut raw = [0; 1];
    debug_assert_eq!(bytes, 1);
    target.read_module(rva, &mut raw).ok()?;
    Some(raw[0])
}

/// Network provider ids of Battle.net games: `BNET`, and `BNAU` for its
/// matchmaking, which the game treats as Battle.net alike.
fn is_battle_net(provider: u32) -> bool {
    provider == u32::from_be_bytes(*b"BNET") || provider == u32::from_be_bytes(*b"BNAU")
}

/// Milliseconds per frame at the current game speed.
fn game_speed_delay_ms(target: &Target) -> Option<u32> {
    let scalars = &target.layout.scalars;
    let pointer = target.module_pointer(scalars.game_speed_pointer_offset).filter(|pointer| *pointer != 0)?;
    // The speed index follows the speed object's vtable pointer.
    let index = target.u32_at(pointer.checked_add(target.layout.pointer_bytes)?)? as usize;
    if index >= GAME_SPEED_PRIMARY_DELAYS_BYTES / 4 {
        return None;
    }
    let mut delays = [0; GAME_SPEED_PRIMARY_DELAYS_BYTES];
    target.read_module(scalars.game_speed_primary_delays_offset, &mut delays).ok()?;
    Some(u32_at(&delays, index * 4))
}

/// The running game: session flags, timing, seed and map names.
pub(super) fn game(target: &Target, state: &mut State, table: &mut Game) {
    let scalars = &target.layout.scalars;
    let mut row = GameRow::UNKNOWN;

    let mut header = [0; 2];
    if let Some(at) = target.layout.replay.header.checked_add(GAME_TYPE_HEADER_OFFSET) {
        if target.read_module(at, &mut header).is_ok() {
            if let Some(game_type) = u8::try_from(u16::from_le_bytes(header)).ok().filter(|value| *value != 0) {
                row.game_type = game_type;
            }
        }
    }
    if let Some(slots) = super::pool::capacity(target).and_then(|slots| u16::try_from(slots).ok()) {
        row.unit_slots = slots;
    }

    let paused = module_byte(target, scalars.session_paused_offset, SESSION_PAUSED_BYTES);
    let multiplayer = module_byte(target, scalars.session_network_in_match_offset, SESSION_NETWORK_IN_MATCH_BYTES);
    if let Some(facts) = read_session_runtime_inputs(
        paused,
        multiplayer,
        target.module_u32(scalars.session_latency_raw_offset),
        target.module_u32(scalars.session_builtin_turn_latency_offset),
        target.module_u32(scalars.session_sync_active_offset),
    ) {
        if let Some(value) = facts.paused {
            row.paused = value as u8;
        }
        if let Some(value) = facts.network_in_match {
            row.multiplayer = value as u8;
        }
        if let Some(provider) = target.layout.network_provider.and_then(|rva| target.module_u32(rva)) {
            row.battle_net = is_battle_net(provider) as u8;
        }
        if let Some(value) = facts.latency_raw {
            row.latency = value;
        }
        if let Some(value) = facts.latency_frames {
            row.latency_frames = value;
        }
    }
    if let Some(value) = target.module_u32(scalars.turn_send_count_offset) {
        row.turns_sent = value;
        row.last_turn_qpc = state
            .turn_watch
            .filter(|&(turns, _)| turns == value)
            .map_or(u64::UNKNOWN, |(_, qpc)| qpc);
    }
    if let Some(save) = target.layout.save {
        if let Some(tick) = target.module_u32(save.tick) {
            row.save_tick = tick;
        }
        let mut name = [0; 32];
        let name = &mut name[..save.name_bytes.min(31)];
        if target.read_module(save.name, name).is_ok() {
            row.save_name = text(name);
        }
    }
    if let Some(value) = game_speed_delay_ms(target) {
        row.frame_ms = value;
    }
    if let Some(value) = target.module_u32(scalars.initial_random_seed_offset) {
        row.random_seed = value;
    }
    if let Some(value) = target.module_u32(scalars.replay_frame_count_offset) {
        row.replay_frames = value;
    }

    let raw = &mut state.block;
    raw.resize(GAME_SPAN_END - GAME_SPAN_START, 0);
    if target.root != 0 && target.read(target.root + GAME_SPAN_START, raw).is_ok() {
        let at = |offset: usize| offset - GAME_SPAN_START;
        let path = &raw[at(BWGAME_MAP_FILE_NAME_OFFSET)..][..BWGAME_MAP_FILE_NAME_BYTES];
        row.map_path = text(path);
        row.map_file = file_name(path);
        row.map_title = text(&raw[at(BWGAME_MAP_TITLE_OFFSET)..][..BWGAME_MAP_TITLE_BYTES]);
        row.countdown_seconds = u32_at(raw, at(BWGAME_COUNTDOWN_SECONDS_OFFSET));
        row.elapsed_seconds = i32::try_from(u32_at(raw, at(BWGAME_ELAPSED_TIME_OFFSET))).unwrap_or(i32::UNKNOWN);
    }
    table.push(row);
}

/// Forces 1..4 with their names.
pub(super) fn forces(target: &Target, table: &mut Forces) {
    if target.root == 0 {
        return;
    }
    let mut raw = [0; FORCES * BWGAME_FORCE_NAME_BYTES];
    if target.read(target.root + BWGAME_FORCE_NAMES_OFFSET, &mut raw).is_err() {
        return;
    }
    for (index, name) in raw.chunks_exact(BWGAME_FORCE_NAME_BYTES).enumerate() {
        table.push(ForceRow { id: index as u8 + 1, name: Text32::from_bytes(name) });
    }
}

/// Start locations of the map in slot order, repeats and unusable ones left out.
pub(super) fn start_locations(target: &Target, table: &mut StartLocations) {
    if target.root == 0 {
        return;
    }
    let mut size = [0; BWGAME_MAP_DIMENSIONS_BYTES];
    let mut positions = [0; START_POSITION_BYTES];
    if target.read(target.root + BWGAME_MAP_WIDTH_OFFSET, &mut size).is_err()
        || target.read(target.root + BWGAME_START_POSITIONS_OFFSET, &mut positions).is_err()
    {
        return;
    }
    for (x, y) in start_tiles(&size, &positions).into_iter().flatten() {
        table.push(StartLocationRow { x, y });
    }
}

/// Which unit types each active player may build. The table is indexed by
/// identity slot; rows carry the actual id.
pub(super) fn unit_availability(target: &Target, state: &mut State, table: &mut UnitAvailability) {
    if target.root == 0 {
        return;
    }
    let Some(identities) = read_identities(target) else { return };
    let ids = slot_ids(&identities);
    let raw = &mut state.block;
    raw.resize(ids.len() * BWGAME_UNIT_AVAILABILITY_TYPE_COUNT, 0);
    if target.read(target.root + BWGAME_PLAYER_UNIT_AVAILABILITY_OFFSET, raw).is_err() {
        return;
    }
    for (slot, id) in ids.iter().enumerate() {
        let Some(id) = *id else { continue };
        let row = &raw[slot * BWGAME_UNIT_AVAILABILITY_TYPE_COUNT..][..BWGAME_UNIT_AVAILABILITY_TYPE_COUNT];
        for (unit_type, available) in row.iter().enumerate() {
            table.push(UnitAvailabilityRow { player: id, unit_type: unit_type as u16, available: *available });
        }
    }
}
