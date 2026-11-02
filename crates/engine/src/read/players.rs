use bwrapi_layout::read_pointer;

use super::game::start_tiles;
use super::{i32_at, u32_at};
use crate::columns::{flag, Cell, Text32};
use crate::decode::bwgame::{
    read_current_resources, read_gathered_resources, BWGAME_ALLIANCE_PLAYER_SLOTS, BWGAME_METADATA_READ_BYTES,
    BWGAME_PLAYABLE_PLAYER_COUNT, BWGAME_PLAYER_ALLIANCES_OFFSET, BWGAME_PLAYER_COLOR_INDEX_OFFSET,
    BWGAME_PLAYER_LEFT_OFFSET, BWGAME_PLAYER_SCORE_BUILDING_OFFSET, BWGAME_PLAYER_SCORE_CUSTOM_OFFSET,
    BWGAME_PLAYER_SCORE_KILL_OFFSET, BWGAME_PLAYER_SCORE_RAZING_OFFSET, BWGAME_PLAYER_SCORE_UNIT_OFFSET,
    BWGAME_PLAYER_VICTORY_STATUS_OFFSET, BWGAME_START_POSITIONS_OFFSET, BWGAME_SUPPLY_TABLE_OFFSET,
    BWGAME_UNIT_COUNT_ALL_OFFSET, BWGAME_UNIT_COUNT_COMPLETED_OFFSET, BWGAME_UNIT_COUNT_DEAD_OFFSET,
    BWGAME_UNIT_COUNT_KILLED_OFFSET, BWGAME_UNIT_COUNT_PLAYER_SLOTS, BWGAME_UNIT_COUNT_TABLE_BYTES,
    BWGAME_UNIT_COUNT_TYPE_COUNT, BWGAME_FRAME_COUNT_OFFSET,
};
use crate::decode::participation::{read_participation, PLAYERS, TRIGGER_BYTES, UNIT_TYPES};
use crate::decode::map::BWGAME_MAP_WIDTH_OFFSET;
use crate::decode::scalars::{
    read_supply_total_by_race_from_tables, read_supply_used_by_race_from_tables, PlayerSlotBinding,
    PLAYER_IDENTITY_ENTRY_STRIDE, PLAYER_IDENTITY_TABLE_BYTES, SUPPLY_TABLE_BYTES,
};
use crate::engine::{State, Target};
use crate::tables::*;

pub(super) const IDENTITY_ROWS: usize = PLAYER_IDENTITY_TABLE_BYTES / PLAYER_IDENTITY_ENTRY_STRIDE;
const IDENTITY_TYPE: usize = 8;
const IDENTITY_RACE: usize = 9;
const IDENTITY_TEAM: usize = 10;
const IDENTITY_NAME: usize = 0x0b;
const IDENTITY_NAME_BYTES: usize = 25;
const PLAYER_TYPE_EMPTY: u8 = 0;
const PLAYER_TYPE_CLOSED: u8 = 8;
const PLAYER_TYPE_COMPUTER_LEFT: u8 = 11;
const FORCES: u8 = 4;
/// Network player entries of the lobby tables.
const NET_PLAYERS: usize = 8;
/// The trigger-mode flag; the block is the flag and a pointer.
const TRIGGER_MODE_FLAG_BYTES: usize = 4;
/// Upper bound on the trigger records walked at match start.
const MAX_TRIGGERS: usize = 4096;

pub(super) type Identities = [u8; PLAYER_IDENTITY_TABLE_BYTES];

/// The identity table, or `None` when it cannot be read.
pub(super) fn read_identities(target: &Target) -> Option<Identities> {
    let mut identities = [0; PLAYER_IDENTITY_TABLE_BYTES];
    target.read_module(target.layout.scalars.player_identity_table_offset, &mut identities).ok()?;
    Some(identities)
}

/// Actual player id of each active identity slot. BWGame per-player tables
/// are addressed by the slot, everything published by the actual id. Empty,
/// closed and left rows are not active; a repeated id keeps its first slot.
pub(super) fn slot_ids(identities: &Identities) -> [Option<u8>; IDENTITY_ROWS] {
    let mut ids = [None; IDENTITY_ROWS];
    let mut seen = [false; IDENTITY_ROWS];
    for (slot, id) in ids.iter_mut().enumerate() {
        let entry = &identities[slot * PLAYER_IDENTITY_ENTRY_STRIDE..][..PLAYER_IDENTITY_ENTRY_STRIDE];
        let id_raw = i32_at(entry, 0);
        if matches!(entry[IDENTITY_TYPE], PLAYER_TYPE_EMPTY | PLAYER_TYPE_CLOSED | PLAYER_TYPE_COMPUTER_LEFT)
            || !(0..IDENTITY_ROWS as i32).contains(&id_raw)
            || std::mem::replace(&mut seen[id_raw as usize], true)
        {
            continue;
        }
        *id = Some(id_raw as u8);
    }
    ids
}

