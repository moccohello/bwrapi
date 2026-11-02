use super::constants::*;
use super::helpers::{
    i16_at, i32_at, is_live_cunit_fact, is_plausible_cunit_record, u16_at, u32_at, usize_at,
};
use super::pattern::{decode_cunit_pattern_state, is_ordinary_building_addon_pair};
use super::production::{
    decode_cunit_production, protoss_unit_type_requires_power, transport_unit_tag,
    unit_type_has_energy, unit_type_has_shields,
};
use super::queue_rally::{
    active_rally_position, attach_queued_orders, cunit_record_offset_from_target_ptr,
    cunit_tag_from_target_ptr, unit_type_can_rally,
};
use bwrapi_layout::Layout;
use bwrapi_internal_model::{
    adapted::{Position, ValidationStatus},
    facts::{SourceAddress, UnitFact, UnitMotionFact, UnitOrderFact, UnitProductionFact, UnitVitalFact},
    orders,
};
use std::collections::HashMap;

pub fn iter_cunit_unit_facts(
    layout: &Layout,
    raw: &[u8],
    region_base: Option<usize>,
    owner_id: Option<u8>,
    include_stale: bool,
    status: ValidationStatus,
) -> Vec<UnitFact> {
    let cunit = &layout.cunit;
    let Some(max_offset) = raw.len().checked_sub(cunit.scan_min_bytes) else {
        return Vec::new();
    };
    let mut facts = Vec::new();
    // The supported alignments [0, stride] are the same grid, so each offset is
    // visited once.
    for offset in (0..=max_offset).step_by(cunit.record_stride) {
        let Some(fact) = decode_cunit_record(layout, raw, offset, region_base, status, false)
        else {
            continue;
        };
        if owner_id.is_some_and(|expected| fact.owner_id != Some(expected)) {
            continue;
        }
        if !include_stale && !is_live_cunit_fact(&fact) {
            continue;
        }
        facts.push(fact);
    }
    facts
}

pub fn iter_cunit_unit_facts_with_queued_orders<F, B>(
    layout: &Layout,
    raw: &[u8],
    region_base: usize,
    owner_id: Option<u8>,
    include_stale: bool,
    status: ValidationStatus,
    read_exact: F,
) -> Vec<UnitFact>
where
    F: FnMut(usize, usize) -> Option<B>,
    B: std::ops::Deref<Target = [u8]> + Clone + PartialEq,
{
    iter_cunit_unit_facts_with_queued_orders_profile(
        layout, raw, region_base, owner_id, include_stale, status, false, read_exact,
    )
}

pub fn iter_cunit_unit_facts_with_queued_orders_profile<F, B>(
    layout: &Layout,
    raw: &[u8],
    region_base: usize,
    owner_id: Option<u8>,
    include_stale: bool,
    status: ValidationStatus,
    direct_status_profile_matches: bool,
    mut read_exact: F,
) -> Vec<UnitFact>
where
    F: FnMut(usize, usize) -> Option<B>,
    B: std::ops::Deref<Target = [u8]> + Clone + PartialEq,
{
    let cunit = &layout.cunit;
    let Some(max_offset) = raw.len().checked_sub(cunit.scan_min_bytes) else {
        return Vec::new();
    };
    let mut facts = Vec::new();
    for offset in (0..=max_offset).step_by(cunit.record_stride) {
        let Some(mut fact) = decode_cunit_record(
            layout,
            raw,
            offset,
            Some(region_base),
            status,
            direct_status_profile_matches,
        ) else {
            continue;
        };
        if owner_id.is_some_and(|expected| fact.owner_id != Some(expected)) {
            continue;
        }
        if !include_stale && !is_live_cunit_fact(&fact) {
            continue;
        }
        // A failed COrder read is told apart from an empty queue; the basic
        // CUnit values are kept.
        if !attach_queued_orders(layout, &mut fact, raw, offset, region_base, status, &mut read_exact)
        {
            if let Some(order) = fact.order.as_mut() {
                order.queue_count = None;
                order.queued_orders.clear();
            }
        }
        facts.push(fact);
    }
    apply_hatchery_larva_cap(&mut facts);
    facts
}

