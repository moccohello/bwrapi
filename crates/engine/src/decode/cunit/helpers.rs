use super::constants::*;
use bwrapi_internal_model::{
    adapted::Position,
    facts::UnitFact,
};

pub(super) fn is_plausible_cunit_record(
    owner: u8,
    unit_type: u16,
    hit_points_raw: i32,
    shields_raw: u32,
    position: Position,
) -> bool {
    if owner > 11 {
        return false;
    }
    // Unit type 0 is Terran Marine, not an empty-record sentinel. Ownership-
    // specific liveness and positive hit points are checked separately below.
    if unit_type > 240 {
        return false;
    }
    let max_hit_points_raw = if is_neutral_resource_type(Some(unit_type)) {
        CUNIT_MAX_MINERAL_HIT_POINTS_RAW
    } else {
        CUNIT_MAX_UNIT_HIT_POINTS_RAW
    };
    if !(0..=max_hit_points_raw).contains(&hit_points_raw) {
        return false;
    }
    if shields_raw > CUNIT_MAX_SHIELDS_RAW {
        return false;
    }
    (-512..=8192).contains(&position.x) && (-512..=8192).contains(&position.y)
}

pub(super) fn is_live_cunit_fact(fact: &UnitFact) -> bool {
    fact.order
        .as_ref()
        .and_then(|order| order.order_id)
        .is_some_and(|order_id| order_id != 0)
        && fact
            .vitals
            .as_ref()
            .and_then(|vitals| vitals.hit_points_raw)
            .is_some_and(|hit_points_raw| hit_points_raw > 0)
}

fn is_neutral_resource_type(unit_type_id: Option<u16>) -> bool {
    matches!(unit_type_id, Some(176 | 177 | 178 | 188))
}

pub(super) fn u16_at(raw: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        raw.get(offset..offset + 2)?.try_into().ok()?,
    ))
}

pub(super) fn u32_at(raw: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        raw.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

pub(super) fn usize_at(raw: &[u8], offset: usize, pointer_bytes: usize) -> Option<usize> {
    bwrapi_layout::read_pointer(raw, offset, pointer_bytes)
}

pub(super) fn i16_at(raw: &[u8], offset: usize) -> Option<i16> {
    Some(i16::from_le_bytes(
        raw.get(offset..offset + 2)?.try_into().ok()?,
    ))
}

pub(super) fn i32_at(raw: &[u8], offset: usize) -> Option<i32> {
    Some(i32::from_le_bytes(
        raw.get(offset..offset + 4)?.try_into().ok()?,
    ))
}
