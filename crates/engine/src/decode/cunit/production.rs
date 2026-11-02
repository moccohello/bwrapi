use super::constants::*;
use super::helpers::{i32_at, is_plausible_cunit_record, u16_at, u32_at, usize_at};
use super::queue_rally::cunit_record_offset_from_target_ptr;
use bwrapi_internal_model::{
    adapted::{Position, ValidationStatus},
    facts::{TechFact, UnitFact, UnitProductionFact, UpgradeFact},
    unit_type_metadata::max_energy_raw,
    orders::is_bwapi_completed,
};

/// Plain-value view of the internal record plausibility gate, for readers
/// outside this module.
pub fn plausible_cunit_record(
    owner: u8,
    unit_type: u16,
    hit_points_raw: i32,
    shields_raw: u32,
    x: i32,
    y: i32,
) -> bool {
    is_plausible_cunit_record(owner, unit_type, hit_points_raw, shields_raw, Position::new(x, y))
}

pub fn derive_cunit_in_progress_facts(units: &[UnitFact]) -> (Vec<UpgradeFact>, Vec<TechFact>) {
    let mut upgrades: Vec<UpgradeFact> = Vec::new();
    let mut techs: Vec<TechFact> = Vec::new();

    for unit in units {
        let Some(production) = &unit.production else {
            continue;
        };
        if production.tech_type_id.is_some() {
            if let (Some(player_id), Some(tech_type_id)) = (unit.owner_id, production.tech_type_id)
            {
                prefer_shorter_tech(
                    &mut techs,
                    TechFact {
                        player_id,
                        tech_type_id,
                        researched: None,
                        available: None,
                        in_progress: Some(true),
                        remaining_frames: production.remaining_research_frames,
                        status: production.status,
                    },
                );
            }
        }
        if production.upgrade_type_id.is_some() {
            if let (Some(player_id), Some(upgrade_type_id)) =
                (unit.owner_id, production.upgrade_type_id)
            {
                prefer_shorter_upgrade(
                    &mut upgrades,
                    UpgradeFact {
                        player_id,
                        upgrade_type_id,
                        level: None,
                        max_level: None,
                        in_progress: Some(true),
                        remaining_frames: production.remaining_upgrade_frames,
                        status: production.status,
                    },
                );
            }
        }
    }

    (upgrades, techs)
}