fn decode_cunit_record(
    layout: &Layout,
    raw: &[u8],
    offset: usize,
    region_base: Option<usize>,
    status: ValidationStatus,
    direct_status_profile_matches: bool,
) -> Option<UnitFact> {
    let cunit = &layout.cunit;
    let owner = *raw.get(offset + cunit.owner_offset)?;
    let unit_type = u16_at(raw, offset + cunit.type_offset)?;
    let order = *raw.get(offset + cunit.order_offset)?;
    let movement_flags = *raw.get(offset + cunit.movement_flags_offset)?;
    let hit_points_raw = i32_at(raw, offset + cunit.hit_points_offset)?;
    let shields_raw = u32_at(raw, offset + cunit.shields_offset)?;
    let status_flags = u32_at(raw, offset + cunit.status_flags_offset)?;
    let detection_player_flags = u32_at(raw, offset + cunit.detection_player_flags_offset)?;
    let in_air = in_air_from_status_flags(status_flags, unit_type);
    let position = Position::new(
        i32::from(i16_at(raw, offset + cunit.position_x_offset)?),
        i32::from(i16_at(raw, offset + cunit.position_y_offset)?),
    );

    if owner == 0
        && unit_type == 0
        && order == 0
        && hit_points_raw == 0
        && shields_raw == 0
        && status_flags == 0
        && position == Position::default()
    {
        return None;
    }

    if !is_plausible_cunit_record(owner, unit_type, hit_points_raw, shields_raw, position) {
        return None;
    }

    let target_unit_tag = region_base.and_then(|base| {
        usize_at(raw, offset + cunit.order_target_unit_ptr_offset, layout.pointer_bytes)
            .and_then(|ptr| cunit_tag_from_target_ptr(ptr, base, cunit))
    });
    let move_target_unit_tag = region_base.and_then(|base| {
        let target_offset = usize_at(raw, offset + cunit.move_target_unit_ptr_offset, layout.pointer_bytes)
            .and_then(|pointer| cunit_record_offset_from_target_ptr(pointer, base, cunit))?;
        (target_offset != offset
            && raw.get(target_offset + cunit.owner_offset).is_some_and(|owner| *owner <= 11)
            && u16_at(raw, target_offset + cunit.type_offset).is_some_and(|unit_type| unit_type < 228)
            && i32_at(raw, target_offset + cunit.hit_points_offset).is_some_and(|hit_points| hit_points > 0))
            .then(|| format!("cunit:record:{target_offset}"))
    });
    let transport_unit_tag =
        region_base.and_then(|base| transport_unit_tag(layout, raw, offset, base, owner, status_flags));
    let is_loaded = if status_flags & (CUNIT_STATUS_IN_TRANSPORT | CUNIT_STATUS_IN_BUILDING) == 0 {
        Some(false)
    } else {
        transport_unit_tag.as_ref().map(|_| true)
    };

    let mut production = decode_cunit_production(layout, raw, offset, unit_type, order, status);
    if let Some(production) = production.as_mut() {
        production.is_powered = Some(
            !protoss_unit_type_requires_power(unit_type)
                || status_flags & CUNIT_STATUS_PROTOSS_UNPOWERED == 0,
        );
        attach_addon_state(layout, production, raw, offset, region_base, owner, unit_type);
        // The currentBuildUnit relation is independent of the training queue.
        let build_unit_offset = live_build_unit_record_offset(layout, raw, offset, region_base, owner);
        production.build_unit_tag =
            build_unit_offset.map(|child_offset| format!("cunit:record:{child_offset}"));
        attach_training_state(production, cunit, raw, offset, build_unit_offset, unit_type);
        attach_larva_regeneration_state(production, cunit, raw, offset, unit_type, status_flags);
    }

    Some(UnitFact {
        source_namespace: "cunit".to_owned(),
        source_address: region_base
            .and_then(|base| base.checked_add(offset))
            .map(SourceAddress::from),
        record_index: Some(offset),
        owner_id: Some(owner),
        unit_type_id: Some(unit_type),
        position: Some(position),
        order: Some(UnitOrderFact {
            order_id: Some(u16::from(order)),
            secondary_order_id: raw
                .get(offset + cunit.secondary_order_offset)
                .copied()
                .filter(|order_id| usize::from(*order_id) < orders::ORDER_COUNT)
                .map(u16::from),
            order_timer: Some(*raw.get(offset + cunit.order_timer_offset)?),
            target_position: Some(Position::new(
                i32::from(i16_at(raw, offset + cunit.order_target_position_x_offset)?),
                i32::from(i16_at(raw, offset + cunit.order_target_position_y_offset)?),
            )),
            target_position_available: Some(true),
            target_unit_available: match usize_at(raw, offset + cunit.order_target_unit_ptr_offset, layout.pointer_bytes) {
                Some(0) => Some(false),
                Some(_) if target_unit_tag.is_some() => Some(true),
                _ => None,
            },
            target_unit_tag,
            secondary_order_target_position: Some(Position::new(
                i32::from(i16_at(raw, offset + cunit.secondary_order_target_position_x_offset)?),
                i32::from(i16_at(raw, offset + cunit.secondary_order_target_position_y_offset)?),
            )),
            move_target_position: Some(Position::new(
                i32::from(i16_at(raw, offset + cunit.move_target_position_x_offset)?),
                i32::from(i16_at(raw, offset + cunit.move_target_position_y_offset)?),
            )),
            move_target_unit_tag,
            next_movement_waypoint: Some(Position::new(
                i32::from(i16_at(raw, offset + cunit.next_movement_waypoint_x_offset)?),
                i32::from(i16_at(raw, offset + cunit.next_movement_waypoint_y_offset)?),
            )),
            next_target_waypoint: Some(Position::new(
                i32::from(i16_at(raw, offset + cunit.next_target_waypoint_x_offset)?),
                i32::from(i16_at(raw, offset + cunit.next_target_waypoint_y_offset)?),
            )),
            rally_position: active_rally_position(
                unit_type,
                order,
                status_flags,
                i16_at(raw, offset + cunit.rally_position_x_offset)?,
                i16_at(raw, offset + cunit.rally_position_y_offset)?,
            ),
            rally_position_available: if !unit_type_can_rally(unit_type) {
                Some(false)
            } else {
                let x = i16_at(raw, offset + cunit.rally_position_x_offset)?;
                let y = i16_at(raw, offset + cunit.rally_position_y_offset)?;
                if (0..=8192).contains(&x) && (0..=8192).contains(&y) {
                    Some(active_rally_position(unit_type, order, status_flags, x, y).is_some())
                } else { None }
            },
            rally_unit_available: if !unit_type_can_rally(unit_type) { Some(false) } else {
                match usize_at(raw, offset + cunit.rally_unit_ptr_offset, layout.pointer_bytes) {
                    Some(0) => Some(false),
                    Some(ptr) => region_base
                        .and_then(|base| cunit_tag_from_target_ptr(ptr, base, cunit))
                        .map(|_| true),
                    None => None,
                }
            },
            rally_unit_tag: region_base
                .filter(|_| unit_type_can_rally(unit_type))
                .and_then(|base| {
                    usize_at(raw, offset + cunit.rally_unit_ptr_offset, layout.pointer_bytes)
                        .and_then(|ptr| cunit_tag_from_target_ptr(ptr, base, cunit))
                }),
            queue_count: Some(*raw.get(offset + cunit.order_queue_count_offset)?),
            queued_orders: Vec::new(),
            status,
        }),
        vitals: Some(UnitVitalFact {
            shields: if unit_type_has_shields(unit_type) {
                None
            } else {
                Some(0)
            },
            hit_points_raw: Some(hit_points_raw),
            shields_raw: Some(shields_raw as i32),
            energy_raw: unit_type_has_energy(unit_type)
                .then(|| u16_at(raw, offset + cunit.energy_raw_offset).map(i32::from))
                .flatten(),
            status,
        }),
        production,
        transport_unit_tag,
        is_loaded,
        cunit: Some({            let mut pattern_state = decode_cunit_pattern_state(
            layout,
            raw,
            offset,
            region_base,
            owner,
            unit_type,
            status_flags,
            status,
            );
            pattern_state.is_stuck = direct_status_profile_matches
                .then(|| {
                    raw.get(offset + cunit.movement_state_offset)
                        .map(|state| *state == CUNIT_MOVEMENT_STATE_MOVE_TO_LEGAL)
                })
                .flatten();
            pattern_state.has_nuke = direct_status_profile_matches
                .then(|| {
                    if unit_type == CUNIT_NUCLEAR_SILO_TYPE_ID {
                        raw.get(offset + cunit.silo_nuke_ready_offset)
                            .map(|ready| *ready != 0)
                    } else {
                        Some(false)
                    }
                })
                .flatten();
            if direct_status_profile_matches {
                let hatchery = super::pattern::larva_hatchery_relation(
                    layout,
                    raw,
                    offset,
                    region_base,
                    owner,
                    unit_type,
                );
                pattern_state.has_hatchery_parent = hatchery.as_ref().map(|(has_parent, _)| *has_parent);
                pattern_state.hatchery_unit_tag = hatchery.and_then(|(_, tag)| tag);
                // The closed status tail is read as one profile-gated block.
                // Keep the fixed-point damage raw; the public projection owns
                // the display-unit conversion.
                pattern_state.remove_timer = u16_at(raw, offset + cunit.remove_timer_offset);
                pattern_state.defense_matrix_damage_raw =
                    u16_at(raw, offset + cunit.defense_matrix_damage_offset);
                pattern_state.defense_matrix_timer =
                    raw.get(offset + cunit.defense_matrix_timer_offset).copied();
                pattern_state.stim_timer = raw.get(offset + cunit.stim_timer_offset).copied();
                pattern_state.ensnare_timer = raw.get(offset + cunit.ensnare_timer_offset).copied();
                pattern_state.lockdown_timer = raw.get(offset + cunit.lockdown_timer_offset).copied();
                pattern_state.irradiate_timer = raw.get(offset + cunit.irradiate_timer_offset).copied();
                pattern_state.stasis_timer = raw.get(offset + cunit.stasis_timer_offset).copied();
                pattern_state.plague_timer = raw.get(offset + cunit.plague_timer_offset).copied();
                pattern_state.storm_timer = raw.get(offset + cunit.storm_timer_offset).copied();
                pattern_state.maelstrom_timer = raw.get(offset + cunit.maelstrom_timer_offset).copied();
                pattern_state.is_parasited = raw
                    .get(offset + cunit.parasite_flags_offset)
                    .map(|value| *value != 0);
                pattern_state.is_blind = raw
                    .get(offset + cunit.is_blind_offset)
                    .map(|value| *value != 0);
                pattern_state.order_queue_timer = raw.get(offset + cunit.order_queue_timer_offset).copied();
                pattern_state.attack_notify_timer = raw.get(offset + cunit.attack_notify_timer_offset).copied();
                pattern_state.previous_unit_type_id = u16_at(raw, offset + cunit.previous_unit_type_offset);
                pattern_state.last_event_timer = raw.get(offset + cunit.last_event_timer_offset).copied();
                pattern_state.last_event_color = raw.get(offset + cunit.last_event_color_offset).copied();
                pattern_state.rank_increase = raw.get(offset + cunit.rank_increase_offset).copied();
                pattern_state.secondary_order_timer = raw.get(offset + cunit.secondary_order_timer_offset).copied();
                pattern_state.ai_action_flags = raw.get(offset + cunit.ai_action_flags_offset).copied();
                pattern_state.user_action_flags = raw.get(offset + cunit.user_action_flags_offset).copied();
                pattern_state.button_set_id = u16_at(raw, offset + cunit.button_set_id_offset);
                pattern_state.raw_is_cloaked = raw.get(offset + cunit.raw_is_cloaked_offset).map(|value| *value != 0);
                pattern_state.movement_state = raw.get(offset + cunit.movement_state_offset).copied();
                pattern_state.recently_attacked = pattern_state
                    .last_event_color
                    .zip(pattern_state.last_event_timer)
                    .map(|(color, timer)| color == 174 && timer != 0);
                pattern_state.building_overlay_state = raw.get(offset + cunit.building_overlay_state_offset).copied();
                pattern_state.hit_points_gain_raw = u16_at(raw, offset + cunit.hit_points_gain_raw_offset);
                pattern_state.shield_gain_raw = u16_at(raw, offset + cunit.shield_gain_raw_offset);
                pattern_state.previous_hit_points_raw = u16_at(raw, offset + cunit.previous_hit_points_raw_offset);
            }
            pattern_state
        }),
        in_air: Some(in_air),
        is_burrowed: Some(status_flags & CUNIT_STATUS_BURROWED != 0),
        is_cloaked: Some(
            status_flags & CUNIT_STATUS_CLOAKED != 0
                && status_flags & CUNIT_STATUS_BURROWED == 0,
        ),
        requires_detection: Some(status_flags & CUNIT_STATUS_REQUIRES_DETECTION != 0),
        detection_player_flags: Some(detection_player_flags),
        // The current CUnit record can only establish the flag portion of
        // BWAPI's temporal isMoving predicate. Runtime publication applies
        // the previous public order before advancing that per-unit state.
        motion: Some(UnitMotionFact {
            is_moving: Some(
                movement_flags & (CUNIT_MOVEMENT_FLAG_MOVING | CUNIT_MOVEMENT_FLAG_ACCELERATING)
                    != 0,
            ),
            is_accelerating: Some(movement_flags & CUNIT_MOVEMENT_FLAG_ACCELERATING != 0),
            is_braking: Some(movement_flags & CUNIT_MOVEMENT_FLAG_BRAKING != 0),
            facing_direction_raw: raw.get(offset + cunit.current_direction_1_offset).copied(),
            turn_radius_raw: raw.get(offset + cunit.turn_radius_offset).copied(),
            velocity_direction_raw: raw.get(offset + cunit.velocity_direction_offset).copied(),
            velocity_x_raw: i32_at(raw, offset + cunit.velocity_x_offset),
            velocity_y_raw: i32_at(raw, offset + cunit.velocity_y_offset),
            flingy_id: u16_at(raw, offset + cunit.flingy_id_offset),
            flingy_movement_type: raw.get(offset + cunit.flingy_movement_type_offset).copied(),
            halt_position_raw: i32_at(raw, offset + cunit.halt_position_x_offset).zip(i32_at(raw, offset + cunit.halt_position_y_offset)).map(|(x, y)| Position::new(x, y)),
            flingy_top_speed_raw: u32_at(raw, offset + cunit.flingy_top_speed_raw_offset),
            flingy_current_speed1_raw: i32_at(raw, offset + cunit.flingy_current_speed1_raw_offset),
            flingy_current_speed2_raw: i32_at(raw, offset + cunit.flingy_current_speed2_raw_offset),
            flingy_acceleration: u16_at(raw, offset + cunit.flingy_acceleration_offset),
            current_direction2_raw: raw.get(offset + cunit.current_direction2_raw_offset).copied(),
            velocity_direction2_raw: raw.get(offset + cunit.velocity_direction2_raw_offset).copied(),
        }),
        is_hallucination: Some(status_flags & CUNIT_STATUS_HALLUCINATION != 0),
        status,
        ..UnitFact::default()
    })
}