/// Identity slots whose network player left a network match: the game keeps
/// the network player's slot mapping and clears its flags, while BWGame's own
/// left table stays 0. All false outside a network match; `None` when
/// unreadable, where only BWGame's table counts.
fn network_left(target: &Target) -> Option<[bool; IDENTITY_ROWS]> {
    let mut left = [false; IDENTITY_ROWS];
    let mut in_match = [0; 1];
    target.read_module(target.layout.scalars.session_network_in_match_offset, &mut in_match).ok()?;
    if in_match[0] != 1 {
        return Some(left);
    }
    let mut to_game = [0; NET_PLAYERS * 4];
    let mut flags = [0; NET_PLAYERS * 4];
    target.read_module(target.layout.players.net_player_to_game, &mut to_game).ok()?;
    target.read_module(target.layout.players.net_player_flags, &mut flags).ok()?;
    for net in 0..NET_PLAYERS {
        let slot = u32_at(&to_game, net * 4) as usize;
        if slot < IDENTITY_ROWS && u32_at(&flags, net * 4) == 0 {
            left[slot] = true;
        }
    }
    Some(left)
}

fn score(raw: &[u8], base: usize, slot: usize) -> i32 {
    Some(i32_at(raw, base + slot * 4)).filter(|value| *value >= 0).unwrap_or(i32::UNKNOWN)
}

/// Occupied player slots: identity, resources, supply, scores, alliances.
/// The BWGame root tables are indexed by identity slot, the rows carry the
/// actual id.
pub(super) fn players(target: &Target, state: &mut State, table: &mut Players) {
    let Some(identities) = read_identities(target) else { return };
    let ids = slot_ids(&identities);
    let root = &mut state.block;
    root.resize(BWGAME_METADATA_READ_BYTES, 0);
    let root = (target.root != 0 && target.read(target.root, root).is_ok()).then_some(&root[..]);
    let starts = root.map(|raw| {
        start_tiles(
            &raw[BWGAME_MAP_WIDTH_OFFSET..][..4],
            &raw[BWGAME_START_POSITIONS_OFFSET..][..BWGAME_PLAYABLE_PLAYER_COUNT * 4],
        )
    });
    // Participation is decided once, on the match's first frame.
    let network_left = network_left(target);
    let participating = root
        .filter(|raw| u32_at(raw, BWGAME_FRAME_COUNT_OFFSET) == 0)
        .and_then(|raw| start_participation(target, raw, &identities, &ids));
    for (slot, id) in ids.iter().enumerate() {
        let Some(id) = *id else { continue };
        let entry = &identities[slot * PLAYER_IDENTITY_ENTRY_STRIDE..][..PLAYER_IDENTITY_ENTRY_STRIDE];
        let name = &entry[IDENTITY_NAME..][..IDENTITY_NAME_BYTES];
        let team = entry[IDENTITY_TEAM];
        let mut row = PlayerRow {
            id,
            race: entry[IDENTITY_RACE],
            name: if name.contains(&0) { Text32::from_bytes(name) } else { Text32::UNKNOWN },
            player_type: entry[IDENTITY_TYPE],
            force: if (1..=FORCES).contains(&team) { team } else { 0 },
            participating: match participating {
                Some(mask) if usize::from(id) < PLAYERS => flag(mask >> id & 1 != 0),
                _ => u8::UNKNOWN,
            },
            ..PlayerRow::UNKNOWN
        };
        if let (Some(raw), Some(binding)) = (root, PlayerSlotBinding::new(slot as u8, id)) {
            if let Some(current) = read_current_resources(raw, binding) {
                row.minerals = current.minerals;
                row.gas = current.gas;
            }
            if let Some(totals) = state.resource_totals.as_ref().and_then(|totals| totals.get(slot)) {
                row.repaired_minerals = totals.repaired_minerals;
                row.repaired_gas = totals.repaired_gas;
                row.refunded_minerals = totals.refunded_minerals;
                row.refunded_gas = totals.refunded_gas;
            }
            if let Some(gathered) = read_gathered_resources(raw, binding) {
                row.gathered_minerals = gathered.gathered_minerals;
                row.gathered_gas = gathered.gathered_gas;
            }
            let supply = &raw[BWGAME_SUPPLY_TABLE_OFFSET..][..SUPPLY_TABLE_BYTES];
            if let Some([zerg, terran, protoss]) = read_supply_used_by_race_from_tables(supply, slot as u8) {
                (row.supply_used_zerg, row.supply_used_terran, row.supply_used_protoss) = (zerg, terran, protoss);
            }
            if let Some([zerg, terran, protoss]) = read_supply_total_by_race_from_tables(supply, slot as u8) {
                (row.supply_total_zerg, row.supply_total_terran, row.supply_total_protoss) = (zerg, terran, protoss);
            }
            row.color = raw[BWGAME_PLAYER_COLOR_INDEX_OFFSET + slot];
            row.victory = raw[BWGAME_PLAYER_VICTORY_STATUS_OFFSET + slot];
            row.left_game = match (raw[BWGAME_PLAYER_LEFT_OFFSET + slot], network_left) {
                (1, _) => 1,
                (0, network) => flag(network.is_some_and(|left| left[slot])),
                _ => u8::UNKNOWN,
            };
            row.unit_score = score(raw, BWGAME_PLAYER_SCORE_UNIT_OFFSET, slot);
            row.kill_score = score(raw, BWGAME_PLAYER_SCORE_KILL_OFFSET, slot);
            row.building_score = score(raw, BWGAME_PLAYER_SCORE_BUILDING_OFFSET, slot);
            row.razing_score = score(raw, BWGAME_PLAYER_SCORE_RAZING_OFFSET, slot);
            row.custom_score = score(raw, BWGAME_PLAYER_SCORE_CUSTOM_OFFSET, slot);
            // The alliance matrix lists slots; a slot is published as its actual id.
            let alliances = &raw[BWGAME_PLAYER_ALLIANCES_OFFSET + slot * BWGAME_ALLIANCE_PLAYER_SLOTS..];
            row.allies = (0..BWGAME_PLAYABLE_PLAYER_COUNT)
                .filter(|other| alliances[*other] != 0)
                .filter_map(|other| ids[other])
                .fold(0, |mask: u16, other_id| mask | 1 << other_id);
        }
        if let Some((x, y)) = starts.as_ref().and_then(|starts| starts[slot]) {
            (row.start_x, row.start_y) = (x, y);
        }
        table.push(row);
    }
}