pub(super) fn decode_cunit_production(
    layout: &bwrapi_layout::Layout,
    raw: &[u8],
    offset: usize,
    unit_type: u16,
    order: u8,
    status: ValidationStatus,
) -> Option<UnitProductionFact> {
    let cunit = &layout.cunit;
    let is_building = is_building_type(unit_type);
    let status_flags = u32_at(raw, offset + cunit.status_flags_offset)?;
    // BWAPI's UnitUpdate reports raw Completed as false while a morph order runs.
    let is_morphing = matches!(
        order,
        CUNIT_ORDER_ZERG_BIRTH
            | CUNIT_ORDER_ZERG_UNIT_MORPH
            | CUNIT_ORDER_ZERG_BUILDING_MORPH
            | CUNIT_ORDER_INCOMPLETE_MORPHING
    );
    let is_completed = Some(is_bwapi_completed(
        status_flags & CUNIT_STATUS_COMPLETED != 0,
        u16::from(order),
    ));
    let mut production = UnitProductionFact {
        is_completed,
        status,
        ..UnitProductionFact::default()
    };
    let zerg_production = matches!(unit_type, 36 | 41 | 59 | 97 | 131..=144 | 146 | 149)
        || matches!(
            order,
            CUNIT_ORDER_ZERG_BIRTH
                | CUNIT_ORDER_ZERG_UNIT_MORPH
                | CUNIT_ORDER_ZERG_BUILDING_MORPH
                | CUNIT_ORDER_INCOMPLETE_MORPHING
                | CUNIT_ORDER_DRONE_LAND
        );
    let mut build_type_known = true;
    if zerg_production {
        // The type comes only from getBuildType's order switch and the current
        // queue slot.
        let (build_type_id, known) = match order {
            CUNIT_ORDER_INCOMPLETE_BUILDING | CUNIT_ORDER_BUILDING_IN_PROGRESS => {
                (Some(unit_type), true)
            }
            CUNIT_ORDER_PLACE_BUILDING
            | CUNIT_ORDER_ZERG_UNIT_MORPH
            | CUNIT_ORDER_ZERG_BUILDING_MORPH
            | CUNIT_ORDER_INCOMPLETE_MORPHING
            | CUNIT_ORDER_DRONE_LAND => {
                let queued_type = raw
                    .get(offset + cunit.build_queue_slot_offset)
                    .and_then(|slot| {
                        let active_slot = usize::from(*slot) % CUNIT_BUILD_QUEUE_SLOTS;
                        u16_at(raw, offset + cunit.build_queue_type_offset + active_slot * 2)
                    })
                    .filter(|queued_type| *queued_type <= CUNIT_UNIT_TYPE_NONE);
                match queued_type {
                    Some(CUNIT_UNIT_TYPE_NONE) if order == CUNIT_ORDER_INCOMPLETE_MORPHING => {
                        (Some(unit_type), true)
                    }
                    Some(CUNIT_UNIT_TYPE_NONE) => (Some(CUNIT_UNIT_TYPE_NONE), true),
                    Some(queued_type) => (Some(queued_type), true),
                    None => (None, false),
                }
            }
            _ => (None, true),
        };
        production.build_type_id = build_type_id;
        build_type_known = known;
    }
    // UnitUpdate's current build time does not depend on the race. Only a morph
    // needs getBuildType's current-queue proof, and a raw 0 is a known value.
    production.remaining_build_frames = if is_completed == Some(true) {
        Some(0)
    } else if !is_morphing
        || production.build_type_id.is_some_and(|build_type| build_type < CUNIT_UNIT_TYPE_NONE)
    {
        u16_at(raw, offset + cunit.build_unit_remaining_time_offset).map(u32::from)
    } else {
        build_type_known.then_some(0)
    };
    if matches!(unit_type, 36 | 59 | 97) {
        return Some(production);
    }
    let remaining_frames = u16_at(raw, offset + cunit.production_remaining_time_offset)?;
    let tech_type_id = *raw.get(offset + cunit.research_tech_type_offset)?;
    let upgrade_type_id = *raw.get(offset + cunit.upgrade_type_offset)?;
    if order == CUNIT_ORDER_RESEARCH && remaining_frames > 0 && valid_tech_type_id(tech_type_id) {
        return Some(UnitProductionFact {
            is_completed,
            tech_type_id: Some(u16::from(tech_type_id)),
            remaining_research_frames: Some(u32::from(remaining_frames)),
            status,
            ..production
        });
    }
    if order == CUNIT_ORDER_UPGRADE
        && remaining_frames > 0
        && valid_upgrade_type_id(upgrade_type_id)
    {
        return Some(UnitProductionFact {
            is_completed,
            upgrade_type_id: Some(u16::from(upgrade_type_id)),
            remaining_upgrade_frames: Some(u32::from(remaining_frames)),
            status,
            ..production
        });
    }
    if is_building {
        // The building union at +0x122 is upgradeResearchTime, not construction
        // progress, and can remain non-zero after the producer returns idle.
        // Completion comes from raw Completed and the morph-order rule.
        return Some(UnitProductionFact {
            is_completed,
            landing_timer: raw.get(offset + cunit.building_landing_timer_offset).copied(),
            upgrade_level: raw.get(offset + cunit.building_upgrade_level_offset).copied(),
            status,
            ..production
        });
    }
    Some(production)
}

fn is_building_type(unit_type: u16) -> bool {
    matches!(
        unit_type,
        106..=130 | 131..=144 | 146 | 149 | 154..=157 | 159 | 160 | 162..=167 | 169..=172
    )
}

pub fn protoss_unit_type_requires_power(unit_type: u16) -> bool {
    matches!(
        unit_type,
        155 | 159 | 160 | 162..=167 | 169..=172
    )
}

pub fn unit_type_has_energy(unit_type: u16) -> bool {
    max_energy_raw(unit_type).is_some_and(|energy| energy > 0)
}