fn in_air_from_status_flags(status_flags: u32, unit_type: u16) -> bool {
    if is_known_building_type(unit_type) && !is_lift_capable_terran_building(unit_type) {
        return false;
    }
    status_flags & CUNIT_STATUS_IN_AIR != 0
}

fn is_known_building_type(unit_type: u16) -> bool {
    matches!(unit_type, 106..=127 | 130..=201 | 214)
}

fn is_lift_capable_terran_building(unit_type: u16) -> bool {
    matches!(unit_type, 106 | 111 | 113 | 114 | 116 | 122)
}

fn attach_addon_state(
    layout: &Layout,
    production: &mut UnitProductionFact,
    raw: &[u8],
    producer_offset: usize,
    region_base: Option<usize>,
    producer_owner: u8,
    producer_type: u16,
) {
    let cunit = &layout.cunit;
    // The common child pointer is accepted only for explicitly paired addons.
    let Some(region_base) = region_base else {
        return;
    };
    let Some(child_offset) = usize_at(raw, producer_offset + cunit.build_unit_ptr_offset, layout.pointer_bytes)
        .and_then(|ptr| cunit_record_offset_from_target_ptr(ptr, region_base, cunit))
    else {
        return;
    };
    let Some(child_type) = u16_at(raw, child_offset + cunit.type_offset) else {
        return;
    };
    let Some(child_owner) = raw.get(child_offset + cunit.owner_offset).copied() else {
        return;
    };
    if is_ordinary_building_addon_pair(producer_type, child_type)
        && child_owner == producer_owner
    {
        production.addon_unit_tag = Some(format!("cunit:record:{child_offset}"));
    }
}