/// Participation bit per actual player id, by the BWAPI match-start rule
/// (GameEvents onGameStart): a player owns a unit, or a trigger of any
/// executor whose conditions can be met creates or gives units to it. The
/// unit counts are indexed by identity slot, the trigger lists and teams by
/// actual id. `None` when a list cannot be walked consistently.
fn start_participation(
    target: &Target,
    root: &[u8],
    identities: &Identities,
    ids: &[Option<u8>; IDENTITY_ROWS],
) -> Option<u8> {
    let layout = &target.layout.players;
    let mut mode = [0u8; TRIGGER_MODE_FLAG_BYTES + 8];
    let mode = &mut mode[..TRIGGER_MODE_FLAG_BYTES + target.layout.pointer_bytes];
    target.read_module(layout.participation_mode, mode).ok()?;
    if mode.iter().any(|byte| *byte != 0) {
        return None;
    }
    let mut counts = [[0u32; UNIT_TYPES]; PLAYERS];
    let mut teams = [0u8; PLAYERS];
    for (slot, id) in ids.iter().enumerate().take(PLAYERS) {
        let Some(id) = id.map(usize::from).filter(|id| *id < PLAYERS) else { continue };
        teams[id] = identities[slot * PLAYER_IDENTITY_ENTRY_STRIDE + IDENTITY_TEAM];
        for (unit_type, count) in counts[id].iter_mut().enumerate() {
            let at = BWGAME_UNIT_COUNT_ALL_OFFSET + (unit_type * BWGAME_UNIT_COUNT_PLAYER_SLOTS + slot) * 4;
            *count = u32::try_from(i32_at(root, at)).ok()?;
        }
    }
    let records = trigger_records(target)?;
    let lists: [Vec<&[u8]>; PLAYERS] =
        std::array::from_fn(|executor| records[executor].chunks_exact(TRIGGER_BYTES).collect());
    let mask = read_participation(Some(&counts), Some(&lists), Some(&teams));
    (mask.known == u8::MAX).then_some(mask.participating)
}

