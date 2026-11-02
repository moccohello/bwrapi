use super::constants::*;
use super::helpers::{i32_at, u16_at, u32_at, usize_at};
use super::queue_rally::{
    cunit_record_offset_from_direct_ptr, cunit_record_offset_from_target_ptr,
};
use bwrapi_layout::{CunitLayout, Layout};
use bwrapi_internal_model::{
    adapted::ValidationStatus,
    facts::UnitCunitFact,
};
use std::collections::BTreeSet;

pub(super) fn decode_cunit_pattern_state(
    layout: &Layout,
    raw: &[u8],
    unit_offset: usize,
    region_base: Option<usize>,
    owner: u8,
    unit_type: u16,
    status_flags: u32,
    status: ValidationStatus,
) -> UnitCunitFact {
    let cunit = &layout.cunit;
    let sub_unit = relation_for_same_owner_live_unit(
        layout,
        raw,
        unit_offset + cunit.sub_unit_ptr_offset,
        region_base,
        owner,
        unit_type,
    );
    let resource_target = worker_resource_target(
        layout,
        raw,
        unit_offset,
        region_base,
        owner,
        unit_type,
    );
    let power_up = worker_power_up(
        layout,
        raw,
        unit_offset,
        region_base,
        owner,
        unit_type,
        status_flags,
    );
    let permanent_addon = building_permanent_addon(
        layout,
        raw,
        unit_offset,
        region_base,
        owner,
        unit_type,
    );
    let hangar = decode_hangar_state(layout, raw, unit_offset, region_base, owner, unit_type);
    let is_being_gathered = resource_is_being_gathered(
        layout, raw, unit_offset, region_base, owner, unit_type, status_flags,
    );
    let bunker_unit_tag = bunker_unit_tag(layout, raw, unit_offset, region_base, owner, status_flags);
    let nydus_exit_unit_tag = nydus_exit_unit_tag(
        layout,
        raw,
        unit_offset,
        region_base,
        owner,
        unit_type,
    );
    UnitCunitFact {
        ground_weapon_cooldown: raw
            .get(unit_offset + cunit.ground_weapon_cooldown_offset)
            .copied(),
        air_weapon_cooldown: raw
            .get(unit_offset + cunit.air_weapon_cooldown_offset)
            .copied(),
        spell_cooldown: raw
            .get(unit_offset + cunit.spell_cooldown_offset)
            .copied(),
        kill_count: raw.get(unit_offset + cunit.kill_count_offset).copied(),
        acid_spore_count: raw
            .get(unit_offset + cunit.acid_spore_count_offset)
            .copied(),
        remove_timer: None,
        defense_matrix_damage_raw: None,
        defense_matrix_timer: None,
        stim_timer: None,
        ensnare_timer: None,
        lockdown_timer: None,
        irradiate_timer: None,
        stasis_timer: None,
        plague_timer: None,
        storm_timer: None,
        maelstrom_timer: None,
        has_sub_unit: sub_unit.as_ref().map(|(_, tag)| tag.is_some()),
        sub_unit_tag: sub_unit.and_then(|(_, tag)| tag),
        has_resource_target: resource_target.as_ref().map(|(_, tag)| tag.is_some()),
        resource_target_unit_tag: resource_target.and_then(|(_, tag)| tag),
        has_power_up: power_up.as_ref().map(|(_, tag)| tag.is_some()),
        power_up_unit_tag: power_up.and_then(|(_, tag)| tag),
        is_carrying_resource: worker_carrying_resource(cunit, raw, unit_offset, unit_type),
        resource_carry_count: worker_resource_carry_count(cunit, raw, unit_offset, unit_type),
        is_gathering: is_worker(unit_type)
            .then_some(status_flags & CUNIT_STATUS_IS_GATHERING != 0),
        is_being_gathered,
        resource_group: resource_group(cunit, raw, unit_offset, owner, unit_type, status_flags),
        // BWAPI's isLoaded() includes the InBuilding bit used by Bunker
        // passengers.  The connected-unit pointer is accepted only when it
        // resolves to a live, same-owner Bunker record.
        is_in_bunker: if status_flags & CUNIT_STATUS_IN_BUILDING == 0 {
            Some(false)
        } else {
            bunker_unit_tag.as_ref().map(|_| true)
        },
        bunker_unit_tag,
        nydus_exit_unit_tag,
        is_interruptible: Some(status_flags & CUNIT_STATUS_CANNOT_RECEIVE_ORDERS == 0),
        is_invincible: Some(status_flags & CUNIT_STATUS_INVINCIBLE != 0),
        is_stuck: None,
        has_nuke: None,
        is_blind: None,
        is_parasited: None,
        has_permanent_addon: permanent_addon.as_ref().map(|(_, tag)| tag.is_some()),
        permanent_addon_unit_tag: permanent_addon.and_then(|(_, tag)| tag),
        addon_build_type_id: building_addon_build_type(cunit, raw, unit_offset, unit_type),
        interceptor_count: hangar.interceptor_count,
        scarab_count: hangar.scarab_count,
        has_hangar_parent: hangar.has_hangar_parent,
        hangar_parent_unit_tag: hangar.hangar_parent_unit_tag,
        is_in_hangar: hangar.is_in_hangar,
        hangar_children_complete: hangar.hangar_children_complete,
        resource_cargo_kind: is_worker(unit_type).then(|| raw.get(unit_offset + cunit.worker_resource_type_offset).copied().filter(|value| matches!(value, 1 | 2))).flatten(),
        is_being_healed: raw.get(unit_offset + cunit.is_being_healed_offset).map(|value| *value != 0),
        last_attacking_player_id: raw.get(unit_offset + cunit.last_attacking_player_id_offset).copied().filter(|value| *value < 12),
        spider_mine_count: matches!(unit_type, 2 | 19)
            .then(|| {
                raw.get(unit_offset + cunit.vulture_spider_mine_count_offset)
                    .copied()
                    .filter(|value| *value <= 3)
            })
            .flatten(),
        gather_queue_count: matches!(unit_type, 110 | 149 | 157 | 176 | 177 | 178 | 188)
            .then(|| is_being_gathered.and(raw.get(unit_offset + cunit.resource_gather_queue_count_offset).copied()))
            .flatten(),
        status,
        ..UnitCunitFact::default()
    }
}