fn live_build_unit_record_offset(
    layout: &Layout,
    raw: &[u8],
    producer_offset: usize,
    region_base: Option<usize>,
    producer_owner: u8,
) -> Option<usize> {
    let cunit = &layout.cunit;
    let child_offset = usize_at(raw, producer_offset + cunit.build_unit_ptr_offset, layout.pointer_bytes)
        .and_then(|ptr| cunit_record_offset_from_target_ptr(ptr, region_base?, cunit))?;
    if producer_offset / cunit.record_stride >= CUNIT_RECORD_COUNT
        || child_offset / cunit.record_stride >= CUNIT_RECORD_COUNT
        || child_offset == producer_offset
    {
        return None;
    }
    raw.get(child_offset..child_offset.checked_add(cunit.scan_min_bytes)?)?;
    let child_owner = *raw.get(child_offset + cunit.owner_offset)?;
    let child_type = u16_at(raw, child_offset + cunit.type_offset)?;
    let child_order = *raw.get(child_offset + cunit.order_offset)?;
    let child_hit_points = i32_at(raw, child_offset + cunit.hit_points_offset)?;
    let child_shields = u32_at(raw, child_offset + cunit.shields_offset)?;
    let child_position = Position::new(
        i32::from(i16_at(raw, child_offset + cunit.position_x_offset)?),
        i32::from(i16_at(raw, child_offset + cunit.position_y_offset)?),
    );
    (child_owner == producer_owner
        && child_type < CUNIT_UNIT_TYPE_NONE
        && child_order != 0
        && child_hit_points > 0
        && is_plausible_cunit_record(
            child_owner,
            child_type,
            child_hit_points,
            child_shields,
            child_position,
        ))
    .then_some(child_offset)
}

