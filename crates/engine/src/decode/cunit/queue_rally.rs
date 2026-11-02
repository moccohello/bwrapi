use super::constants::*;
use super::helpers::{i16_at, u16_at, usize_at};
use bwrapi_layout::{CunitLayout, Layout};
use bwrapi_internal_model::{
    adapted::{Position, ValidationStatus},
    facts::{QueuedOrderFact, UnitFact},
    orders,
};

pub(super) fn attach_queued_orders<F, B>(
    layout: &Layout,
    fact: &mut UnitFact,
    raw: &[u8],
    offset: usize,
    region_base: usize,
    status: ValidationStatus,
    read_exact: &mut F,
) -> bool
where
    F: FnMut(usize, usize) -> Option<B>,
    B: std::ops::Deref<Target = [u8]> + Clone + PartialEq,
{
    let cunit = &layout.cunit;
    let Some(queue_count) = raw.get(offset + cunit.order_queue_count_offset).copied() else { return false; };
    if queue_count > CORDER_MAX_PUBLIC_QUEUE_COUNT {
        return false;
    }
    let Some(head) = usize_at(raw, offset + cunit.order_queue_head_offset, layout.pointer_bytes) else {
        return false;
    };
    let Some(tail) = usize_at(raw, offset + cunit.order_queue_tail_offset, layout.pointer_bytes) else {
        return false;
    };
    if head == 0 || tail == 0 {
        return queue_count == 0 && head == 0 && tail == 0;
    }
    let Some(queued_orders) =
        read_coherent_corder_chain(layout, head, tail, queue_count, region_base, status, read_exact)
    else {
        return false;
    };
    let Some(record_address) = region_base.checked_add(offset) else {
        return false;
    };
    let Some(fresh_raw) = read_exact(record_address, cunit.scan_min_bytes) else {
        return false;
    };
    if !command_queue_identity_matches(layout, raw, offset, &fresh_raw) {
        return false;
    }
    if let Some(order) = fact.order.as_mut() {
        order.queue_count = Some(queue_count);
        order.queued_orders = queued_orders;
        return order.queue_count.is_some();
    }
    false
}

fn command_queue_identity_matches(
    layout: &Layout,
    initial: &[u8],
    offset: usize,
    fresh: &[u8],
) -> bool {
    let cunit = &layout.cunit;
    initial.get(offset + cunit.order_offset) == fresh.get(cunit.order_offset)
        && initial.get(offset + cunit.order_queue_count_offset)
            == fresh.get(cunit.order_queue_count_offset)
        && usize_at(initial, offset + cunit.order_queue_head_offset, layout.pointer_bytes)
            == usize_at(fresh, cunit.order_queue_head_offset, layout.pointer_bytes)
        && usize_at(initial, offset + cunit.order_queue_tail_offset, layout.pointer_bytes)
            == usize_at(fresh, cunit.order_queue_tail_offset, layout.pointer_bytes)
        && usize_at(initial, offset + cunit.order_target_unit_ptr_offset, layout.pointer_bytes)
            == usize_at(fresh, cunit.order_target_unit_ptr_offset, layout.pointer_bytes)
        && i16_at(initial, offset + cunit.order_target_position_x_offset)
            == i16_at(fresh, cunit.order_target_position_x_offset)
        && i16_at(initial, offset + cunit.order_target_position_y_offset)
            == i16_at(fresh, cunit.order_target_position_y_offset)
}

fn read_coherent_corder_chain<F, B>(
    layout: &Layout,
    head: usize,
    tail: usize,
    queue_count: u8,
    cunit_region_base: usize,
    status: ValidationStatus,
    read_exact: &mut F,
) -> Option<Vec<QueuedOrderFact>>
where
    F: FnMut(usize, usize) -> Option<B>,
    B: std::ops::Deref<Target = [u8]> + Clone + PartialEq,
{
    let cunit = &layout.cunit;
    let public_order_limit = usize::from(queue_count);
    let mut orders = Vec::with_capacity(public_order_limit);
    let mut observed_nodes = Vec::with_capacity(public_order_limit + 1);
    let mut previous = 0usize;
    let mut current = head;
    let mut linked_node_count = 0usize;
    // The list can use tail as the last order node. Traverse it inclusively so
    // a one-order queue with head == tail is not discarded.
    loop {
        if current == 0 || linked_node_count >= CORDER_MAX_LINKED_NODE_COUNT {
            return None;
        }
        let raw = read_exact(current, layout.corder.read_bytes)?;
        observed_nodes.push((current, raw.clone()));
        let prev = usize_at(&raw, layout.corder.prev_offset, layout.pointer_bytes)?;
        let next = usize_at(&raw, layout.corder.next_offset, layout.pointer_bytes)?;
        if prev != previous {
            return None;
        }
        let order_id = u16_at(&raw, layout.corder.order_offset)?;
        if usize::from(order_id) >= orders::ORDER_COUNT {
            return None;
        }
        let unit_type_id = u16_at(&raw, layout.corder.unit_type_offset)?;
        let raw_target_position = Position::new(
            i32::from(i16_at(&raw, layout.corder.target_x_offset)?),
            i32::from(i16_at(&raw, layout.corder.target_y_offset)?),
        );
        if !plausible_order_target(raw_target_position) {
            return None;
        }
        let target_unit_ptr = usize_at(&raw, layout.corder.target_unit_ptr_offset, layout.pointer_bytes)?;
        if target_unit_ptr != 0 && raw_target_position == Position::default() {
            // Zero coordinates with a target pointer: the pointer must still
            // resolve, but the target's position does not replace the COrder's
            // stored value.
            read_target_position_from_ptr(
                layout,
                target_unit_ptr,
                cunit_region_base,
                read_exact,
            )?;
        }
        if orders.len() < public_order_limit {
            // COrder coordinates can be zero, so they do not tell the target
            // kind of the order.
            orders.push(QueuedOrderFact {
                order_id: Some(order_id),
                unit_type_id: Some(unit_type_id),
                target_position: Some(raw_target_position),
                target_unit_tag: cunit_tag_from_target_ptr(target_unit_ptr, cunit_region_base, cunit),
                secondary_order_target_position: None,
                move_target_position: None,
                move_target_unit_tag: None,
                status,
            });
        }
        previous = current;
        linked_node_count += 1;
        if current == tail {
            if next != 0 {
                return None;
            }
            break;
        }
        current = next;
    }
    if orders.len() != public_order_limit
        || (linked_node_count != public_order_limit
            && linked_node_count != public_order_limit.saturating_add(1))
    {
        return None;
    }
    for (address, observed) in observed_nodes {
        if read_exact(address, layout.corder.read_bytes)? != observed {
            return None;
        }
    }
    Some(orders)
}

