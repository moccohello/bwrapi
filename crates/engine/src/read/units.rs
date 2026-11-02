use bwrapi_layout::read_pointer;
use bwrapi_internal_model::orders::is_bwapi_completed;

use super::pool::{self, Pool};
use super::{i16_at, i32_at, u16_at, u32_at};
use crate::columns::{flag, Cell, Table};
use crate::decode::cunit::*;
use crate::engine::{State, Target};
use crate::identity::unit_slot;
use crate::tables::*;

const RESOURCE_TYPES: [u16; 7] = [110, 149, 157, 176, 177, 178, 188];
/// Flag or mineral chunk a worker can carry (power-up units).
const POWER_UP_TYPE_ID: u16 = 215;

pub(super) fn units(target: &Target, state: &State, table: &mut Units) {
    let Some(pool) = pool::get(target, state) else { return };
    for slot in 0..pool.slots() {
        if pool.is_live(slot) {
            let record = pool.record(slot);
            table.push(unit_row(&pool, record, slot));
        }
    }
    apply_larva_cap(table);
}

fn unit_row(pool: &Pool, record: &[u8], slot: usize) -> UnitRow {
    let c = pool.cunit;
    let unit_type = u16_at(record, c.type_offset);
    let owner = record[c.owner_offset];
    let order = record[c.order_offset];
    let status = u32_at(record, c.status_flags_offset);
    let has = |bit: u32| flag(status & bit != 0);
    let movement_flags = record[c.movement_flags_offset];
    let secondary_order = record[c.secondary_order_offset];
    let production = production(pool, record);
    let training = training(pool, record, slot);
    let (loaded, transport) = loaded_transport(pool, record, status);
    let rally = rally(pool, record, order);
    let (move_target, move_target_x, move_target_y) = move_target_columns(pool, record, slot);
    let (scarabs, interceptors) = hangar_counts(pool, slot, unit_type);
    UnitRow {
        id: pool.id(slot),
        unit_type,
        owner,
        x: i16_at(record, c.position_x_offset),
        y: i16_at(record, c.position_y_offset),
        hit_points: i32_at(record, c.hit_points_offset),
        shields: if unit_type_has_shields(unit_type) { i32_at(record, c.shields_offset) } else { 0 },
        energy: if unit_type_has_energy(unit_type) { u16_at(record, c.energy_raw_offset) } else { u16::UNKNOWN },
        resources: if RESOURCE_TYPES.contains(&unit_type) { u16_at(record, c.resource_amount_offset) } else { u16::UNKNOWN },
        order,
        order_target: pool.target_id(record, c.order_target_unit_ptr_offset),
        order_target_x: i16_at(record, c.order_target_position_x_offset),
        order_target_y: i16_at(record, c.order_target_position_y_offset),
        secondary_order: if usize::from(secondary_order) < bwrapi_internal_model::orders::ORDER_COUNT { secondary_order } else { u8::UNKNOWN },
        status_flags: status,
        detected_by: u32_at(record, c.detection_player_flags_offset),
        completed: flag(status & CUNIT_STATUS_COMPLETED != 0 && !matches!(order, 41 | 42 | 43 | 45)),
        burrowed: has(CUNIT_STATUS_BURROWED),
        cloaked: flag(status & CUNIT_STATUS_CLOAKED != 0 && status & CUNIT_STATUS_BURROWED == 0),
        in_air: has(CUNIT_STATUS_IN_AIR),
        hallucination: has(CUNIT_STATUS_HALLUCINATION),
        interruptible: flag(status & CUNIT_STATUS_CANNOT_RECEIVE_ORDERS == 0),
        powered: flag(!protoss_unit_type_requires_power(unit_type) || status & CUNIT_STATUS_PROTOSS_UNPOWERED == 0),
        invincible: has(CUNIT_STATUS_INVINCIBLE),
        loaded,
        transport,
        addon: addon_id(pool, record),
        hatchery: hatchery_id(pool, record, slot),
        build_type: production
            .build_type
            .or(training.head())
            .unwrap_or(if production.build_type_known { CUNIT_UNIT_TYPE_NONE } else { u16::UNKNOWN }),
        remaining_build_frames: production.remaining_build_frames.unwrap_or(u32::UNKNOWN),
        researching: flag(production.researching),
        upgrading: flag(production.upgrading),
        spider_mines: if matches!(unit_type, 2 | 19) { record[c.vulture_spider_mine_count_offset] } else { u8::UNKNOWN },
        lockdown_timer: record[c.lockdown_timer_offset],
        maelstrom_timer: record[c.maelstrom_timer_offset],
        stasis_timer: record[c.stasis_timer_offset],
        irradiate_timer: record[c.irradiate_timer_offset],
        plague_timer: record[c.plague_timer_offset],
        ensnare_timer: record[c.ensnare_timer_offset],
        defense_matrix_timer: record[c.defense_matrix_timer_offset],
        remove_timer: u16_at(record, c.remove_timer_offset),
        rally_x: rally.0,
        rally_y: rally.1,
        rally_unit: rally.2,
        gas_building: gas_building_id(pool, record),
        larva_frames: larva_regeneration_frames(pool, record, unit_type),
        remaining_train_frames: training.remaining.unwrap_or(u32::UNKNOWN),
        train_queue_0: training.queue[0],
        train_queue_1: training.queue[1],
        train_queue_2: training.queue[2],
        train_queue_3: training.queue[3],
        train_queue_4: training.queue[4],
        tech: production.tech.unwrap_or(u8::UNKNOWN),
        remaining_research_frames: production.remaining_research_frames.unwrap_or(u32::UNKNOWN),
        upgrade: production.upgrade.unwrap_or(u8::UNKNOWN),
        remaining_upgrade_frames: production.remaining_upgrade_frames.unwrap_or(u32::UNKNOWN),
        build_unit: build_unit_id(pool, record, slot),
        move_target,
        move_target_x,
        move_target_y,
        sub_unit: sub_unit_id(pool, record),
        carrier: carrier_id(pool, record, slot),
        nydus_exit: nydus_exit_id(pool, record, slot),
        power_up: power_up_id(pool, record, status),
        order_timer: u16::from(record[c.order_timer_offset]),
        stim_timer: record[c.stim_timer_offset],
        storm_timer: record[c.storm_timer_offset],
        defense_matrix_damage: u16_at(record, c.defense_matrix_damage_offset),
        ground_cooldown: record[c.ground_weapon_cooldown_offset],
        air_cooldown: record[c.air_weapon_cooldown_offset],
        spell_cooldown: record[c.spell_cooldown_offset],
        kill_count: u16::from(record[c.kill_count_offset]),
        acid_spores: record[c.acid_spore_count_offset],
        scarabs,
        interceptors,
        resource_group: resource_group(record, c, unit_type, owner),
        carry: if is_worker(unit_type) {
            match record[c.worker_resource_type_offset] {
                kind @ (0 | 1 | 2) => kind,
                _ => u8::UNKNOWN,
            }
        } else {
            u8::UNKNOWN
        },
        last_attacker: match record[c.last_attacking_player_id_offset] {
            attacker if attacker < 12 => attacker,
            _ => u8::UNKNOWN,
        },
        recently_attacked: flag(
            record[c.last_event_color_offset] == 174 && record[c.last_event_timer_offset] != 0,
        ),
        button_set: u16_at(record, c.button_set_id_offset),
        previous_hit_points: u16_at(record, c.previous_hit_points_raw_offset),
        being_healed: flag(record[c.is_being_healed_offset] != 0),
        gathering: is_worker(unit_type)
            .then(|| has(CUNIT_STATUS_IS_GATHERING))
            .unwrap_or(u8::UNKNOWN),
        being_gathered: being_gathered(pool, record, unit_type),
        blind: flag(record[c.is_blind_offset] != 0),
        parasited: flag(record[c.parasite_flags_offset] != 0),
        has_nuke: flag(unit_type == CUNIT_NUCLEAR_SILO_TYPE_ID && record[c.silo_nuke_ready_offset] != 0),
        stuck: flag(record[c.movement_state_offset] == CUNIT_MOVEMENT_STATE_MOVE_TO_LEGAL),
        moving: flag(movement_flags & (CUNIT_MOVEMENT_FLAG_MOVING | CUNIT_MOVEMENT_FLAG_ACCELERATING) != 0),
        accelerating: flag(movement_flags & CUNIT_MOVEMENT_FLAG_ACCELERATING != 0),
        braking: flag(movement_flags & CUNIT_MOVEMENT_FLAG_BRAKING != 0),
        facing: u16::from(record[c.current_direction_1_offset]),
        velocity_x: i32_at(record, c.velocity_x_offset),
        velocity_y: i32_at(record, c.velocity_y_offset),
    }
}