/// Resolve the `connectedUnit` union for a unit inside a Terran
/// Bunker.  This is deliberately separate from ordinary transport validation:
/// an InBuilding passenger must never be treated as a Dropship/Shuttle/Overlord
/// passenger, and a stale pointer must remain unavailable rather than becoming
/// a false relation.
pub(super) fn bunker_unit_tag(
    layout: &Layout,
    raw: &[u8],
    passenger_offset: usize,
    region_base: Option<usize>,
    passenger_owner: u8,
    status_flags: u32,
) -> Option<String> {
    let cunit = &layout.cunit;
    if status_flags & CUNIT_STATUS_IN_BUILDING == 0 {
        return None;
    }
    let region_base = region_base?;
    let bunker_ptr = usize_at(raw, passenger_offset + cunit.connected_unit_ptr_offset, layout.pointer_bytes)?;
    // connectedUnit is a Target-like relation in the SCR record.  Transport
    // and bunker passengers store the selection-table pointer (record + 0x10),
    // so use the normalizer that accepts both forms.
    let bunker_offset = cunit_record_offset_from_target_ptr(bunker_ptr, region_base, cunit)?;
    if bunker_offset == passenger_offset {
        return None;
    }
    let bunker_type = u16_at(raw, bunker_offset + cunit.type_offset)?;
    let bunker_owner = *raw.get(bunker_offset + cunit.owner_offset)?;
    let bunker_hit_points = i32_at(raw, bunker_offset + cunit.hit_points_offset)?;
    let bunker_status_flags = u32_at(raw, bunker_offset + cunit.status_flags_offset)?;
    (bunker_type == CUNIT_BUNKER_TYPE_ID
        && bunker_owner == passenger_owner
        && bunker_hit_points > 0
        && bunker_status_flags & CUNIT_STATUS_COMPLETED != 0)
        .then(|| format!("cunit:record:{bunker_offset}"))
}

