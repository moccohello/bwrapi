use super::pool::Pool;
use super::u16_at;
use crate::columns::flag;
use crate::decode::bwgame::*;
use crate::decode::cunit::*;
use super::players::IDENTITY_ROWS;
use crate::engine::{State, Target};
use crate::tables::*;

/// Research or upgrade running in a building: (player, type, remaining frames).
pub(super) type Progress = Vec<(u8, u16, u32)>;

/// Fills `progress` with the research or upgrades in progress, from the
/// buildings doing them; the shortest remaining time wins when two buildings
/// work on the same type.
fn in_progress(pool: &Pool, order: u8, type_field: usize, max_type: u8, progress: &mut Progress) {
    let c = pool.cunit;
    for slot in 0..pool.slots() {
        let record = pool.record(slot);
        let remaining = u32::from(u16_at(record, c.production_remaining_time_offset));
        let kind = record[type_field];
        if !pool.is_live(slot) || record[c.order_offset] != order || remaining == 0 || kind > max_type
            || matches!(u16_at(record, c.type_offset), 36 | 59 | 97)
        {
            continue;
        }
        let (player, kind) = (record[c.owner_offset], u16::from(kind));
        match progress.iter_mut().find(|entry| entry.0 == player && entry.1 == kind) {
            Some(entry) => entry.2 = entry.2.min(remaining),
            None => progress.push((player, kind, remaining)),
        }
    }
}

/// Fills the reused progress buffer of `state`: research when `research`,
/// upgrades otherwise. The buffer is empty when the pool was not read.
pub(super) fn fill_progress(target: &Target, state: &mut State, research: bool) {
    let cunit = &target.layout.cunit;
    let (order, type_field, max_type) = if research {
        (CUNIT_ORDER_RESEARCH, cunit.research_tech_type_offset, CUNIT_TECH_TYPE_MAX)
    } else {
        (CUNIT_ORDER_UPGRADE, cunit.upgrade_type_offset, CUNIT_UPGRADE_TYPE_MAX)
    };
    let raw = (state.pool_base != 0).then(|| state.cunits.as_slice());
    let progress = if research { &mut state.tech_progress } else { &mut state.upgrade_progress };
    progress.clear();
    let Some(raw) = raw else { return };
    let pool = Pool {
        raw,
        base: state.pool_base,
        pointer_bytes: target.layout.pointer_bytes,
        cunit,
        reached: &state.reached,
    };
    in_progress(&pool, order, type_field, max_type, progress);
}

/// Remaining frames of an in-progress entry: from its building, unknown
/// when no building is seen working on it.
fn remaining(progress: &Progress, player: usize, kind: usize, in_progress: bool) -> u32 {
    if !in_progress {
        return 0;
    }
    progress
        .iter()
        .find(|entry| usize::from(entry.0) == player && usize::from(entry.1) == kind)
        .map_or(u32::MAX, |entry| entry.2)
}

const PLAYERS: usize = IDENTITY_ROWS;
const FIRST: usize = BWGAME_TECH_AVAILABLE_SC_OFFSET;
const END: usize = BWGAME_UPGRADE_IN_PROGRESS_OFFSET + PLAYERS * BWGAME_UPGRADE_IN_PROGRESS_STRIDE;

/// Reads the upgrade and tech span of the BWGame root into `state.block`;
/// the block is left empty when the span could not be read.
pub(super) fn read_span(target: &Target, state: &mut State) {
    state.block.clear();
    if target.root == 0 {
        return;
    }
    state.block.resize(END - FIRST, 0);
    if target.read(target.root + FIRST, &mut state.block).is_err() {
        state.block.clear();
    }
}

fn bit(raw: &[u8], base: usize, index: usize) -> bool {
    raw[base - FIRST + index / 8] & 1 << (index % 8) != 0
}