/// A resource container is being gathered while a worker waits in its
/// gather queue or the next-gatherer link names one (UnitUpdate
/// `gatherQueueCount || nextGatherer`).
fn being_gathered(pool: &Pool, record: &[u8], unit_type: u16) -> u8 {
    let c = pool.cunit;
    if !RESOURCE_TYPES.contains(&unit_type) {
        return u8::UNKNOWN;
    }
    match read_pointer(record, c.resource_next_gatherer_ptr_offset, pool.pointer_bytes) {
        Some(next) => flag(record[c.resource_gather_queue_count_offset] != 0 || next != 0),
        None => u8::UNKNOWN,
    }
}

/// Rally columns per the reference rally decoder: the active rally position
/// of a unit type that rallies — lifted Terran buildings and unset or
/// off-map positions are not active — unknown for types that cannot rally.
/// The rally-unit pointer keeps the pointer-column convention: 0 for none,
/// unknown outside the pool.
fn rally(pool: &Pool, record: &[u8], order: u8) -> (i16, i16, u64) {
    let c = pool.cunit;
    let unit_type = u16_at(record, c.type_offset);
    if !unit_type_can_rally(unit_type) {
        return (i16::UNKNOWN, i16::UNKNOWN, u64::UNKNOWN);
    }
    let position = active_rally_position(
        unit_type,
        order,
        u32_at(record, c.status_flags_offset),
        i16_at(record, c.rally_position_x_offset),
        i16_at(record, c.rally_position_y_offset),
    );
    let (x, y) = position.map_or((i16::UNKNOWN, i16::UNKNOWN), |position| {
        (i16::try_from(position.x).unwrap_or(i16::UNKNOWN),
            i16::try_from(position.y).unwrap_or(i16::UNKNOWN))
    });
    (x, y, pool.target_id(record, c.rally_unit_ptr_offset))
}