fn attach_training_state(
    production: &mut UnitProductionFact,
    cunit: &bwrapi_layout::CunitLayout,
    raw: &[u8],
    producer_offset: usize,
    build_unit_offset: Option<usize>,
    producer_type: u16,
) {
    // The confirmed CUnit queue layout is shared by standard buildings that
    // train units. Keep this explicit: research-only buildings, addons, and
    // other building unions must not be decoded as a training queue.
    if !is_common_training_producer(producer_type) {
        return;
    }
    let Some(active_slot) = raw
        .get(producer_offset + cunit.build_queue_slot_offset)
        .copied()
        .map(usize::from)
        .filter(|slot| *slot < CUNIT_BUILD_QUEUE_SLOTS)
    else {
        return;
    };
    let Some(queue) = (0..CUNIT_BUILD_QUEUE_SLOTS)
        .map(|index| {
            let slot = (active_slot + index) % CUNIT_BUILD_QUEUE_SLOTS;
            u16_at(
                raw,
                producer_offset + cunit.build_queue_type_offset + slot * 2,
            )
        })
        .collect::<Option<Vec<_>>>()
        .filter(|queue| {
            queue
                .iter()
                .all(|unit_type| *unit_type <= CUNIT_UNIT_TYPE_NONE)
        })
    else {
        return;
    };
    let logical_queue: Vec<u16> = queue
        .into_iter()
        .filter(|unit_type| *unit_type != CUNIT_UNIT_TYPE_NONE)
        .collect();
    let secondary_order = raw
        .get(producer_offset + cunit.secondary_order_offset)
        .copied();
    let is_training = secondary_order == Some(CUNIT_ORDER_TRAIN) && !logical_queue.is_empty();
    production.training_queue = Some(logical_queue.clone());
    if !is_training {
        return;
    }
    let Some(child_offset) = build_unit_offset else {
        return;
    };
    let Some(child_type) = u16_at(raw, child_offset + cunit.type_offset) else {
        return;
    };
    if logical_queue.first().copied() != Some(child_type) {
        return;
    }
    production.remaining_train_frames =
        u16_at(raw, child_offset + cunit.build_unit_remaining_time_offset).map(u32::from);
}