/// Upgrade levels per active player; the tables are indexed by identity slot
/// and the rows carry the player id. Entries with a level above the maximum or
/// a maximum of 0 or above 3 are left out.
/// The span must be in `state.block` and the upgrade progress in
/// `state.upgrade_progress`.
pub(super) fn upgrades(state: &State, ids: &[Option<u8>; PLAYERS], table: &mut Upgrades) {
    let raw = state.block.as_slice();
    if raw.is_empty() {
        return;
    }
    let progress = &state.upgrade_progress;
    for (slot, id) in ids.iter().enumerate() {
        let Some(id) = *id else { continue };
        for upgrade_type in 0..BWGAME_TOTAL_UPGRADE_COUNT {
            let (count, current, max, index) = if upgrade_type < BWGAME_SC_UPGRADE_COUNT {
                (BWGAME_SC_UPGRADE_COUNT, BWGAME_CURRENT_UPGRADE_SC_OFFSET, BWGAME_MAX_UPGRADE_SC_OFFSET, upgrade_type)
            } else {
                (BWGAME_BW_UPGRADE_COUNT, BWGAME_CURRENT_UPGRADE_BW_OFFSET, BWGAME_MAX_UPGRADE_BW_OFFSET, upgrade_type - BWGAME_SC_UPGRADE_COUNT)
            };
            let level = raw[current - FIRST + slot * count + index];
            let max_level = raw[max - FIRST + slot * count + index];
            if level > max_level || max_level > BWGAME_MAX_UPGRADE_LEVEL || max_level == 0 {
                continue;
            }
            let in_progress = bit(raw, BWGAME_UPGRADE_IN_PROGRESS_OFFSET + slot * BWGAME_UPGRADE_IN_PROGRESS_STRIDE, upgrade_type);
            table.push(UpgradeRow {
                player: id,
                upgrade_type: upgrade_type as u16,
                level,
                max_level,
                in_progress: flag(in_progress),
                remaining_frames: remaining(progress, usize::from(id), upgrade_type, in_progress),
            });
        }
    }
}

/// Tech state per active player; the tables are indexed by identity slot and
/// the rows carry the player id. Entries with an invalid flag byte are left out.
/// The span must be in `state.block` and the research progress in
/// `state.tech_progress`.
pub(super) fn techs(state: &State, ids: &[Option<u8>; PLAYERS], table: &mut Techs) {
    let raw = state.block.as_slice();
    if raw.is_empty() {
        return;
    }
    let progress = &state.tech_progress;
    let flag_at = |offset: usize| match raw[offset - FIRST] {
        value @ (0 | 1) => Some(value),
        _ => None,
    };
    for (slot, id) in ids.iter().enumerate() {
        let Some(id) = *id else { continue };
        for tech_type in 0..BWGAME_TOTAL_TECH_COUNT {
            let (count, researched, available, index) = if tech_type < BWGAME_SC_TECH_COUNT {
                (BWGAME_SC_TECH_COUNT, BWGAME_TECH_RESEARCHED_SC_OFFSET, BWGAME_TECH_AVAILABLE_SC_OFFSET, tech_type)
            } else {
                (BWGAME_BW_TECH_COUNT, BWGAME_TECH_RESEARCHED_BW_OFFSET, BWGAME_TECH_AVAILABLE_BW_OFFSET, tech_type - BWGAME_SC_TECH_COUNT)
            };
            let at = slot * count + index;
            let (Some(researched), Some(available)) = (flag_at(researched + at), flag_at(available + at)) else { continue };
            // One research bitset, indexed by the full tech id.
            let in_progress = bit(raw, BWGAME_TECH_RESEARCHING_BITSET_OFFSET + slot * BWGAME_TECH_BITSET_STRIDE, tech_type);
            table.push(TechRow {
                player: id,
                tech_type: tech_type as u16,
                researched,
                available,
                in_progress: flag(in_progress),
                remaining_frames: remaining(progress, usize::from(id), tech_type, in_progress),
            });
        }
    }
}