fn read_target_position_from_ptr<F, B>(
    layout: &Layout,
    target_ptr: usize,
    region_base: usize,
    read_exact: &mut F,
) -> Option<Position>
where
    F: FnMut(usize, usize) -> Option<B>,
    B: std::ops::Deref<Target = [u8]> + Clone + PartialEq,
{
    let cunit = &layout.cunit;
    let record_offset = cunit_record_offset_from_target_ptr(target_ptr, region_base, cunit)?;
    let address = region_base
        .checked_add(record_offset)?
        .checked_add(cunit.position_x_offset)?;
    let read_bytes = cunit.position_y_offset
        .checked_sub(cunit.position_x_offset)?
        .checked_add(std::mem::size_of::<i16>())?;
    let raw = read_exact(address, read_bytes)?;
    let position = Position::new(
        i32::from(i16_at(&raw, 0)?),
        i32::from(i16_at(
            &raw,
            cunit.position_y_offset - cunit.position_x_offset,
        )?),
    );
    ((position.x != 0 || position.y != 0) && plausible_order_target(position)).then_some(position)
}

fn plausible_order_target(position: Position) -> bool {
    (0..=8192).contains(&position.x) && (0..=8192).contains(&position.y)
}

pub fn cunit_tag_from_target_ptr(
    target_ptr: usize,
    region_base: usize,
    cunit: &CunitLayout,
) -> Option<String> {
    cunit_record_offset_from_target_ptr(target_ptr, region_base, cunit)
        .map(|offset| format!("cunit:record:{offset}"))
}

pub fn cunit_record_offset_from_target_ptr(
    target_ptr: usize,
    region_base: usize,
    cunit: &CunitLayout,
) -> Option<usize> {
    if target_ptr == 0 {
        return None;
    }
    cunit_record_offset_from_ptr(target_ptr, region_base, cunit).or_else(|| {
        target_ptr
            .checked_sub(cunit.selection_ptr_adjustment)
            .and_then(|ptr| cunit_record_offset_from_ptr(ptr, region_base, cunit))
    })
}

/// Resolve a direct CUnit pointer. Union-owned list links must not accept the
/// selection-table `+0x10` adjustment that target pointers support.
pub fn cunit_record_offset_from_direct_ptr(
    target_ptr: usize,
    region_base: usize,
    cunit: &CunitLayout,
) -> Option<usize> {
    (target_ptr != 0)
        .then(|| cunit_record_offset_from_ptr(target_ptr, region_base, cunit))
        .flatten()
}

fn cunit_record_offset_from_ptr(
    ptr: usize,
    region_base: usize,
    cunit: &CunitLayout,
) -> Option<usize> {
    let offset = ptr.checked_sub(region_base)?;
    if offset >= cunit.region_bytes {
        return None;
    }
    // The pool grid accepts record offsets from both the array base and the
    // first-slot base (the two supported alignments, [0, record_stride]).
    [0, cunit.record_stride]
        .iter()
        .any(|alignment| offset >= *alignment && (offset - alignment) % cunit.record_stride == 0)
        .then_some(offset)
}

pub fn unit_type_can_rally(unit_type: u16) -> bool {
    matches!(
        unit_type,
        106 | 111 | 113 | 114 | 131 | 132 | 133 | 154 | 155 | 160 | 167
    )
}

pub fn active_rally_position(
    unit_type: u16,
    order: u8,
    _status_flags: u32,
    rally_x: i16,
    rally_y: i16,
) -> Option<Position> {
    if !unit_type_can_rally(unit_type) || is_lifted_rally_building(unit_type, order) {
        return None;
    }

    let position = Position::new(i32::from(rally_x), i32::from(rally_y));
    (position != Position::default() && plausible_order_target(position)).then_some(position)
}

fn is_lifted_rally_building(unit_type: u16, order: u8) -> bool {
    matches!(unit_type, 106 | 111 | 113 | 114)
        && matches!(order, 6 | 13 | 49 | 70 | 71 | 72 | 73 | 74)
}