/// Id of the Refinery/Extractor/Assimilator a gas worker gathers from: worker
/// types with a gather order whose gas-resource pointer resolves to a live,
/// same-owner gas building.
fn gas_building_id(pool: &Pool, record: &[u8]) -> u64 {
    let c = pool.cunit;
    if !matches!(u16_at(record, c.type_offset), 7 | 41 | 64)
        || !matches!(record[c.order_offset], 79..=84 | 91 | 151)
    {
        return 0;
    }
    let Some(slot) = pool.target_slot(record, c.gas_resource_ptr_offset) else {
        return 0;
    };
    let target = pool.record(slot);
    (record[c.owner_offset] == target[c.owner_offset]
        && i32_at(target, c.hit_points_offset) > 0
        && matches!(u16_at(target, c.type_offset), 110 | 149 | 157))
        .then(|| pool.id(slot))
        .unwrap_or(0)
}

/// Hatchery-family larva countdown as BWAPI publishes it for every
/// Hatchery, Lair and Hive, in any state (its incomplete-hatchery branch
/// can never be taken): `larva_timer * 9 + ((order_queue_timer + 8) % 9)`.
/// Unknown for every other type.
fn larva_regeneration_frames(pool: &Pool, record: &[u8], unit_type: u16) -> u32 {
    let c = pool.cunit;
    if !CUNIT_ZERG_HATCHERY_FAMILY_TYPE_IDS.contains(&unit_type) {
        return u32::UNKNOWN;
    }
    u32::from(record[c.larva_timer_offset]) * 9
        + ((u32::from(record[c.order_queue_timer_offset]) + 8) % 9)
}

/// Official augmentUnitData clears a hatchery's larva countdown once three
/// larvae are connected to it — regeneration stops while the hatchery is
/// full. The counts come from the rows' own `hatchery` relation, so this
/// runs after every row exists.
fn apply_larva_cap(table: &mut Units) {
    let mut larva_counts = [0u8; CUNIT_RECORD_COUNT];
    for row in 0..table.rows() {
        if table.unit_type[row] != CUNIT_ZERG_LARVA_TYPE_ID {
            continue;
        }
        let hatchery = table.hatchery[row];
        if hatchery == 0 || hatchery == u64::UNKNOWN {
            continue;
        }
        if let Some(count) = larva_counts.get_mut(usize::from(unit_slot(hatchery))) {
            *count = count.saturating_add(1);
        }
    }
    for row in 0..table.rows() {
        if table.larva_frames[row] == u32::UNKNOWN {
            continue;
        }
        if larva_counts.get(usize::from(unit_slot(table.id[row]))).is_some_and(|count| *count >= 3) {
            table.larva_frames[row] = 0;
        }
    }
}

/// (loaded flag, transport id): a Bunker or air-transport relation, unknown
/// when the passenger bits are set but the relation does not resolve.
fn loaded_transport(pool: &Pool, record: &[u8], status: u32) -> (u8, u64) {
    let c = pool.cunit;
    let in_building = status & CUNIT_STATUS_IN_BUILDING != 0;
    let in_transport = status & CUNIT_STATUS_IN_TRANSPORT != 0;
    if !in_building && !in_transport {
        return (flag(false), 0);
    }
    let unknown = (u8::UNKNOWN, u64::UNKNOWN);
    let Some(slot) = pool.target_slot(record, c.connected_unit_ptr_offset) else {
        return unknown;
    };
    let target = pool.record(slot);
    // InBuilding means a Bunker passenger; the other branch is an air
    // transport (Dropship/Overlord/Shuttle/Arbiter).
    let valid_type = if in_building {
        u16_at(target, c.type_offset) == CUNIT_BUNKER_TYPE_ID
    } else {
        matches!(u16_at(target, c.type_offset), 11 | 42 | 57 | 69)
    };
    (valid_type
        && target[c.owner_offset] == record[c.owner_offset]
        && i32_at(target, c.hit_points_offset) > 0
        && u32_at(target, c.status_flags_offset) & CUNIT_STATUS_COMPLETED != 0
        && (in_building || target[c.order_offset] != 0))
        .then(|| (flag(true), pool.id(slot)))
        .unwrap_or(unknown)
}