fn is_common_training_producer(unit_type: u16) -> bool {
    // Hatchery/Lair/Hive deliberately stay out of this list: their build
    // queue is not a training queue and BWAPI reports isTraining = false for
    // Zerg resource depots. Larva regeneration is published separately through
    // attach_larva_regeneration_state below.
    matches!(
        unit_type,
        // Terran Command Center, Barracks, Factory, Starport.
        106 | 111 | 113 | 114 |
        // Zerg Infested Command Center.
        130 |
        // Protoss Nexus, Robotics Facility, Gateway, Stargate.
        154 | 155 | 160 | 167
    )
}

/// Zerg Hatchery/Lair/Hive expose their larva-regeneration countdown through
/// BWAPI's getRemainingTrainTime even though isTraining stays false for Zerg
/// resource depots:
///
///   remainingTrainTime = larvaTimer * 9 + ((orderQueueTimer + 8) % 9)
///
/// The larva timer counts 9-frame ticks, not frames. The build time is a
/// separate production field: BWAPI resets buildType/remainingBuildTime before
/// this switch, so an unfinished Hatchery's train time never becomes a build
/// countdown. The formula is published for a raw Completed depot or a
/// confirmed Lair/Hive morph.
fn attach_larva_regeneration_state(
    production: &mut UnitProductionFact,
    cunit: &bwrapi_layout::CunitLayout,
    raw: &[u8],
    offset: usize,
    unit_type: u16,
    status_flags: u32,
) {
    if !CUNIT_ZERG_HATCHERY_FAMILY_TYPE_IDS.contains(&unit_type) {
        return;
    }
    // A completed depot keeps regenerating larvae while upgrading to Lair/Hive,
    // even when SCR clears the raw Completed bit. Require the morph order and
    // its active target; a newly constructed Hatchery must stay unavailable.
    let depot_morph = raw
        .get(offset + cunit.order_offset)
        .is_some_and(|order| {
            matches!(
                *order,
                CUNIT_ORDER_ZERG_BUILDING_MORPH | CUNIT_ORDER_INCOMPLETE_MORPHING
            )
        })
        && raw
            .get(offset + cunit.build_queue_slot_offset)
            .copied()
            .map(usize::from)
            .filter(|slot| *slot < CUNIT_BUILD_QUEUE_SLOTS)
            .and_then(|slot| {
                u16_at(raw, offset + cunit.build_queue_type_offset + slot * 2)
            })
            .is_some_and(|target| matches!((unit_type, target), (131, 132) | (132, 133)));
    if status_flags & CUNIT_STATUS_COMPLETED == 0 && !depot_morph {
        return;
    }
    let Some(larva_timer) = raw.get(offset + cunit.larva_timer_offset).copied() else {
        return;
    };
    let Some(order_queue_timer) = raw
        .get(offset + cunit.order_queue_timer_offset)
        .copied()
    else {
        return;
    };
    production.remaining_train_frames =
        Some(u32::from(larva_timer) * 9 + ((u32::from(order_queue_timer) + 8) % 9));
}