pub(super) fn transport_unit_tag(
    layout: &bwrapi_layout::Layout,
    raw: &[u8],
    passenger_offset: usize,
    region_base: usize,
    passenger_owner: u8,
    status_flags: u32,
) -> Option<String> {
    let cunit = &layout.cunit;
    let in_transport = status_flags & CUNIT_STATUS_IN_TRANSPORT != 0;
    let in_building = status_flags & CUNIT_STATUS_IN_BUILDING != 0;
    if !in_transport && !in_building {
        return None;
    }
    // SCR sets both bits for a unit stored in a Bunker.  InBuilding is the
    // stronger discriminator; only the remaining branch represents an air
    // transport (Dropship/Shuttle/Overlord/Arbiter).
    // SCR stores connectedUnit as the selection-table pointer (record + 0x10)
    // for air-transport passengers; accept both that form and a direct record
    // pointer through the shared target-pointer normalizer.
    let transport_offset = usize_at(raw, passenger_offset + cunit.connected_unit_ptr_offset, layout.pointer_bytes)
        .and_then(|ptr| cunit_record_offset_from_target_ptr(ptr, region_base, cunit))?;
    let transport_type = u16_at(raw, transport_offset + cunit.type_offset)?;
    let transport_owner = *raw.get(transport_offset + cunit.owner_offset)?;
    let transport_hit_points = i32_at(raw, transport_offset + cunit.hit_points_offset)?;
    let valid_parent_type = if in_building {
        transport_type == CUNIT_BUNKER_TYPE_ID
    } else {
        matches!(transport_type, 11 | 42 | 57 | 69)
    };
    let transport_order = *raw.get(transport_offset + cunit.order_offset)?;
    let transport_status_flags = u32_at(raw, transport_offset + cunit.status_flags_offset)?;
    if !valid_parent_type
        || transport_owner != passenger_owner
        || transport_hit_points <= 0
        || transport_status_flags & CUNIT_STATUS_COMPLETED == 0
        || (!in_building && transport_order == 0)
    {
        return None;
    }
    Some(format!("cunit:record:{transport_offset}"))
}

fn valid_tech_type_id(type_id: u8) -> bool {
    (CUNIT_TECH_TYPE_MIN..=CUNIT_TECH_TYPE_MAX).contains(&type_id)
}

fn valid_upgrade_type_id(type_id: u8) -> bool {
    (CUNIT_UPGRADE_TYPE_MIN..=CUNIT_UPGRADE_TYPE_MAX).contains(&type_id)
}

pub fn unit_type_has_shields(unit_type: u16) -> bool {
    // BWAPI 4.4 UnitType.cpp defaultMaxSP: include hero units and exclude
    // Devourer (62) and the unused building slot (168), whose maxima are zero.
    matches!(unit_type, 60..=61 | 63..=88 | 98 | 154..=167 | 169..=172)
}

#[cfg(test)]
mod shield_tests {
    use super::unit_type_has_shields;

    #[test]
    fn shields_match_official_unit_and_building_boundaries() {
        for id in [60, 61, 63, 83, 84, 85, 86, 87, 88, 98, 154, 167, 169, 172] {
            assert!(unit_type_has_shields(id), "shield-bearing type {id}");
        }
        for id in [59, 62, 89, 97, 99, 153, 168, 173, 228] {
            assert!(!unit_type_has_shields(id), "shieldless type {id}");
        }
    }
}

fn prefer_shorter_upgrade(upgrades: &mut Vec<UpgradeFact>, candidate: UpgradeFact) {
    if let Some(existing) = upgrades.iter_mut().find(|existing| {
        existing.player_id == candidate.player_id
            && existing.upgrade_type_id == candidate.upgrade_type_id
    }) {
        if shorter(candidate.remaining_frames, existing.remaining_frames) {
            *existing = candidate;
        }
    } else {
        upgrades.push(candidate);
    }
}

fn prefer_shorter_tech(techs: &mut Vec<TechFact>, candidate: TechFact) {
    if let Some(existing) = techs.iter_mut().find(|existing| {
        existing.player_id == candidate.player_id && existing.tech_type_id == candidate.tech_type_id
    }) {
        if shorter(candidate.remaining_frames, existing.remaining_frames) {
            *existing = candidate;
        }
    } else {
        techs.push(candidate);
    }
}

fn shorter(candidate: Option<u32>, existing: Option<u32>) -> bool {
    match (candidate, existing) {
        (Some(candidate), Some(existing)) => candidate < existing,
        (Some(_), None) => true,
        _ => false,
    }
}