/// Id of the building's addon: the attached addon pointer, or the addon under
/// construction through the build-child pointer.
fn addon_id(pool: &Pool, record: &[u8]) -> u64 {
    let c = pool.cunit;
    let unit_type = u16_at(record, c.type_offset);
    let owner = record[c.owner_offset];
    if !matches!(unit_type, 106 | 113 | 114 | 116) {
        return 0;
    }
    if let Some(slot) = pool.target_slot(record, c.building_addon_ptr_offset) {
        let target = pool.record(slot);
        if is_ordinary_building_addon_pair(unit_type, u16_at(target, c.type_offset))
            && target[c.owner_offset] == owner
            && target[c.order_offset] != 0
            && i32_at(target, c.hit_points_offset) > 0
        {
            return pool.id(slot);
        }
    }
    let Some(slot) = pool.target_slot(record, c.build_unit_ptr_offset) else {
        return 0;
    };
    let target = pool.record(slot);
    (is_ordinary_building_addon_pair(unit_type, u16_at(target, c.type_offset))
        && target[c.owner_offset] == owner)
        .then(|| pool.id(slot))
        .unwrap_or(0)
}

/// Id of a larva's hatchery; other types carry no relation.
fn hatchery_id(pool: &Pool, record: &[u8], larva_slot: usize) -> u64 {
    let c = pool.cunit;
    if u16_at(record, c.type_offset) != CUNIT_ZERG_LARVA_TYPE_ID {
        return 0;
    }
    if read_pointer(record, c.connected_unit_ptr_offset, pool.pointer_bytes) == Some(0) {
        return 0;
    }
    let unknown = u64::UNKNOWN;
    let Some(slot) = pool.target_slot(record, c.connected_unit_ptr_offset) else {
        return unknown;
    };
    if slot == larva_slot {
        return unknown;
    }
    let target = pool.record(slot);
    (CUNIT_ZERG_HATCHERY_FAMILY_TYPE_IDS.contains(&u16_at(target, c.type_offset))
        && target[c.owner_offset] == record[c.owner_offset]
        && i32_at(target, c.hit_points_offset) > 0)
        .then(|| pool.id(slot))
        .unwrap_or(unknown)
}

/// Slot of the live build-child the training relation accepts.
fn live_build_child_slot(pool: &Pool, record: &[u8], producer_slot: usize) -> Option<usize> {
    let c = pool.cunit;
    let child_slot = pool.target_slot(record, c.build_unit_ptr_offset)?;
    if producer_slot >= CUNIT_RECORD_COUNT
        || child_slot >= CUNIT_RECORD_COUNT
        || child_slot == producer_slot
    {
        return None;
    }
    let child = pool.record(child_slot);
    let child_type = u16_at(child, c.type_offset);
    let child_hit_points = i32_at(child, c.hit_points_offset);
    (child[c.owner_offset] == record[c.owner_offset]
        && child_type < CUNIT_UNIT_TYPE_NONE
        && child[c.order_offset] != 0
        && child_hit_points > 0
        && plausible_cunit_record(
            child[c.owner_offset],
            child_type,
            child_hit_points,
            u32_at(child, c.shields_offset),
            i32::from(i16_at(child, c.position_x_offset)),
            i32::from(i16_at(child, c.position_y_offset)),
        ))
    .then_some(child_slot)
}

/// Id of the live unit this unit produces through the build-child pointer
/// (a building under construction, an addon, a trained unit), per the
/// reference relation: 0 when the pointer is null, unknown when it names
/// no live child.
fn build_unit_id(pool: &Pool, record: &[u8], slot: usize) -> u64 {
    if let Some(child) = live_build_child_slot(pool, record, slot) {
        return pool.id(child);
    }
    match read_pointer(record, pool.cunit.build_unit_ptr_offset, pool.pointer_bytes) {
        Some(0) | None => 0,
        Some(_) => u64::UNKNOWN,
    }
}