fn fact_record_tag(fact: &UnitFact) -> Option<String> {
    Some(format!("{}:record:{}", fact.source_namespace, fact.record_index?))
}

/// BWAPI clears a hatchery's larva countdown once three larva are connected,
/// because regeneration stops while the hatchery is full. Apply the same cap
/// after the per-record scan using the validated larva->hatchery relation
/// tags. That relation is only decoded under the direct-status profile, so
/// without the profile the formula value stands unchanged.
fn apply_hatchery_larva_cap(facts: &mut [UnitFact]) {
    let mut larva_counts: HashMap<String, u32> = HashMap::new();
    for fact in facts.iter() {
        if fact.unit_type_id != Some(CUNIT_ZERG_LARVA_TYPE_ID) {
            continue;
        }
        let Some(tag) = fact
            .cunit
            .as_ref()
            .and_then(|cunit| cunit.hatchery_unit_tag.as_deref())
        else {
            continue;
        };
        *larva_counts.entry(tag.to_owned()).or_default() += 1;
    }
    if larva_counts.is_empty() {
        return;
    }
    for fact in facts.iter_mut() {
        if !fact
            .unit_type_id
            .is_some_and(|unit_type| CUNIT_ZERG_HATCHERY_FAMILY_TYPE_IDS.contains(&unit_type))
        {
            continue;
        }
        // The cap applies only when the larva formula was published. The
        // public completed flag can be false during a morph, and a separate
        // build countdown must not be cleared by the cap.
        let cap_applies = fact
            .production
            .as_ref()
            .is_some_and(|production| production.remaining_train_frames.is_some())
            && fact_record_tag(fact)
                .and_then(|tag| larva_counts.get(&tag).copied())
                .is_some_and(|count| count >= 3);
        if cap_applies {
            if let Some(production) = fact.production.as_mut() {
                production.remaining_train_frames = Some(0);
            }
        }
    }
}