/// Resolve `connectedUnit` as the producer relation for a current-profile
/// Zerg Larva. Invalid non-null pointers stay unavailable.
pub(super) fn larva_hatchery_relation(
    layout: &Layout,
    raw: &[u8],
    larva_offset: usize,
    region_base: Option<usize>,
    larva_owner: u8,
    unit_type: u16,
) -> Option<(bool, Option<String>)> {
    let cunit = &layout.cunit;
    const ZERG_LARVA: u16 = 35;
    const ZERG_HATCHERY: u16 = 131;
    const ZERG_LAIR: u16 = 132;
    const ZERG_HIVE: u16 = 133;
    if unit_type != ZERG_LARVA {
        return None;
    }
    let region_base = region_base?;
    let hatchery_ptr = usize_at(raw, larva_offset + cunit.connected_unit_ptr_offset, layout.pointer_bytes)?;
    if hatchery_ptr == 0 {
        return Some((false, None));
    }
    let hatchery_offset = cunit_record_offset_from_target_ptr(hatchery_ptr, region_base, cunit)?;
    if hatchery_offset == larva_offset {
        return None;
    }
    let hatchery_type = u16_at(raw, hatchery_offset + cunit.type_offset)?;
    let hatchery_owner = *raw.get(hatchery_offset + cunit.owner_offset)?;
    let hatchery_hit_points = i32_at(raw, hatchery_offset + cunit.hit_points_offset)?;
    matches!(hatchery_type, ZERG_HATCHERY | ZERG_LAIR | ZERG_HIVE)
        .then_some(())
        .filter(|_| hatchery_owner == larva_owner && hatchery_hit_points > 0)?;
    Some((true, Some(format!("cunit:record:{hatchery_offset}"))))
}

/// Resolve the SCR union-B pointer used by a completed Zerg Nydus Canal.
/// The x64 record stores a selection-table pointer (record + 0x10); the
/// shared target normalizer also accepts a direct record pointer and keeps the
/// relation bounded to the current CUnit region.
pub(super) fn nydus_exit_unit_tag(
    layout: &Layout,
    raw: &[u8],
    unit_offset: usize,
    region_base: Option<usize>,
    owner: u8,
    unit_type: u16,
) -> Option<String> {
    let cunit = &layout.cunit;
    if unit_type != CUNIT_NYDUS_CANAL_TYPE_ID {
        return None;
    }
    let region_base = region_base?;
    let exit_ptr = usize_at(raw, unit_offset + cunit.resource_amount_offset, layout.pointer_bytes)?;
    if exit_ptr == 0 {
        return None;
    }
    let exit_offset = cunit_record_offset_from_target_ptr(exit_ptr, region_base, cunit)?;
    if exit_offset == unit_offset {
        return None;
    }
    let exit_type = u16_at(raw, exit_offset + cunit.type_offset)?;
    let exit_owner = *raw.get(exit_offset + cunit.owner_offset)?;
    let exit_hit_points = i32_at(raw, exit_offset + cunit.hit_points_offset)?;
    let exit_status_flags = u32_at(raw, exit_offset + cunit.status_flags_offset)?;
    (exit_type == CUNIT_NYDUS_CANAL_TYPE_ID
        && exit_owner == owner
        && exit_hit_points > 0
        && exit_status_flags & CUNIT_STATUS_COMPLETED != 0)
        .then(|| format!("cunit:record:{exit_offset}"))
}