/// The trigger records of each executor's list, concatenated. The table
/// holds one list head per executor: last node, first node and node count,
/// each a pointer-sized word; a node is a prev/next link pair followed by
/// the trigger record, and the list closes on the head itself.
fn trigger_records(target: &Target) -> Option<[Vec<u8>; PLAYERS]> {
    let word = target.layout.pointer_bytes;
    let rva = target.layout.players.participation_table;
    let mut table = vec![0u8; PLAYERS * 3 * word];
    target.read_module(rva, &mut table).ok()?;
    let table_address = target.module.range_address(rva, table.len())?;
    let mut records: [Vec<u8>; PLAYERS] = Default::default();
    let mut node_raw = vec![0u8; 2 * word + TRIGGER_BYTES];
    let mut walked = 0usize;
    for (executor, list) in records.iter_mut().enumerate() {
        let head = executor * 3 * word;
        let sentinel = table_address + head;
        let last = read_pointer(&table, head, word)?;
        let mut node = read_pointer(&table, head + word, word)?;
        let count = read_pointer(&table, head + 2 * word, word)?;
        walked = walked.checked_add(count).filter(|total| *total <= MAX_TRIGGERS)?;
        let mut previous = sentinel;
        for _ in 0..count {
            if node == sentinel || node == 0 {
                return None;
            }
            target.read(node, &mut node_raw).ok()?;
            if read_pointer(&node_raw, 0, word)? != previous {
                return None;
            }
            list.extend_from_slice(&node_raw[2 * word..]);
            previous = node;
            node = read_pointer(&node_raw, word, word)?;
        }
        if node != sentinel || last != previous {
            return None;
        }
    }
    Some(records)
}

/// Unit counts per player and type: all, completed, killed and dead. The
/// count tables are indexed by identity slot.
pub(super) fn unit_counts(target: &Target, state: &mut State, table: &mut UnitCounts) {
    if target.root == 0 {
        return;
    }
    let Some(identities) = read_identities(target) else { return };
    let ids = slot_ids(&identities);
    let raw = &mut state.block;
    let span = BWGAME_UNIT_COUNT_DEAD_OFFSET + BWGAME_UNIT_COUNT_TABLE_BYTES - BWGAME_UNIT_COUNT_ALL_OFFSET;
    raw.resize(span, 0);
    if target.read(target.root + BWGAME_UNIT_COUNT_ALL_OFFSET, raw).is_err() {
        return;
    }
    let count = |table: usize, at: usize| {
        u32::try_from(i32_at(raw, table - BWGAME_UNIT_COUNT_ALL_OFFSET + at)).unwrap_or(u32::UNKNOWN)
    };
    for (slot, id) in ids.iter().enumerate() {
        let Some(id) = *id else { continue };
        for unit_type in 0..BWGAME_UNIT_COUNT_TYPE_COUNT {
            let at = (unit_type * BWGAME_UNIT_COUNT_PLAYER_SLOTS + slot) * 4;
            let row = UnitCountRow {
                player: id,
                unit_type: unit_type as u16,
                all: count(BWGAME_UNIT_COUNT_ALL_OFFSET, at),
                completed: count(BWGAME_UNIT_COUNT_COMPLETED_OFFSET, at),
                killed: count(BWGAME_UNIT_COUNT_KILLED_OFFSET, at),
                dead: count(BWGAME_UNIT_COUNT_DEAD_OFFSET, at),
            };
            if [row.all, row.completed, row.killed, row.dead].iter().any(|count| *count != 0) {
                table.push(row);
            }
        }
    }
}

pub(super) fn viewport(target: &Target, table: &mut Viewport) {
    let scalars = &target.layout.scalars;
    if let (Some(x), Some(y)) = (
        target.module_u32(scalars.viewport_map_x_offset),
        target.module_u32(scalars.viewport_map_y_offset),
    ) {
        let (mouse_x, mouse_y) = mouse(target).unwrap_or((i32::MIN, i32::MIN));
        table.push(ViewportRow { x: x as i32, y: y as i32, mouse_x, mouse_y });
    }
}

fn mouse(target: &Target) -> Option<(i32, i32)> {
    let layout = target.layout.mouse?;
    let width = target.layout.pointer_bytes;
    let holder = target.module_pointer(layout.pointer)?;
    let mut raw = [0; 8];
    target.read(holder, &mut raw[..width]).ok()?;
    let point = bwrapi_layout::read_pointer(&raw, 0, width)?.checked_add(layout.offset)?;
    target.read(point, &mut raw).ok()?;
    Some((i32::from_le_bytes(raw[..4].try_into().ok()?), i32::from_le_bytes(raw[4..].try_into().ok()?)))
}