/// Move-target columns per the reference order decoder: the raw waypoint
/// position, and the pointer's target when it names another live unit; 0
/// when the pointer is null.
fn move_target_columns(pool: &Pool, record: &[u8], slot: usize) -> (u64, i16, i16) {
    let c = pool.cunit;
    let position = (
        i16_at(record, c.move_target_position_x_offset),
        i16_at(record, c.move_target_position_y_offset),
    );
    let unit = match read_pointer(record, c.move_target_unit_ptr_offset, pool.pointer_bytes) {
        Some(0) => 0,
        Some(pointer) => pool
            .slot_of(pointer)
            .filter(|&target| {
                target != slot && {
                    let target = pool.record(target);
                    target[c.owner_offset] <= 11
                        && u16_at(target, c.type_offset) < CUNIT_UNIT_TYPE_NONE
                        && i32_at(target, c.hit_points_offset) > 0
                }
            })
            .map_or(u64::UNKNOWN, |target| pool.id(target)),
        None => u64::UNKNOWN,
    };
    (unit, position.0, position.1)
}

/// Id of the turret of a tank- or goliath-like unit, per the reference
/// sub-unit relation: a live same-owner unit of the supported parent/turret
/// pair; 0 when the pointer is null.
fn sub_unit_id(pool: &Pool, record: &[u8]) -> u64 {
    let c = pool.cunit;
    match read_pointer(record, c.sub_unit_ptr_offset, pool.pointer_bytes) {
        Some(0) | None => 0,
        Some(pointer) => pool
            .slot_of(pointer)
            .filter(|&slot| {
                let target = pool.record(slot);
                is_ordinary_sub_unit_pair(u16_at(record, c.type_offset), u16_at(target, c.type_offset))
                    && target[c.owner_offset] == record[c.owner_offset]
                    && target[c.order_offset] != 0
                    && i32_at(target, c.hit_points_offset) > 0
            })
            .map_or(u64::UNKNOWN, |slot| pool.id(slot)),
    }
}

/// Id of an interceptor's carrier, per the reference hangar decoder: the
/// raw union pointer must name a live parent whose doubly-linked child
/// roster validates and includes this interceptor in the half matching its
/// in-hangar byte; 0 when the pointer is null.
fn carrier_id(pool: &Pool, record: &[u8], slot: usize) -> u64 {
    let c = pool.cunit;
    // BWAPI exposes the carrier of interceptors only; a scarab's parent
    // stays internal.
    if u16_at(record, c.type_offset) != 73 {
        return u64::UNKNOWN;
    }
    let parent_ptr = match read_pointer(record, c.hangar_in_head_ptr_offset, pool.pointer_bytes) {
        Some(0) => return 0,
        Some(pointer) => pointer,
        None => return u64::UNKNOWN,
    };
    let owner = record[c.owner_offset];
    let Some(parent_slot) = direct_slot(pool, parent_ptr) else {
        return u64::UNKNOWN;
    };
    let parent = pool.record(parent_slot);
    let parent_live = u16_at(parent, c.type_offset) == 72
        && parent[c.owner_offset] == owner
        && parent[c.order_offset] != 0
        && i32_at(parent, c.hit_points_offset) > 0;
    let in_hangar = match record[c.hangar_in_hangar_offset] {
        byte @ (0 | 1) => byte == 1,
        _ => return u64::UNKNOWN,
    };
    let mut children = HangarChildren::default();
    if !parent_live
        || !hangar_roster_valid(pool, parent_slot, owner, &mut children)
        || !children.has(slot, in_hangar)
    {
        return u64::UNKNOWN;
    }
    pool.id(parent_slot)
}

/// Id of a Nydus Canal's paired exit, per the reference relation: the union
/// slot holds the other completed canal of the same owner; 0 for other
/// types and a null pointer.
fn nydus_exit_id(pool: &Pool, record: &[u8], slot: usize) -> u64 {
    let c = pool.cunit;
    if u16_at(record, c.type_offset) != CUNIT_NYDUS_CANAL_TYPE_ID {
        return 0;
    }
    match read_pointer(record, c.resource_amount_offset, pool.pointer_bytes) {
        Some(0) | None => 0,
        Some(pointer) => pool
            .slot_of(pointer)
            .filter(|&target| {
                target != slot && {
                    let exit = pool.record(target);
                    u16_at(exit, c.type_offset) == CUNIT_NYDUS_CANAL_TYPE_ID
                        && exit[c.owner_offset] == record[c.owner_offset]
                        && i32_at(exit, c.hit_points_offset) > 0
                        && u32_at(exit, c.status_flags_offset) & CUNIT_STATUS_COMPLETED != 0
                }
            })
            .map_or(u64::UNKNOWN, |target| pool.id(target)),
    }
}

/// Id of the flag or mineral chunk a completed worker carries, per the
/// reference power-up relation; 0 when the pointer is null.
fn power_up_id(pool: &Pool, record: &[u8], status: u32) -> u64 {
    let c = pool.cunit;
    if !is_worker(u16_at(record, c.type_offset)) || !completed_live(record, c, status) {
        return u64::UNKNOWN;
    }
    match read_pointer(record, c.worker_power_up_ptr_offset, pool.pointer_bytes) {
        Some(0) | None => 0,
        Some(pointer) => direct_slot(pool, pointer)
            .filter(|&slot| {
                let target = pool.record(slot);
                u16_at(target, c.type_offset) == POWER_UP_TYPE_ID
                    && completed_live(target, c, u32_at(target, c.status_flags_offset))
            })
            .map_or(u64::UNKNOWN, |slot| pool.id(slot)),
    }
}