#[derive(Default)]
struct HangarState {
    interceptor_count: Option<u8>,
    scarab_count: Option<u8>,
    has_hangar_parent: Option<bool>,
    hangar_parent_unit_tag: Option<String>,
    is_in_hangar: Option<bool>,
    hangar_children_complete: Option<bool>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct HangarParentHeader {
    in_head: usize,
    out_head: usize,
    in_count: u8,
    out_count: u8,
}

#[derive(Clone, Copy)]
struct HangarChildLink {
    offset: usize,
    ptr: usize,
    prev: usize,
    next: usize,
    in_hangar: bool,
}

fn decode_hangar_state(
    layout: &Layout,
    raw: &[u8],
    unit_offset: usize,
    region_base: Option<usize>,
    owner: u8,
    unit_type: u16,
) -> HangarState {
    if !is_live_hangar_unit(&layout.cunit, raw, unit_offset, owner, unit_type) {
        return HangarState::default();
    }
    match unit_type {
        72 => decode_hangar_parent(layout, raw, unit_offset, region_base, owner, unit_type),
        83 => decode_hangar_parent(layout, raw, unit_offset, region_base, owner, unit_type),
        // BWAPI exposes `getCarrier` for Interceptors only. Scarab's parent
        // remains internal even when its raw union can be structurally read.
        73 => decode_interceptor_parent(layout, raw, unit_offset, region_base, owner),
        _ => HangarState::default(),
    }
}

fn decode_hangar_parent(
    layout: &Layout,
    raw: &[u8],
    unit_offset: usize,
    region_base: Option<usize>,
    owner: u8,
    parent_type: u16,
) -> HangarState {
    let Some(header) = hangar_parent_header(layout, raw, unit_offset) else {
        return HangarState::default();
    };
    let (interceptor_count, scarab_count, hangar_children_complete) = match parent_type {
        72 => {
            let interceptor_count = carrier_interceptor_count(header);
            let complete = interceptor_count.and_then(|_| {
                region_base.map(|base| {
                    validate_hangar_children(layout, raw, unit_offset, base, owner, parent_type)
                        .is_some()
                })
            });
            (interceptor_count, None, complete)
        }
        // Scarab parent/list links remain internal. BWAPI exposes only the
        // `inHangerCount` count; no child roster is emitted.
        83 => (None, reaver_scarab_count(header), None),
        _ => return HangarState::default(),
    };
    HangarState {
        interceptor_count,
        scarab_count,
        hangar_children_complete,
        ..HangarState::default()
    }
}

fn decode_interceptor_parent(
    layout: &Layout,
    raw: &[u8],
    unit_offset: usize,
    region_base: Option<usize>,
    owner: u8,
) -> HangarState {
    let cunit = &layout.cunit;
    let Some(parent_ptr) = usize_at(raw, unit_offset + cunit.hangar_in_head_ptr_offset, layout.pointer_bytes) else {
        return HangarState::default();
    };
    if parent_ptr == 0 {
        return HangarState {
            has_hangar_parent: Some(false),
            ..HangarState::default()
        };
    }
    let Some(region_base) = region_base else {
        return HangarState::default();
    };
    let Some(parent_offset) =
        cunit_record_offset_from_direct_ptr(parent_ptr, region_base, cunit)
    else {
        return HangarState::default();
    };
    if !is_live_hangar_unit(cunit, raw, parent_offset, owner, 72) {
        return HangarState::default();
    }
    let Some(in_hangar) = hangar_bool(raw, unit_offset + cunit.hangar_in_hangar_offset) else {
        return HangarState::default();
    };
    let Some(children) = validate_hangar_children(layout, raw, parent_offset, region_base, owner, 72)
    else {
        return HangarState::default();
    };
    if !children
        .iter()
        .any(|child| child.offset == unit_offset && child.in_hangar == in_hangar)
    {
        return HangarState::default();
    }
    HangarState {
        has_hangar_parent: Some(true),
        hangar_parent_unit_tag: Some(format!("cunit:record:{parent_offset}")),
        is_in_hangar: Some(in_hangar),
        ..HangarState::default()
    }
}

fn hangar_parent_header(
    layout: &Layout,
    raw: &[u8],
    unit_offset: usize,
) -> Option<HangarParentHeader> {
    let cunit = &layout.cunit;
    Some(HangarParentHeader {
        in_head: usize_at(raw, unit_offset + cunit.hangar_in_head_ptr_offset, layout.pointer_bytes)?,
        out_head: usize_at(raw, unit_offset + cunit.hangar_out_head_ptr_offset, layout.pointer_bytes)?,
        in_count: *raw.get(unit_offset + cunit.hangar_in_count_offset)?,
        out_count: *raw.get(unit_offset + cunit.hangar_out_count_offset)?,
    })
}

fn carrier_interceptor_count(header: HangarParentHeader) -> Option<u8> {
    let total = header.in_count.checked_add(header.out_count)?;
    (header.in_count <= 8 && header.out_count <= 8 && total <= 8).then_some(total)
}

fn reaver_scarab_count(header: HangarParentHeader) -> Option<u8> {
    (header.in_count <= 10).then_some(header.in_count)
}

fn validate_hangar_children(
    layout: &Layout,
    raw: &[u8],
    parent_offset: usize,
    region_base: usize,
    owner: u8,
    parent_type: u16,
) -> Option<Vec<HangarChildLink>> {
    let initial_header = hangar_parent_header(layout, raw, parent_offset)?;
    if !hangar_header_is_list_bounded(initial_header, parent_type) {
        return None;
    }
    let parent_address = region_base.checked_add(parent_offset)?;
    let child_type = ordinary_hangar_child_type(parent_type)?;
    let mut visited_offsets = BTreeSet::new();
    let mut children = Vec::with_capacity(
        usize::from(initial_header.in_count) + usize::from(initial_header.out_count),
    );
    read_hangar_list(
        layout,
        raw,
        initial_header.in_head,
        initial_header.in_count,
        true,
        region_base,
        parent_address,
        owner,
        child_type,
        &mut visited_offsets,
        &mut children,
    )?;
    read_hangar_list(
        layout,
        raw,
        initial_header.out_head,
        initial_header.out_count,
        false,
        region_base,
        parent_address,
        owner,
        child_type,
        &mut visited_offsets,
        &mut children,
    )?;
    // The raw CUnit window is one bounded snapshot. Re-read the parent header
    // after both chains so a malformed or internally inconsistent union does
    // not become a complete roster.
    (hangar_parent_header(layout, raw, parent_offset)? == initial_header).then_some(children)
}

#[allow(clippy::too_many_arguments)]
fn read_hangar_list(
    layout: &Layout,
    raw: &[u8],
    head: usize,
    count: u8,
    expected_in_hangar: bool,
    region_base: usize,
    parent_address: usize,
    owner: u8,
    child_type: u16,
    visited_offsets: &mut BTreeSet<usize>,
    children: &mut Vec<HangarChildLink>,
) -> Option<()> {
    let cunit = &layout.cunit;
    if count == 0 {
        return (head == 0).then_some(());
    }
    if head == 0 {
        return None;
    }
    let start = children.len();
    let mut current = head;
    for _ in 0..count {
        let child_offset = cunit_record_offset_from_direct_ptr(current, region_base, cunit)?;
        if !visited_offsets.insert(child_offset)
            || !is_live_hangar_unit(cunit, raw, child_offset, owner, child_type)
        {
            return None;
        }
        let parent_ptr = usize_at(raw, child_offset + cunit.hangar_in_head_ptr_offset, layout.pointer_bytes)?;
        let prev = usize_at(raw, child_offset + cunit.hangar_out_head_ptr_offset, layout.pointer_bytes)?;
        let next = usize_at(raw, child_offset + cunit.hangar_in_count_offset, layout.pointer_bytes)?;
        let in_hangar = hangar_bool(raw, child_offset + cunit.hangar_in_hangar_offset)?;
        if parent_ptr != parent_address || in_hangar != expected_in_hangar {
            return None;
        }
        children.push(HangarChildLink {
            offset: child_offset,
            ptr: current,
            prev,
            next,
            in_hangar,
        });
        current = next;
    }
    let list = &children[start..];
    for pair in list.windows(2) {
        if pair[1].prev != pair[0].ptr || pair[0].next != pair[1].ptr {
            return None;
        }
    }
    let first = list.first()?;
    let last = list.last()?;
    match current {
        0 if first.prev == 0 => Some(()),
        ptr if ptr == head && first.prev == last.ptr => Some(()),
        _ => None,
    }
}

fn hangar_header_is_list_bounded(header: HangarParentHeader, parent_type: u16) -> bool {
    let Some(maximum) = hangar_child_limit(parent_type) else {
        return false;
    };
    header.in_count <= maximum
        && header.out_count <= maximum
        && header.in_count.checked_add(header.out_count).is_some_and(|total| total <= maximum)
}

fn ordinary_hangar_child_type(parent_type: u16) -> Option<u16> {
    match parent_type {
        72 => Some(73),
        83 => Some(85),
        _ => None,
    }
}

fn hangar_child_limit(parent_type: u16) -> Option<u8> {
    match parent_type {
        72 => Some(8),
        83 => Some(10),
        _ => None,
    }
}

fn is_live_hangar_unit(
    cunit: &CunitLayout,
    raw: &[u8],
    offset: usize,
    owner: u8,
    unit_type: u16,
) -> bool {
    raw.get(offset + cunit.owner_offset).copied() == Some(owner)
        && u16_at(raw, offset + cunit.type_offset) == Some(unit_type)
        && raw.get(offset + cunit.order_offset).copied().is_some_and(|order| order != 0)
        && i32_at(raw, offset + cunit.hit_points_offset).is_some_and(|hit_points| hit_points > 0)
}

fn hangar_bool(raw: &[u8], offset: usize) -> Option<bool> {
    match raw.get(offset).copied()? {
        0 => Some(false),
        1 => Some(true),
        _ => None,
    }
}

fn relation_for_same_owner_live_unit(
    layout: &Layout,
    raw: &[u8],
    ptr_offset: usize,
    region_base: Option<usize>,
    owner: u8,
    parent_type: u16,
) -> Option<(bool, Option<String>)> {
    let cunit = &layout.cunit;
    let ptr = usize_at(raw, ptr_offset, layout.pointer_bytes)?;
    if ptr == 0 {
        return Some((false, None));
    }
    let target_offset = cunit_record_offset_from_target_ptr(ptr, region_base?, cunit)?;
    let target_type = u16_at(raw, target_offset + cunit.type_offset)?;
    let target_owner = *raw.get(target_offset + cunit.owner_offset)?;
    let target_order = *raw.get(target_offset + cunit.order_offset)?;
    let target_hit_points = i32_at(raw, target_offset + cunit.hit_points_offset)?;
    (is_ordinary_sub_unit_pair(parent_type, target_type)
        && target_owner == owner
        && target_order != 0
        && target_hit_points > 0)
        .then(|| (true, Some(format!("cunit:record:{target_offset}"))))
}

fn worker_resource_target(
    layout: &Layout,
    raw: &[u8],
    unit_offset: usize,
    region_base: Option<usize>,
    owner: u8,
    unit_type: u16,
) -> Option<(bool, Option<String>)> {
    let cunit = &layout.cunit;
    if !is_worker(unit_type) {
        return None;
    }
    let ptr = usize_at(raw, unit_offset + cunit.worker_resource_target_ptr_offset, layout.pointer_bytes)?;
    if ptr == 0 {
        return Some((false, None));
    }
    let target_offset = cunit_record_offset_from_target_ptr(ptr, region_base?, cunit)?;
    let target_type = u16_at(raw, target_offset + cunit.type_offset)?;
    let target_owner = *raw.get(target_offset + cunit.owner_offset)?;
    let target_hit_points = i32_at(raw, target_offset + cunit.hit_points_offset)?;
    let valid = target_hit_points > 0
        && matches!(target_type, 110 | 149 | 157 | 176 | 177 | 178 | 188)
        && (!matches!(target_type, 110 | 149 | 157) || target_owner == owner);
    valid.then(|| (true, Some(format!("cunit:record:{target_offset}"))))
}

fn worker_power_up(
    layout: &Layout,
    raw: &[u8],
    unit_offset: usize,
    region_base: Option<usize>,
    owner: u8,
    unit_type: u16,
    status_flags: u32,
) -> Option<(bool, Option<String>)> {
    let cunit = &layout.cunit;
    if !is_worker(unit_type)
        || !is_completed_live_cunit(cunit, raw, unit_offset, owner, unit_type, status_flags)
    {
        return None;
    }
    let ptr = usize_at(raw, unit_offset + cunit.worker_power_up_ptr_offset, layout.pointer_bytes)?;
    if ptr == 0 {
        return Some((false, None));
    }
    let target_offset = cunit_record_offset_from_direct_ptr(ptr, region_base?, cunit)?;
    let target_owner = *raw.get(target_offset + cunit.owner_offset)?;
    let target_status_flags = u32_at(raw, target_offset + cunit.status_flags_offset)?;
    is_completed_live_cunit(
        cunit,
        raw,
        target_offset,
        target_owner,
        215,
        target_status_flags,
    )
    .then(|| (true, Some(format!("cunit:record:{target_offset}"))))
}

fn building_permanent_addon(
    layout: &Layout,
    raw: &[u8],
    unit_offset: usize,
    region_base: Option<usize>,
    owner: u8,
    parent_type: u16,
) -> Option<(bool, Option<String>)> {
    let cunit = &layout.cunit;
    if !is_ordinary_addon_parent(parent_type) {
        return None;
    }
    let ptr = usize_at(raw, unit_offset + cunit.building_addon_ptr_offset, layout.pointer_bytes)?;
    if ptr == 0 {
        return Some((false, None));
    }
    let target_offset = cunit_record_offset_from_target_ptr(ptr, region_base?, cunit)?;
    let target_type = u16_at(raw, target_offset + cunit.type_offset)?;
    let target_owner = *raw.get(target_offset + cunit.owner_offset)?;
    let target_order = *raw.get(target_offset + cunit.order_offset)?;
    let target_hit_points = i32_at(raw, target_offset + cunit.hit_points_offset)?;
    (is_ordinary_building_addon_pair(parent_type, target_type)
        && target_owner == owner
        && target_order != 0
        && target_hit_points > 0)
        .then(|| (true, Some(format!("cunit:record:{target_offset}"))))
}

fn building_addon_build_type(
    cunit: &CunitLayout,
    raw: &[u8],
    unit_offset: usize,
    parent_type: u16,
) -> Option<u16> {
    if !is_ordinary_addon_parent(parent_type)
        || raw.get(unit_offset + cunit.secondary_order_offset).copied()?
            != CUNIT_ORDER_BUILD_ADDON
    {
        return None;
    }
    let addon_type = u16_at(raw, unit_offset + cunit.building_addon_build_type_offset)?;
    is_ordinary_building_addon_pair(parent_type, addon_type).then_some(addon_type)
}

// BWAPI carries by `carryResourceType != 0` (UnitShared.cpp isCarryingGas /
// isCarryingMinerals), which is the cargo kind byte. Worker +0x132 is not a
// carry flag: it is set only while the worker holds a resource's gather slot.
fn worker_carrying_resource(
    cunit: &CunitLayout,
    raw: &[u8],
    unit_offset: usize,
    unit_type: u16,
) -> Option<bool> {
    if !is_worker(unit_type) {
        return None;
    }
    match raw.get(unit_offset + cunit.worker_resource_type_offset).copied()? {
        0 => Some(false),
        1 | 2 => Some(true),
        _ => None,
    }
}

fn worker_resource_carry_count(
    cunit: &CunitLayout,
    raw: &[u8],
    unit_offset: usize,
    unit_type: u16,
) -> Option<u8> {
    is_worker(unit_type)
        .then(|| raw.get(unit_offset + cunit.worker_resource_carry_count_offset).copied())
        .flatten()
}

fn resource_is_being_gathered(
    layout: &Layout,
    raw: &[u8],
    unit_offset: usize,
    region_base: Option<usize>,
    owner: u8,
    unit_type: u16,
    status_flags: u32,
) -> Option<bool> {
    let cunit = &layout.cunit;
    if !is_live_resource_container(cunit, raw, unit_offset, owner, unit_type, status_flags) {
        return None;
    }
    let gatherer_count = usize::from(*raw.get(unit_offset + cunit.resource_gather_queue_count_offset)?);
    let head = usize_at(raw, unit_offset + cunit.resource_next_gatherer_ptr_offset, layout.pointer_bytes)?;
    if gatherer_count == 0 && head == 0 {
        return Some(false);
    }
    if gatherer_count == 0 || head == 0 || gatherer_count > 256 {
        return None;
    }
    let region_base = region_base?;
    let resource_address = region_base.checked_add(unit_offset)?;
    let mut current = head;
    let mut first_prev = None;
    let mut previous_ptr = None;
    let mut visited_offsets = BTreeSet::new();
    for _ in 0..gatherer_count {
        let gatherer_offset = cunit_record_offset_from_direct_ptr(current, region_base, cunit)?;
        if !visited_offsets.insert(gatherer_offset) {
            return None;
        }
        let gatherer_type = u16_at(raw, gatherer_offset + cunit.type_offset)?;
        let gatherer_owner = *raw.get(gatherer_offset + cunit.owner_offset)?;
        let gatherer_status_flags = u32_at(raw, gatherer_offset + cunit.status_flags_offset)?;
        if !is_worker(gatherer_type)
            || !is_completed_live_cunit(
                cunit,
                raw,
                gatherer_offset,
                gatherer_owner,
                gatherer_type,
                gatherer_status_flags,
            )
            || !resource_gatherer_owner_is_valid(unit_type, owner, gatherer_owner)
            || usize_at(raw, gatherer_offset + cunit.resource_gatherer_target_ptr_offset, layout.pointer_bytes)?
                != resource_address
        {
            return None;
        }
        let prev = usize_at(raw, gatherer_offset + cunit.resource_gatherer_prev_ptr_offset, layout.pointer_bytes)?;
        let next = usize_at(raw, gatherer_offset + cunit.resource_gatherer_next_ptr_offset, layout.pointer_bytes)?;
        if let Some(previous_ptr) = previous_ptr {
            if prev != previous_ptr {
                return None;
            }
        } else {
            first_prev = Some(prev);
        }
        previous_ptr = Some(current);
        current = next;
    }
    let last_ptr = previous_ptr?;
    match current {
        0 if first_prev == Some(0) => Some(true),
        ptr if ptr == head && first_prev == Some(last_ptr) => Some(true),
        _ => None,
    }
}

fn resource_group(
    cunit: &CunitLayout,
    raw: &[u8],
    unit_offset: usize,
    owner: u8,
    unit_type: u16,
    status_flags: u32,
) -> Option<u8> {
    if !is_live_resource_container(cunit, raw, unit_offset, owner, unit_type, status_flags) {
        return None;
    }
    raw.get(unit_offset + cunit.resource_group_offset)
        .copied()
        .filter(|value| *value <= 250)
}

fn is_completed_live_cunit(
    cunit: &CunitLayout,
    raw: &[u8],
    offset: usize,
    owner: u8,
    unit_type: u16,
    status_flags: u32,
) -> bool {
    raw.get(offset + cunit.owner_offset).copied() == Some(owner)
        && u16_at(raw, offset + cunit.type_offset) == Some(unit_type)
        && raw.get(offset + cunit.order_offset).copied().is_some_and(|order| order != 0)
        && i32_at(raw, offset + cunit.hit_points_offset).is_some_and(|hit_points| hit_points > 0)
        && status_flags & CUNIT_STATUS_COMPLETED != 0
}

fn is_live_resource_container(
    cunit: &CunitLayout,
    raw: &[u8],
    offset: usize,
    owner: u8,
    unit_type: u16,
    status_flags: u32,
) -> bool {
    is_resource_container(unit_type)
        && is_completed_live_cunit(cunit, raw, offset, owner, unit_type, status_flags)
        && if is_neutral_resource(unit_type) {
            owner == 11
        } else {
            owner != 11
        }
}

fn resource_gatherer_owner_is_valid(resource_type: u16, resource_owner: u8, gatherer_owner: u8) -> bool {
    if is_neutral_resource(resource_type) {
        gatherer_owner != 11
    } else {
        gatherer_owner == resource_owner
    }
}

pub fn is_ordinary_sub_unit_pair(parent_type: u16, target_type: u16) -> bool {
    ordinary_sub_unit_type(parent_type) == Some(target_type)
}

pub fn ordinary_sub_unit_type(parent_type: u16) -> Option<u16> {
    // Ordinary units only; hero/UMS variants are not supported.
    match parent_type { 3 => Some(4), 5 => Some(6), 30 => Some(31), _ => None }
}

fn is_ordinary_addon_parent(unit_type: u16) -> bool {
    matches!(unit_type, 106 | 113 | 114 | 116)
}

pub fn is_ordinary_building_addon_pair(parent_type: u16, addon_type: u16) -> bool {
    matches!(
        (parent_type, addon_type),
        // Ordinary Terran buildings only; hero/campaign variants are not covered.
        (106, 107 | 108) | (113, 120) | (114, 115) | (116, 117 | 118)
    )
}

pub fn is_worker(unit_type: u16) -> bool {
    matches!(unit_type, 7 | 41 | 64)
}

fn is_resource_container(unit_type: u16) -> bool {
    matches!(unit_type, 110 | 149 | 157 | 176 | 177 | 178 | 188)
}

fn is_neutral_resource(unit_type: u16) -> bool {
    matches!(unit_type, 176 | 177 | 178 | 188)
}