/// Slot of a direct record pointer: union-owned links must not accept the
/// selection-table adjustment that target pointers support.
fn direct_slot(pool: &Pool, pointer: usize) -> Option<usize> {
    cunit_record_offset_from_direct_ptr(pointer, pool.base, pool.cunit)
        .map(|offset| offset / pool.cunit.record_stride)
        .filter(|slot| *slot < pool.slots())
}

/// The record is a live completed unit (the reference decoder's
/// `is_completed_live_cunit` without its owner/type checks, which always pass
/// here).
fn completed_live(record: &[u8], c: &bwrapi_layout::CunitLayout, status: u32) -> bool {
    record[c.order_offset] != 0
        && i32_at(record, c.hit_points_offset) > 0
        && status & CUNIT_STATUS_COMPLETED != 0
}

/// Resource group of a live resource container, per the reference rule:
/// neutral fields carry the neutral owner, player gas buildings do not, and
/// the group byte stays unknown above 250.
fn resource_group(record: &[u8], c: &bwrapi_layout::CunitLayout, unit_type: u16, owner: u8) -> u8 {
    let neutral = matches!(unit_type, 176 | 177 | 178 | 188);
    if !RESOURCE_TYPES.contains(&unit_type)
        || !completed_live(record, c, u32_at(record, c.status_flags_offset))
        || (neutral != (owner == 11))
    {
        return u8::UNKNOWN;
    }
    match record[c.resource_group_offset] {
        group if group <= 250 => group,
        _ => u8::UNKNOWN,
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct HangarHeader {
    in_head: usize,
    out_head: usize,
    in_count: u8,
    out_count: u8,
}

fn hangar_header(pool: &Pool, slot: usize) -> Option<HangarHeader> {
    let c = pool.cunit;
    let record = pool.record(slot);
    Some(HangarHeader {
        in_head: read_pointer(record, c.hangar_in_head_ptr_offset, pool.pointer_bytes)?,
        out_head: read_pointer(record, c.hangar_out_head_ptr_offset, pool.pointer_bytes)?,
        in_count: record[c.hangar_in_count_offset],
        out_count: record[c.hangar_out_count_offset],
    })
}

/// Hangar counts per the reference hangar decoder: a Carrier's interceptor
/// total (in hangar plus launched) and a Reaver's scarab count, bounded the
/// same way; unknown for every other type.
fn hangar_counts(pool: &Pool, slot: usize, unit_type: u16) -> (u8, u8) {
    let header = match unit_type {
        72 | 83 => hangar_header(pool, slot),
        _ => None,
    };
    let scarabs = match (unit_type, header) {
        (83, Some(header)) if header.in_count <= 10 => header.in_count,
        _ => u8::UNKNOWN,
    };
    let interceptors = match (unit_type, header) {
        (72, Some(header))
            if header.in_count <= 8
                && header.out_count <= 8
                && u16::from(header.in_count) + u16::from(header.out_count) <= 8 =>
        {
            header.in_count + header.out_count
        }
        _ => u8::UNKNOWN,
    };
    (scarabs, interceptors)
}

/// The parent's validated child roster, allocation-free: a carrier holds at
/// most eight interceptors.
#[derive(Default)]
struct HangarChildren {
    entries: [(usize, bool); 8],
    len: usize,
}

impl HangarChildren {
    fn push(&mut self, slot: usize, in_hangar: bool) {
        self.entries[self.len] = (slot, in_hangar);
        self.len += 1;
    }

    fn has_slot(&self, slot: usize) -> bool {
        self.entries[..self.len].iter().any(|(seen, _)| *seen == slot)
    }

    fn has(&self, slot: usize, in_hangar: bool) -> bool {
        self.entries[..self.len]
            .iter()
            .any(|(seen, hangar)| *seen == slot && *hangar == in_hangar)
    }
}

/// The reference hangar roster check: both child lists must be consistent
/// doubly-linked rings or lines of live same-owner interceptors that name
/// this parent, within the eight-child carrier limit, and the header must
/// not change across the walk.
fn hangar_roster_valid(
    pool: &Pool,
    parent_slot: usize,
    owner: u8,
    children: &mut HangarChildren,
) -> bool {
    let c = pool.cunit;
    let Some(header) = hangar_header(pool, parent_slot) else { return false };
    if header.in_count > 8
        || header.out_count > 8
        || u16::from(header.in_count) + u16::from(header.out_count) > 8
    {
        return false;
    }
    let parent_address = pool.base + parent_slot * c.record_stride;
    hangar_list_valid(pool, header.in_head, header.in_count, true, parent_address, owner, children)
        && hangar_list_valid(pool, header.out_head, header.out_count, false, parent_address, owner, children)
        && hangar_header(pool, parent_slot) == Some(header)
}

/// One hangar child list: every node must be a live same-owner interceptor
/// of this parent in the expected hangar half, and the links must form a
/// consistent chain closed by a line end or the head.
fn hangar_list_valid(
    pool: &Pool,
    head: usize,
    count: u8,
    expected_in_hangar: bool,
    parent_address: usize,
    owner: u8,
    children: &mut HangarChildren,
) -> bool {
    let c = pool.cunit;
    if count == 0 {
        return head == 0;
    }
    if head == 0 {
        return false;
    }
    // (pointer, prev, next) per visited node.
    let mut links = [(0usize, 0usize, 0usize); 8];
    let mut current = head;
    for index in 0..usize::from(count) {
        let Some(slot) = direct_slot(pool, current) else { return false };
        if children.has_slot(slot) {
            return false;
        }
        let child = pool.record(slot);
        let child_live = u16_at(child, c.type_offset) == 73
            && child[c.owner_offset] == owner
            && child[c.order_offset] != 0
            && i32_at(child, c.hit_points_offset) > 0;
        let in_hangar = match child[c.hangar_in_hangar_offset] {
            byte @ (0 | 1) => byte == 1,
            _ => return false,
        };
        let (Some(parent_ptr), Some(prev), Some(next)) = (
            read_pointer(child, c.hangar_in_head_ptr_offset, pool.pointer_bytes),
            read_pointer(child, c.hangar_out_head_ptr_offset, pool.pointer_bytes),
            read_pointer(child, c.hangar_in_count_offset, pool.pointer_bytes),
        ) else {
            return false;
        };
        if !child_live || parent_ptr != parent_address || in_hangar != expected_in_hangar {
            return false;
        }
        links[index] = (current, prev, next);
        children.push(slot, in_hangar);
        current = next;
    }
    let list = &links[..usize::from(count)];
    for pair in list.windows(2) {
        if pair[1].1 != pair[0].0 || pair[0].2 != pair[1].0 {
            return false;
        }
    }
    let (first, last) = (list[0], list[usize::from(count) - 1]);
    match current {
        0 if first.1 == 0 => true,
        pointer if pointer == head && first.1 == last.0 => true,
        _ => false,
    }
}

struct Production {
    build_type: Option<u16>,
    /// False only while a morph queue holds an out-of-range type.
    build_type_known: bool,
    remaining_build_frames: Option<u32>,
    researching: bool,
    tech: Option<u8>,
    remaining_research_frames: Option<u32>,
    upgrading: bool,
    upgrade: Option<u8>,
    remaining_upgrade_frames: Option<u32>,
}

/// Build columns of one record, per the reference production decoder.
fn production(pool: &Pool, record: &[u8]) -> Production {
    let c = pool.cunit;
    let unit_type = u16_at(record, c.type_offset);
    let order = record[c.order_offset];
    let status = u32_at(record, c.status_flags_offset);
    let is_morphing = matches!(
        order,
        CUNIT_ORDER_ZERG_BIRTH
            | CUNIT_ORDER_ZERG_UNIT_MORPH
            | CUNIT_ORDER_ZERG_BUILDING_MORPH
            | CUNIT_ORDER_INCOMPLETE_MORPHING
    );
    let is_completed =
        is_bwapi_completed(status & CUNIT_STATUS_COMPLETED != 0, u16::from(order));
    let zerg_production = matches!(unit_type, 36 | 41 | 59 | 97 | 131..=144 | 146 | 149)
        || matches!(
            order,
            CUNIT_ORDER_ZERG_BIRTH
                | CUNIT_ORDER_ZERG_UNIT_MORPH
                | CUNIT_ORDER_ZERG_BUILDING_MORPH
                | CUNIT_ORDER_INCOMPLETE_MORPHING
                | CUNIT_ORDER_DRONE_LAND
        );
    let mut build_type = None;
    let mut build_type_known = true;
    if zerg_production {
        let (known_type, known) = match order {
            CUNIT_ORDER_INCOMPLETE_BUILDING | CUNIT_ORDER_BUILDING_IN_PROGRESS => {
                (Some(unit_type), true)
            }
            CUNIT_ORDER_PLACE_BUILDING
            | CUNIT_ORDER_ZERG_UNIT_MORPH
            | CUNIT_ORDER_ZERG_BUILDING_MORPH
            | CUNIT_ORDER_INCOMPLETE_MORPHING
            | CUNIT_ORDER_DRONE_LAND => {
                let active =
                    usize::from(record[c.build_queue_slot_offset]) % CUNIT_BUILD_QUEUE_SLOTS;
                let queued = u16_at(record, c.build_queue_type_offset + active * 2);
                if queued > CUNIT_UNIT_TYPE_NONE {
                    (None, false)
                } else if queued == CUNIT_UNIT_TYPE_NONE
                    && order == CUNIT_ORDER_INCOMPLETE_MORPHING
                {
                    (Some(unit_type), true)
                } else {
                    (Some(queued), true)
                }
            }
            _ => (None, true),
        };
        build_type = known_type;
        build_type_known = known;
    }
    let remaining_build_frames = if is_completed {
        Some(0)
    } else if !is_morphing
        || build_type.is_some_and(|build_type| build_type < CUNIT_UNIT_TYPE_NONE)
    {
        Some(u32::from(u16_at(record, c.build_unit_remaining_time_offset)))
    } else {
        build_type_known.then_some(0)
    };
    // The reference leaves the research/upgrade union unread for cocoons.
    let mut researching = false;
    let mut tech = None;
    let mut remaining_research_frames = None;
    let mut upgrading = false;
    let mut upgrade = None;
    let mut remaining_upgrade_frames = None;
    if !matches!(unit_type, 36 | 59 | 97) {
        let remaining = u16_at(record, c.production_remaining_time_offset);
        let tech_id = record[c.research_tech_type_offset];
        let upgrade_id = record[c.upgrade_type_offset];
        researching = order == CUNIT_ORDER_RESEARCH
            && remaining > 0
            && (CUNIT_TECH_TYPE_MIN..=CUNIT_TECH_TYPE_MAX).contains(&tech_id);
        upgrading = order == CUNIT_ORDER_UPGRADE
            && remaining > 0
            && (CUNIT_UPGRADE_TYPE_MIN..=CUNIT_UPGRADE_TYPE_MAX).contains(&upgrade_id);
        if researching {
            tech = Some(tech_id);
            remaining_research_frames = Some(u32::from(remaining));
        }
        if upgrading {
            upgrade = Some(upgrade_id);
            remaining_upgrade_frames = Some(u32::from(remaining));
        }
    }
    Production {
        build_type,
        build_type_known,
        remaining_build_frames,
        researching,
        tech,
        remaining_research_frames,
        upgrading,
        upgrade,
        remaining_upgrade_frames,
    }
}

struct Training {
    in_progress: bool,
    /// Logical queue, front first; unknown past its length.
    queue: [u16; CUNIT_BUILD_QUEUE_SLOTS],
    remaining: Option<u32>,
}

impl Training {
    /// Not training: no build unit, so no remaining time.
    fn idle() -> Self {
        Self { in_progress: false, queue: [u16::UNKNOWN; CUNIT_BUILD_QUEUE_SLOTS], remaining: Some(0) }
    }

    /// A queue the reader cannot interpret.
    fn unknown() -> Self {
        Self { remaining: None, ..Self::idle() }
    }

    /// Queue head while training is in progress.
    fn head(&self) -> Option<u16> {
        self.in_progress.then_some(self.queue[0])
    }
}

/// Training state of a standard training producer; hatchery-family producers
/// are out: their queue is larva regeneration, not training.
fn training(pool: &Pool, record: &[u8], slot: usize) -> Training {
    let c = pool.cunit;
    if !matches!(u16_at(record, c.type_offset), 106 | 111 | 113 | 114 | 130 | 154 | 155 | 160 | 167)
    {
        return Training::idle();
    }
    let active = usize::from(record[c.build_queue_slot_offset]);
    if active >= CUNIT_BUILD_QUEUE_SLOTS {
        return Training::unknown();
    }
    let mut queue = [u16::UNKNOWN; CUNIT_BUILD_QUEUE_SLOTS];
    let mut len = 0;
    for index in 0..CUNIT_BUILD_QUEUE_SLOTS {
        let queued = u16_at(
            record,
            c.build_queue_type_offset + (active + index) % CUNIT_BUILD_QUEUE_SLOTS * 2,
        );
        if queued > CUNIT_UNIT_TYPE_NONE {
            return Training::unknown();
        }
        if queued != CUNIT_UNIT_TYPE_NONE {
            queue[len] = queued;
            len += 1;
        }
    }
    let in_progress = record[c.secondary_order_offset] == CUNIT_ORDER_TRAIN && len > 0;
    let mut remaining = if in_progress { None } else { Some(0) };
    if in_progress {
        if let Some(child_slot) = live_build_child_slot(pool, record, slot) {
            let child = pool.record(child_slot);
            if u16_at(child, c.type_offset) == queue[0] {
                remaining = Some(u32::from(u16_at(child, c.build_unit_remaining_time_offset)));
            }
        }
    }
    Training { in_progress, queue, remaining }
}
