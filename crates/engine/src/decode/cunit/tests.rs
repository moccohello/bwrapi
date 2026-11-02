use bwrapi_layout::Layout;
use super::*;
use bwrapi_internal_model::{
    adapted::{Position, ValidationStatus},
    facts::SourceAddress,
};
use std::cell::Cell;

fn put_cunit_record(
    raw: &mut [u8],
    offset: usize,
    owner_id: u8,
    unit_type_id: u16,
    order_id: u8,
    hit_points_raw: i32,
    shields_raw: u32,
    position: (i16, i16),
) {
    raw[offset + CUNIT_OWNER_OFFSET] = owner_id;
    raw[offset + CUNIT_ORDER_OFFSET] = order_id;
    raw[offset + CUNIT_TYPE_OFFSET..offset + CUNIT_TYPE_OFFSET + 2]
        .copy_from_slice(&unit_type_id.to_le_bytes());
    raw[offset + CUNIT_HIT_POINTS_OFFSET..offset + CUNIT_HIT_POINTS_OFFSET + 4]
        .copy_from_slice(&hit_points_raw.to_le_bytes());
    raw[offset + CUNIT_SHIELDS_OFFSET..offset + CUNIT_SHIELDS_OFFSET + 4]
        .copy_from_slice(&shields_raw.to_le_bytes());
    raw[offset + CUNIT_POSITION_X_OFFSET..offset + CUNIT_POSITION_X_OFFSET + 2]
        .copy_from_slice(&position.0.to_le_bytes());
    raw[offset + CUNIT_POSITION_Y_OFFSET..offset + CUNIT_POSITION_Y_OFFSET + 2]
        .copy_from_slice(&position.1.to_le_bytes());
}

fn put_cunit_production(
    raw: &mut [u8],
    offset: usize,
    remaining_frames: u16,
    tech_type_id: Option<u8>,
    upgrade_type_id: Option<u8>,
) {
    raw[offset + CUNIT_PRODUCTION_REMAINING_TIME_OFFSET
        ..offset + CUNIT_PRODUCTION_REMAINING_TIME_OFFSET + 2]
        .copy_from_slice(&remaining_frames.to_le_bytes());
    if let Some(tech_type_id) = tech_type_id {
        raw[offset + CUNIT_RESEARCH_TECH_TYPE_OFFSET] = tech_type_id;
    }
    if let Some(upgrade_type_id) = upgrade_type_id {
        raw[offset + CUNIT_UPGRADE_TYPE_OFFSET] = upgrade_type_id;
    }
}

fn put_cunit_status(raw: &mut [u8], offset: usize, status_flags: u32) {
    raw[offset + CUNIT_STATUS_FLAGS_OFFSET..offset + CUNIT_STATUS_FLAGS_OFFSET + 4]
        .copy_from_slice(&status_flags.to_le_bytes());
}

fn put_i16(raw: &mut [u8], offset: usize, value: i16) {
    raw[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn put_usize(raw: &mut [u8], offset: usize, value: usize) {
    raw[offset..offset + 8].copy_from_slice(&(value as u64).to_le_bytes());
}

fn corder_node(
    previous: usize,
    next: usize,
    order_id: u16,
    unit_type_id: u16,
    target: (i16, i16),
    target_unit_ptr: usize,
) -> Vec<u8> {
    let mut raw = vec![0; CORDER_READ_BYTES];
    put_usize(&mut raw, CORDER_PREV_OFFSET, previous);
    put_usize(&mut raw, CORDER_NEXT_OFFSET, next);
    raw[CORDER_ORDER_OFFSET..CORDER_ORDER_OFFSET + 2].copy_from_slice(&order_id.to_le_bytes());
    raw[CORDER_UNIT_TYPE_OFFSET..CORDER_UNIT_TYPE_OFFSET + 2]
        .copy_from_slice(&unit_type_id.to_le_bytes());
    put_i16(&mut raw, CORDER_TARGET_X_OFFSET, target.0);
    put_i16(&mut raw, CORDER_TARGET_Y_OFFSET, target.1);
    put_usize(&mut raw, CORDER_TARGET_UNIT_PTR_OFFSET, target_unit_ptr);
    raw
}

fn corder_terminal_node(previous: usize) -> Vec<u8> {
    corder_node(previous, 0, 151, 228, (0, 0), 0)
}

#[test]
fn decodes_live_records_and_filters_stale() {
    let mut raw = vec![0; CUNIT_STRIDE * 2];
    put_cunit_record(&mut raw, 0, 1, 7, 3, 15360, 0, (64, 96));
    put_cunit_record(&mut raw, CUNIT_STRIDE, 1, 7, 0, 0, 77, (64, 96));

    let facts = iter_cunit_unit_facts(&Layout::X64, 
        &raw,
        None,
        Some(1),
        false,
        ValidationStatus::StrongCandidate,
    );

    assert_eq!(facts.len(), 1);
    assert_eq!(facts[0].source_namespace, "cunit");
    assert_eq!(facts[0].record_index, Some(0));
    assert_eq!(facts[0].owner_id, Some(1));
    assert_eq!(facts[0].unit_type_id, Some(7));
    assert_eq!(facts[0].position, Some(Position::new(64, 96)));
    assert_eq!(
        facts[0].order.as_ref().and_then(|order| order.order_id),
        Some(3)
    );
    assert_eq!(
        facts[0]
            .vitals
            .as_ref()
            .and_then(|vitals| vitals.hit_points_raw),
        Some(15360)
    );
    assert_eq!(facts[0].status, ValidationStatus::StrongCandidate);
}

#[test]
fn excludes_depleted_zero_order_mineral_fields_and_keeps_active_fields() {
    let mut raw = vec![0; CUNIT_STRIDE * 2];
    put_cunit_record(&mut raw, 0, 11, 176, 0, 25_600_000, 25_600, (320, 640));
    put_cunit_record(
        &mut raw,
        CUNIT_STRIDE,
        11,
        176,
        CUNIT_ORDER_IDLE,
        25_600_000,
        25_600,
        (384, 640),
    );

    let facts = iter_cunit_unit_facts(&Layout::X64, 
        &raw,
        Some(0x1000_0000),
        None,
        false,
        ValidationStatus::Validated,
    );

    assert_eq!(facts.len(), 1);
    assert_eq!(facts[0].record_index, Some(CUNIT_STRIDE));
    assert_eq!(facts[0].owner_id, Some(11));
    assert_eq!(facts[0].unit_type_id, Some(176));
    assert_eq!(facts[0].position, Some(Position::new(384, 640)));
    assert_eq!(
        facts[0].order.as_ref().and_then(|order| order.order_id),
        Some(u16::from(CUNIT_ORDER_IDLE))
    );
    assert_eq!(
        facts[0]
            .vitals
            .as_ref()
            .and_then(|vitals| vitals.hit_points_raw),
        Some(25_600_000)
    );
}

#[test]
fn rally_position_accepts_incomplete_grounded_rally_building() {
    let mut raw = vec![0; CUNIT_STRIDE * 2];
    put_cunit_record(&mut raw, 0, 1, 106, 23, 384000, 0, (320, 640));
    put_i16(&mut raw, CUNIT_RALLY_POSITION_X_OFFSET, 0);
    put_i16(&mut raw, CUNIT_RALLY_POSITION_Y_OFFSET, 704);
    raw[CUNIT_STATUS_FLAGS_OFFSET..CUNIT_STATUS_FLAGS_OFFSET + 4]
        .copy_from_slice(&CUNIT_STATUS_COMPLETED.to_le_bytes());
    put_cunit_record(&mut raw, CUNIT_STRIDE, 1, 106, 23, 384000, 0, (384, 640));
    put_i16(&mut raw, CUNIT_STRIDE + CUNIT_RALLY_POSITION_X_OFFSET, 448);
    put_i16(&mut raw, CUNIT_STRIDE + CUNIT_RALLY_POSITION_Y_OFFSET, 704);

    let facts = iter_cunit_unit_facts(&Layout::X64, &raw, None, Some(1), false, ValidationStatus::Validated);

    assert_eq!(
        facts[0]
            .order
            .as_ref()
            .and_then(|order| order.rally_position),
        Some(Position::new(0, 704))
    );
    assert_eq!(
        facts[1]
            .order
            .as_ref()
            .and_then(|order| order.rally_position),
        Some(Position::new(448, 704))
    );
}

#[test]
fn rally_unit_decodes_plus10_target_from_the_other_cunit_alignment() {
    let cunit_base = 0x1000_0000;
    let producer_offset = CUNIT_ALIGNMENTS[0];
    let target_offset = CUNIT_ALIGNMENTS[1];
    let mut raw = vec![0; CUNIT_STRIDE * 2];
    put_cunit_record(
        &mut raw,
        producer_offset,
        1,
        106,
        CUNIT_ORDER_IDLE,
        384_000,
        0,
        (320, 640),
    );
    put_cunit_record(
        &mut raw,
        target_offset,
        1,
        7,
        CUNIT_ORDER_IDLE,
        15_360,
        0,
        (384, 640),
    );
    put_usize(
        &mut raw,
        producer_offset + CUNIT_RALLY_UNIT_PTR_OFFSET,
        cunit_base + target_offset + CUNIT_SELECTION_PTR_ADJUSTMENT,
    );

    let facts = iter_cunit_unit_facts(&Layout::X64, 
        &raw,
        Some(cunit_base),
        Some(1),
        false,
        ValidationStatus::Validated,
    );
    let producer = facts
        .iter()
        .find(|fact| fact.record_index == Some(producer_offset))
        .expect("producer");

    assert_eq!(
        producer
            .order
            .as_ref()
            .and_then(|order| order.rally_unit_tag.as_deref()),
        Some("cunit:record:488")
    );
}

#[test]
fn decodes_coherent_corder_queue_targets() {
    let cunit_base = 0x1000_0000;
    let first_node = 0x3000_0000;
    let second_node = 0x3000_0040;
    let terminal_node = 0x3000_0080;
    let mut raw = vec![0; CUNIT_STRIDE * 2];
    put_cunit_record(&mut raw, 0, 1, 7, 6, 15360, 0, (320, 640));
    put_cunit_record(&mut raw, CUNIT_STRIDE, 1, 106, 23, 384000, 0, (512, 640));
    put_usize(&mut raw, CUNIT_ORDER_QUEUE_HEAD_OFFSET, first_node);
    put_usize(&mut raw, CUNIT_ORDER_QUEUE_TAIL_OFFSET, terminal_node);
    raw[CUNIT_ORDER_QUEUE_COUNT_OFFSET] = 2;
    let first = corder_node(
        first_node.wrapping_sub(first_node),
        second_node,
        6,
        228,
        (352, 672),
        0,
    );
    let second = corder_node(
        first_node,
        terminal_node,
        10,
        106,
        (512, 640),
        cunit_base + CUNIT_STRIDE + 0x10,
    );
    let terminal = corder_terminal_node(second_node);

    let facts = iter_cunit_unit_facts_with_queued_orders(&Layout::X64, 
        &raw,
        cunit_base,
        Some(1),
        false,
        ValidationStatus::Validated,
        |address, _size| match address {
            address if address == first_node => Some(first.clone()),
            address if address == second_node => Some(second.clone()),
            address if address == terminal_node => Some(terminal.clone()),
            address if address == cunit_base => Some(raw[..CUNIT_SCAN_MIN_BYTES].to_vec()),
            _ => None,
        },
    );

    let queued = &facts[0].order.as_ref().unwrap().queued_orders;
    assert_eq!(queued.len(), 2);
    assert_eq!(queued[0].order_id, Some(6));
    assert_eq!(queued[0].target_position, Some(Position::new(352, 672)));
    assert_eq!(queued[0].target_unit_tag, None);
    assert_eq!(queued[1].order_id, Some(10));
    assert_eq!(queued[1].target_position, Some(Position::new(512, 640)));
    assert_eq!(
        queued[1].target_unit_tag.as_deref(),
        Some("cunit:record:488")
    );
}

#[test]
fn preserves_pointer_free_corder_zero_coordinate_as_one_source_row() {
    let cunit_base = 0x1000_0000;
    let node = 0x3000_0000;
    let terminal_node = 0x3000_0040;
    let mut raw = vec![0; CUNIT_STRIDE];
    put_cunit_record(&mut raw, 0, 1, 7, 6, 15360, 0, (320, 640));
    put_usize(&mut raw, CUNIT_ORDER_QUEUE_HEAD_OFFSET, node);
    put_usize(&mut raw, CUNIT_ORDER_QUEUE_TAIL_OFFSET, terminal_node);
    raw[CUNIT_ORDER_QUEUE_COUNT_OFFSET] = 1;
    let queued_node = corder_node(0, terminal_node, 6, 228, (0, 0), 0);
    let terminal = corder_terminal_node(node);

    let facts = iter_cunit_unit_facts_with_queued_orders(&Layout::X64, 
        &raw,
        cunit_base,
        Some(1),
        false,
        ValidationStatus::Validated,
        |address, _size| match address {
            address if address == node => Some(queued_node.clone()),
            address if address == terminal_node => Some(terminal.clone()),
            address if address == cunit_base => Some(raw[..CUNIT_SCAN_MIN_BYTES].to_vec()),
            _ => None,
        },
    );

    let order = facts[0].order.as_ref().unwrap();
    assert_eq!(order.queue_count, Some(1));
    assert_eq!(order.queued_orders.len(), 1);
    assert_eq!(order.queued_orders[0].target_position, Some(Position::new(0, 0)));
}

#[test]
fn corder_coordinates_preserve_zero_rows_followers_and_stored_unit_target_position() {
    let base = 0x1000_0000;
    let head = 0x3000_0000;
    for points in [[(0, 0), (96, 128), (704, 832)], [(96, 128), (0, 0), (704, 832)]] {
        let mut raw = vec![0; CUNIT_STRIDE * 2];
        put_cunit_record(&mut raw, 0, 1, 7, 6, 15360, 0, (320, 640));
        put_cunit_record(&mut raw, CUNIT_STRIDE, 1, 37, 23, 9000, 0, (512, 672));
        raw[CUNIT_ORDER_QUEUE_COUNT_OFFSET] = 3;
        put_usize(&mut raw, CUNIT_ORDER_QUEUE_HEAD_OFFSET, head);
        put_usize(&mut raw, CUNIT_ORDER_QUEUE_TAIL_OFFSET, head + 3 * 0x40);
        let nodes: Vec<_> = (0..3).map(|index| corder_node(
            if index == 0 { 0 } else { head + (index - 1) * 0x40 },
            head + (index + 1) * 0x40, 6, 0, points[index],
            if index == 1 { base + CUNIT_STRIDE } else { 0 },
        )).chain(std::iter::once(corder_terminal_node(head + 2 * 0x40))).collect();
        let facts = iter_cunit_unit_facts_with_queued_orders(&Layout::X64, 
            &raw, base, Some(1), false, ValidationStatus::Validated,
            |address, _size| {
                if address == base { return Some(raw[..CUNIT_SCAN_MIN_BYTES].to_vec()); }
                if address == base + CUNIT_STRIDE + CUNIT_POSITION_X_OFFSET {
                    return Some(raw[CUNIT_STRIDE + CUNIT_POSITION_X_OFFSET..CUNIT_STRIDE + CUNIT_POSITION_Y_OFFSET + 2].to_vec());
                }
                if address >= head && address <= head + 3 * 0x40 && (address - head) % 0x40 == 0 {
                    return Some(nodes[(address - head) / 0x40].clone());
                }
                None
            },
        );
        let order = facts[0].order.as_ref().unwrap();
        assert_eq!(order.queue_count, Some(3));
        assert_eq!(order.queued_orders.len(), 3);
        for (row, point) in order.queued_orders.iter().zip(points) {
            assert_eq!(row.target_position, Some(Position::new(i32::from(point.0), i32::from(point.1))));
        }
        assert_eq!(order.queued_orders[1].target_unit_tag.as_deref(), Some("cunit:record:488"));
    }
}

#[test]
fn corder_linked_count_rejects_extra_nodes_but_keeps_count_and_one_tail_compatibility() {
    let base = 0x1000_0000;
    let head = 0x3000_0000;
    let nodes = [
        corder_node(0, head + 0x40, 6, 0, (0, 0), 0),
        corder_node(head, head + 0x80, 6, 0, (96, 128), 0),
        corder_node(head + 0x40, 0, 6, 0, (704, 832), 0),
    ];
    for count in [1u8, 2, 3] {
        let mut raw = vec![0; CUNIT_STRIDE];
        put_cunit_record(&mut raw, 0, 1, 7, 6, 15360, 0, (320, 640));
        raw[CUNIT_ORDER_QUEUE_COUNT_OFFSET] = count;
        put_usize(&mut raw, CUNIT_ORDER_QUEUE_HEAD_OFFSET, head);
        put_usize(&mut raw, CUNIT_ORDER_QUEUE_TAIL_OFFSET, head + 0x80);
        let facts = iter_cunit_unit_facts_with_queued_orders(&Layout::X64, 
            &raw, base, Some(1), false, ValidationStatus::Validated,
            |address, _size| {
                if address == base { return Some(raw[..CUNIT_SCAN_MIN_BYTES].to_vec()); }
                if address >= head && address <= head + 0x80 && (address - head) % 0x40 == 0 {
                    return Some(nodes[(address - head) / 0x40].clone());
                }
                None
            },
        );
        let order = facts[0].order.as_ref().unwrap();
        if count == 1 {
            assert_eq!(order.queue_count, None);
            assert!(order.queued_orders.is_empty());
        } else {
            assert_eq!(order.queue_count, Some(count));
            assert_eq!(order.queued_orders.len(), usize::from(count));
            assert_eq!(order.queued_orders[0].target_position, Some(Position::new(0, 0)));
        }
    }
}

#[test]
fn retains_base_unit_when_corder_target_is_unresolved() {
    let cunit_base = 0x1000_0000;
    let node = 0x3000_0000;
    let terminal_node = 0x3000_0040;
    let mut raw = vec![0; CUNIT_STRIDE];
    put_cunit_record(&mut raw, 0, 1, 7, 6, 15360, 0, (320, 640));
    put_usize(&mut raw, CUNIT_ORDER_QUEUE_HEAD_OFFSET, node);
    put_usize(&mut raw, CUNIT_ORDER_QUEUE_TAIL_OFFSET, terminal_node);
    raw[CUNIT_ORDER_QUEUE_COUNT_OFFSET] = 1;
    let queued_node = corder_node(0, terminal_node, 6, 228, (0, 0), 0x5000_0000);
    let terminal = corder_terminal_node(node);

    let facts = iter_cunit_unit_facts_with_queued_orders(&Layout::X64, 
        &raw,
        cunit_base,
        Some(1),
        false,
        ValidationStatus::Validated,
        |address, _size| match address {
            address if address == node => Some(queued_node.clone()),
            address if address == terminal_node => Some(terminal.clone()),
            address if address == cunit_base => Some(raw[..CUNIT_SCAN_MIN_BYTES].to_vec()),
            _ => None,
        },
    );

    assert_eq!(facts.len(), 1);
    let order = facts[0].order.as_ref().unwrap();
    assert_eq!(order.queue_count, None);
    assert!(order.queued_orders.is_empty());
}

#[test]
fn retains_base_unit_when_corder_node_changes_during_fence() {
    let cunit_base = 0x1000_0000;
    let node = 0x3000_0000;
    let terminal_node = 0x3000_0040;
    let mut raw = vec![0; CUNIT_STRIDE];
    put_cunit_record(&mut raw, 0, 1, 7, 6, 15360, 0, (320, 640));
    put_usize(&mut raw, CUNIT_ORDER_QUEUE_HEAD_OFFSET, node);
    put_usize(&mut raw, CUNIT_ORDER_QUEUE_TAIL_OFFSET, terminal_node);
    raw[CUNIT_ORDER_QUEUE_COUNT_OFFSET] = 1;
    let initial = corder_node(0, terminal_node, 6, 228, (352, 672), 0);
    let mut changed = initial.clone();
    put_i16(&mut changed, CORDER_TARGET_X_OFFSET, 353);
    let terminal = corder_terminal_node(node);
    let node_reads = Cell::new(0);

    let facts = iter_cunit_unit_facts_with_queued_orders(&Layout::X64, 
        &raw,
        cunit_base,
        Some(1),
        false,
        ValidationStatus::Validated,
        |address, _size| match address {
            address if address == node => {
                let read = node_reads.get();
                node_reads.set(read + 1);
                Some(if read == 0 {
                    initial.clone()
                } else {
                    changed.clone()
                })
            }
            address if address == terminal_node => Some(terminal.clone()),
            address if address == cunit_base => Some(raw[..CUNIT_SCAN_MIN_BYTES].to_vec()),
            _ => None,
        },
    );

    assert_eq!(facts.len(), 1);
    let order = facts[0].order.as_ref().unwrap();
    assert_eq!(order.queue_count, None);
    assert!(order.queued_orders.is_empty());
    assert_eq!(node_reads.get(), 2);
}

#[test]
fn decodes_current_order_target_unit_tag() {
    let cunit_base = 0x1000_0000;
    let mut raw = vec![0; CUNIT_STRIDE * 2];
    put_cunit_record(&mut raw, 0, 1, 43, 10, 12000, 0, (320, 640));
    put_cunit_record(&mut raw, CUNIT_STRIDE, 2, 37, 23, 9000, 0, (512, 672));
    put_usize(
        &mut raw,
        CUNIT_ORDER_TARGET_UNIT_PTR_OFFSET,
        cunit_base + CUNIT_STRIDE + 0x10,
    );

    let facts = iter_cunit_unit_facts(&Layout::X64, 
        &raw,
        Some(cunit_base),
        None,
        false,
        ValidationStatus::Validated,
    );

    assert_eq!(
        facts[0]
            .order
            .as_ref()
            .and_then(|order| order.target_unit_tag.as_deref()),
        Some("cunit:record:488")
    );
}

#[test]
fn retains_base_unit_with_incoherent_corder_queue_chain() {
    let cunit_base = 0x1000_0000;
    let first_node = 0x3000_0000;
    let second_node = 0x3000_0040;
    let terminal_node = 0x3000_0080;
    let mut raw = vec![0; CUNIT_STRIDE];
    put_cunit_record(&mut raw, 0, 1, 7, 6, 15360, 0, (320, 640));
    put_usize(&mut raw, CUNIT_ORDER_QUEUE_HEAD_OFFSET, first_node);
    put_usize(&mut raw, CUNIT_ORDER_QUEUE_TAIL_OFFSET, terminal_node);
    raw[CUNIT_ORDER_QUEUE_COUNT_OFFSET] = 2;
    let first = corder_node(0, second_node, 6, 228, (352, 672), 0);
    let second = corder_node(0, terminal_node, 6, 228, (384, 704), 0);
    let terminal = corder_terminal_node(second_node);

    let facts = iter_cunit_unit_facts_with_queued_orders(&Layout::X64, 
        &raw,
        cunit_base,
        None,
        false,
        ValidationStatus::Validated,
        |address, _size| match address {
            address if address == first_node => Some(first.clone()),
            address if address == second_node => Some(second.clone()),
            address if address == terminal_node => Some(terminal.clone()),
            _ => None,
        },
    );

    assert_eq!(facts.len(), 1);
    let order = facts[0].order.as_ref().unwrap();
    assert_eq!(order.queue_count, None);
    assert!(order.queued_orders.is_empty());
}

#[test]
fn can_include_stale_for_identity_tracking() {
    let mut raw = vec![0; CUNIT_STRIDE];
    put_cunit_record(&mut raw, 0, 1, 7, 0, 0, 0, (320, 640));

    let facts =
        iter_cunit_unit_facts(&Layout::X64, &raw, None, None, true, ValidationStatus::StrongCandidate);

    assert_eq!(facts.len(), 1);
    assert_eq!(
        facts[0].order.as_ref().and_then(|order| order.order_id),
        Some(0)
    );
    assert_eq!(
        facts[0]
            .vitals
            .as_ref()
            .and_then(|vitals| vitals.hit_points_raw),
        Some(0)
    );
}

#[test]
fn uses_region_base_as_source_address() {
    let mut raw = vec![0; CUNIT_STRIDE];
    put_cunit_record(&mut raw, 0, 1, 7, 3, 15360, 0, (320, 640));

    let facts = iter_cunit_unit_facts(&Layout::X64, 
        &raw,
        Some(0x1000_0000),
        None,
        false,
        ValidationStatus::StrongCandidate,
    );

    assert_eq!(facts[0].record_index, Some(0));
    assert_eq!(
        facts[0].source_address,
        Some(SourceAddress::from(0x1000_0000_usize))
    );
}

#[test]
fn decodes_research_and_upgrade_production() {
    let mut raw = vec![0; CUNIT_STRIDE * 2];
    put_cunit_record(
        &mut raw,
        0,
        1,
        111,
        CUNIT_ORDER_RESEARCH,
        15360,
        0,
        (320, 640),
    );
    put_cunit_status(&mut raw, 0, CUNIT_STATUS_COMPLETED);
    put_cunit_production(&mut raw, 0, 123, Some(3), None);
    put_cunit_record(
        &mut raw,
        CUNIT_STRIDE,
        1,
        111,
        CUNIT_ORDER_UPGRADE,
        15360,
        0,
        (320, 640),
    );
    put_cunit_status(&mut raw, CUNIT_STRIDE, CUNIT_STATUS_COMPLETED);
    put_cunit_production(&mut raw, CUNIT_STRIDE, 456, None, Some(13));

    let facts =
        iter_cunit_unit_facts(&Layout::X64, &raw, None, None, false, ValidationStatus::StrongCandidate);
    let (upgrades, techs) = derive_cunit_in_progress_facts(&facts);

    let research = facts[0].production.as_ref().expect("research production");
    assert_eq!(research.is_completed, Some(true));
    assert_eq!(research.tech_type_id, Some(3));
    assert_eq!(research.remaining_research_frames, Some(123));
    let upgrade = facts[1].production.as_ref().expect("upgrade production");
    assert_eq!(upgrade.is_completed, Some(true));
    assert_eq!(upgrade.upgrade_type_id, Some(13));
    assert_eq!(upgrade.remaining_upgrade_frames, Some(456));
    assert_eq!(techs.len(), 1);
    assert_eq!(techs[0].player_id, 1);
    assert_eq!(techs[0].tech_type_id, 3);
    assert_eq!(techs[0].remaining_frames, Some(123));
    assert_eq!(upgrades.len(), 1);
    assert_eq!(upgrades[0].player_id, 1);
    assert_eq!(upgrades[0].upgrade_type_id, 13);
    assert_eq!(upgrades[0].remaining_frames, Some(456));
}

#[test]
fn construction_countdown_uses_current_raw_timer_for_every_race() {
    // A non-morphing construction reads the raw timer as is, for every race.
    for (unit_type, order) in [
        (109, CUNIT_ORDER_INCOMPLETE_BUILDING),
        (131, CUNIT_ORDER_INCOMPLETE_BUILDING),
        (156, CUNIT_ORDER_BUILDING_IN_PROGRESS),
    ] {
        for remaining in [550u16, 419, 0] {
            let mut raw = vec![0; CUNIT_STRIDE];
            put_cunit_record(&mut raw, 0, 1, unit_type, order, 1250 * 256, 0, (496, 2144));
            put_cunit_production(&mut raw, 0, 999, None, None);
            raw[CUNIT_BUILD_UNIT_REMAINING_TIME_OFFSET..][..2]
                .copy_from_slice(&remaining.to_le_bytes());
            let facts = iter_cunit_unit_facts(&Layout::X64, 
                &raw, None, None, false, ValidationStatus::StrongCandidate,
            );
            let production = facts[0].production.as_ref().unwrap();
            assert_eq!(production.is_completed, Some(false));
            assert_eq!(production.remaining_build_frames, Some(u32::from(remaining)));
            assert_eq!(production.remaining_train_frames, None);
            assert_eq!(production.remaining_research_frames, None);
            assert_eq!(production.remaining_upgrade_frames, None);
        }
    }
}

#[test]
fn completed_units_ignore_stale_construction_countdown_for_every_race() {
    for unit_type in [7, 109, 131, 156] {
        let mut raw = vec![0; CUNIT_STRIDE];
        put_cunit_record(&mut raw, 0, 1, unit_type, CUNIT_ORDER_IDLE, 1250 * 256, 0, (496, 2144));
        put_cunit_status(&mut raw, 0, CUNIT_STATUS_COMPLETED);
        raw[CUNIT_BUILD_UNIT_REMAINING_TIME_OFFSET..][..2]
            .copy_from_slice(&550u16.to_le_bytes());
        let facts = iter_cunit_unit_facts(&Layout::X64, 
            &raw, None, None, false, ValidationStatus::StrongCandidate,
        );
        let production = facts[0].production.as_ref().unwrap();
        assert_eq!(production.is_completed, Some(true));
        assert_eq!(production.remaining_build_frames, Some(0));
    }
}

#[test]
fn morph_construction_countdown_preserves_current_queue_availability() {
    for (queued_type, expected) in [(132u16, Some(550)), (CUNIT_UNIT_TYPE_NONE, Some(0)), (229, None)] {
        let mut raw = vec![0; CUNIT_STRIDE];
        put_cunit_record(&mut raw, 0, 1, 131, CUNIT_ORDER_ZERG_BUILDING_MORPH, 1250 * 256, 0, (496, 2144));
        // Morphing overrides raw Completed, but cannot invent a target type.
        put_cunit_status(&mut raw, 0, CUNIT_STATUS_COMPLETED);
        raw[CUNIT_BUILD_QUEUE_TYPE_OFFSET..][..2]
            .copy_from_slice(&queued_type.to_le_bytes());
        raw[CUNIT_BUILD_UNIT_REMAINING_TIME_OFFSET..][..2]
            .copy_from_slice(&550u16.to_le_bytes());
        let facts = iter_cunit_unit_facts(&Layout::X64, 
            &raw, None, None, false, ValidationStatus::StrongCandidate,
        );
        let production = facts[0].production.as_ref().unwrap();
        assert_eq!(production.is_completed, Some(false));
        assert_eq!(production.remaining_build_frames, expected);
    }
}

#[test]
fn ignores_stale_upgrade_timer_for_completed_building() {
    let mut raw = vec![0; CUNIT_STRIDE];
    put_cunit_record(
        &mut raw,
        0,
        1,
        164,
        CUNIT_ORDER_IDLE,
        55000,
        55000,
        (320, 640),
    );
    put_cunit_status(&mut raw, 0, CUNIT_STATUS_COMPLETED);
    put_cunit_production(&mut raw, 0, 456, None, Some(13));

    let facts =
        iter_cunit_unit_facts(&Layout::X64, &raw, None, None, false, ValidationStatus::StrongCandidate);
    let (upgrades, techs) = derive_cunit_in_progress_facts(&facts);

    let production = facts[0].production.as_ref().expect("building production");
    assert_eq!(production.is_completed, Some(true));
    assert_eq!(production.remaining_build_frames, Some(0));
    assert!(upgrades.is_empty());
    assert!(techs.is_empty());
}

#[test]
fn only_projects_shields_for_protoss_types() {
    let mut raw = vec![0; CUNIT_STRIDE * 2];
    put_cunit_record(&mut raw, 0, 1, 7, 3, 15360, 25600, (320, 640));
    put_cunit_record(&mut raw, CUNIT_STRIDE, 0, 65, 3, 25600, 15360, (320, 640));

    let facts =
        iter_cunit_unit_facts(&Layout::X64, &raw, None, None, false, ValidationStatus::StrongCandidate);

    let first_vitals = facts[0].vitals.as_ref().expect("first vitals");
    let second_vitals = facts[1].vitals.as_ref().expect("second vitals");
    assert_eq!(first_vitals.shields, Some(0));
    assert_eq!(first_vitals.shields_raw, Some(25600));
    assert_eq!(second_vitals.shields, None);
    assert_eq!(second_vitals.shields_raw, Some(15360));
}

#[test]
fn hero_shields_keep_the_source_value_for_semantic_projection() {
    for unit_type in [84, 85, 86, 87, 88, 98] {
        let mut raw = vec![0; CUNIT_STRIDE];
        put_cunit_record(&mut raw, 0, 1, unit_type, 3, 25600, 15360, (320, 640));
        let facts =
            iter_cunit_unit_facts(&Layout::X64, &raw, None, None, false, ValidationStatus::StrongCandidate);
        let vitals = facts[0].vitals.as_ref().expect("hero vitals");
        // The consumer converts the raw shield value; the reader must not
        // override a shield-bearing type with a literal zero.
        assert_eq!(vitals.shields, None, "type {unit_type}");
        assert_eq!(vitals.shields_raw, Some(15360), "type {unit_type}");
    }
}

#[test]
fn marks_worker_construction_orders() {
    let mut raw = vec![0; CUNIT_STRIDE];
    put_cunit_record(
        &mut raw,
        0,
        1,
        7,
        CUNIT_ORDER_CONSTRUCTING,
        15360,
        0,
        (320, 640),
    );

    let facts =
        iter_cunit_unit_facts(&Layout::X64, &raw, None, None, false, ValidationStatus::StrongCandidate);
    let production = facts[0]
        .production
        .as_ref()
        .expect("constructing production");
    let (upgrades, techs) = derive_cunit_in_progress_facts(&facts);

    assert_eq!(production.is_completed, Some(false));
    assert!(upgrades.is_empty());
    assert!(techs.is_empty());
}

#[test]
fn decodes_building_completion_from_status_bit_only() {
    let mut raw = vec![0; CUNIT_STRIDE * 3];
    put_cunit_record(
        &mut raw,
        0,
        1,
        156,
        CUNIT_ORDER_BUILDING_IN_PROGRESS,
        12300,
        12480,
        (320, 640),
    );
    put_cunit_production(&mut raw, 0, 120, None, None);
    put_cunit_record(
        &mut raw,
        CUNIT_STRIDE,
        1,
        156,
        CUNIT_ORDER_IDLE,
        64000,
        64000,
        (320, 640),
    );
    put_cunit_production(&mut raw, CUNIT_STRIDE, 96, None, None);
    put_cunit_record(
        &mut raw,
        CUNIT_STRIDE * 2,
        1,
        156,
        CUNIT_ORDER_IDLE,
        12000,
        8000,
        (320, 640),
    );
    put_cunit_status(&mut raw, CUNIT_STRIDE * 2, CUNIT_STATUS_COMPLETED);

    let facts =
        iter_cunit_unit_facts(&Layout::X64, &raw, None, None, false, ValidationStatus::StrongCandidate);

    assert_eq!(
        facts[0].production.as_ref().unwrap().is_completed,
        Some(false)
    );
    assert_eq!(
        facts[0].production.as_ref().unwrap().remaining_build_frames,
        Some(0)
    );
    assert_eq!(
        facts[1].production.as_ref().unwrap().is_completed,
        Some(false)
    );
    assert_eq!(
        facts[1].production.as_ref().unwrap().remaining_build_frames,
        Some(0)
    );
    assert_eq!(
        facts[2].production.as_ref().unwrap().is_completed,
        Some(true)
    );
}

#[test]
fn decodes_mobile_unit_completion_from_status_bit() {
    let mut raw = vec![0; CUNIT_STRIDE * 2];
    put_cunit_record(&mut raw, 0, 1, 66, CUNIT_ORDER_IDLE, 25600, 0, (320, 640));
    put_cunit_record(
        &mut raw,
        CUNIT_STRIDE,
        1,
        66,
        CUNIT_ORDER_IDLE,
        25600,
        0,
        (352, 640),
    );
    put_cunit_status(&mut raw, CUNIT_STRIDE, CUNIT_STATUS_COMPLETED);

    let facts =
        iter_cunit_unit_facts(&Layout::X64, &raw, None, None, false, ValidationStatus::StrongCandidate);

    assert_eq!(
        facts[0].production.as_ref().unwrap().is_completed,
        Some(false)
    );
    assert_eq!(
        facts[1].production.as_ref().unwrap().is_completed,
        Some(true)
    );
}

#[test]
fn decodes_in_air_from_validated_cunit_status_bit() {
    let mut raw = vec![0; CUNIT_STRIDE * 2];
    put_cunit_record(&mut raw, 0, 1, 106, CUNIT_ORDER_IDLE, 384000, 0, (320, 640));
    put_cunit_record(
        &mut raw,
        CUNIT_STRIDE,
        1,
        106,
        CUNIT_ORDER_IDLE,
        384000,
        0,
        (352, 640),
    );
    put_cunit_status(&mut raw, CUNIT_STRIDE, CUNIT_STATUS_IN_AIR);

    let facts =
        iter_cunit_unit_facts(&Layout::X64, &raw, None, None, false, ValidationStatus::Validated);

    assert_eq!(facts[0].in_air, Some(false));
    assert_eq!(facts[1].in_air, Some(true));
}

#[test]
fn does_not_emit_progress_for_invalid_production_candidates() {
    let mut raw = vec![0; CUNIT_STRIDE * 3];
    put_cunit_record(
        &mut raw,
        0,
        1,
        111,
        CUNIT_ORDER_RESEARCH,
        15360,
        0,
        (320, 640),
    );
    put_cunit_production(&mut raw, 0, 0, Some(3), None);
    put_cunit_record(
        &mut raw,
        CUNIT_STRIDE,
        1,
        111,
        CUNIT_ORDER_RESEARCH,
        15360,
        0,
        (320, 640),
    );
    put_cunit_production(&mut raw, CUNIT_STRIDE, 1, Some(255), None);
    put_cunit_record(
        &mut raw,
        CUNIT_STRIDE * 2,
        1,
        111,
        CUNIT_ORDER_UPGRADE,
        15360,
        0,
        (320, 640),
    );
    put_cunit_production(&mut raw, CUNIT_STRIDE * 2, 1, None, Some(255));

    let facts =
        iter_cunit_unit_facts(&Layout::X64, &raw, None, None, false, ValidationStatus::StrongCandidate);
    let (upgrades, techs) = derive_cunit_in_progress_facts(&facts);

    assert_eq!(facts.len(), 3);
    assert!(facts.iter().all(|fact| {
        let production = fact.production.as_ref().expect("building status");
        production.tech_type_id.is_none()
            && production.upgrade_type_id.is_none()
    }));
    assert!(upgrades.is_empty());
    assert!(techs.is_empty());
}

#[test]
fn decodes_secondary_alignment() {
    let mut raw = vec![0; CUNIT_STRIDE * 2];
    let secondary_offset = CUNIT_ALIGNMENTS[1];
    put_cunit_record(&mut raw, secondary_offset, 1, 7, 3, 15360, 0, (96, 128));

    let facts =
        iter_cunit_unit_facts(&Layout::X64, &raw, None, None, false, ValidationStatus::StrongCandidate);

    assert_eq!(facts.len(), 1);
    assert_eq!(facts[0].record_index, Some(secondary_offset));
    assert_eq!(facts[0].position, Some(Position::new(96, 128)));
}

#[test]
fn rejects_implausible_records() {
    let cases = [
        (12, 7, 3, 15360, 0, (64, 96)),
        (1, 241, 3, 15360, 0, (64, 96)),
        (1, 7, 3, -1, 0, (64, 96)),
        (1, 7, 3, 15360, 5_000_001, (64, 96)),
        (1, 7, 3, 15360, 0, (9000, 64)),
    ];

    for (owner, unit_type, order, hit_points, shields, position) in cases {
        let mut raw = vec![0; CUNIT_STRIDE];
        put_cunit_record(
            &mut raw, 0, owner, unit_type, order, hit_points, shields, position,
        );
        assert!(iter_cunit_unit_facts(&Layout::X64, 
            &raw,
            None,
            None,
            false,
            ValidationStatus::StrongCandidate
        )
        .is_empty());
    }

    let mut marine = vec![0; CUNIT_STRIDE];
    put_cunit_record(&mut marine, 0, 1, 0, 3, 15360, 0, (64, 96));
    assert_eq!(
        iter_cunit_unit_facts(&Layout::X64, 
            &marine,
            None,
            None,
            false,
            ValidationStatus::StrongCandidate,
        )
        .len(),
        1
    );
}

#[test]
fn constants_match_python_memory_contract() {
    assert_eq!(CUNIT_STRIDE, 0x1e8);
    assert_eq!(CUNIT_READ_WINDOW_BYTES, 0x3d0);
    assert_eq!(CUNIT_ALIGNMENTS, [0x0, 0x1e8]);
    assert_eq!(CUNIT_OWNER_OFFSET, 0x78);
    assert_eq!(CUNIT_ORDER_OFFSET, 0x79);
    assert_eq!(CUNIT_TYPE_OFFSET, 0x9c);
    assert_eq!(CUNIT_VELOCITY_X_OFFSET, 0x68);
    assert_eq!(CUNIT_VELOCITY_Y_OFFSET, 0x6c);
    assert_eq!(CUNIT_PRODUCTION_REMAINING_TIME_OFFSET, 0x122);
    assert_eq!(CUNIT_RESEARCH_TECH_TYPE_OFFSET, 0x124);
    assert_eq!(CUNIT_UPGRADE_TYPE_OFFSET, 0x125);
    assert_eq!(CUNIT_STATUS_FLAGS_OFFSET, 0x150);
    assert_eq!(CUNIT_STIM_TIMER_OFFSET, 0x1b1);
    assert_eq!(CUNIT_STATUS_COMPLETED, 1);
    assert_eq!(CUNIT_STATUS_IN_AIR, 4);
    assert_eq!(CUNIT_ORDER_IDLE, 23);
    assert_eq!(CUNIT_ORDER_BUILDING_IN_PROGRESS, 48);
}

#[test]
fn worker_carrying_follows_cargo_kind_not_gather_slot_byte() {
    // A harvesting worker has the gather-slot byte at 1 and cargo kind 0; a
    // returning worker has it at 0 and kind 1 (gas) or 2 (minerals).
    let worker_resource_type_offset = Layout::X64.cunit.worker_resource_type_offset;

    let mut raw = vec![0; CUNIT_STRIDE * 3];
    for (index, (gather_slot, kind)) in [(1u8, 0u8), (0, 1), (0, 2)].into_iter().enumerate() {
        let offset = CUNIT_STRIDE * index;
        put_cunit_record(&mut raw, offset, 0, 41, CUNIT_ORDER_IDLE, 40 * 256, 0, (1024, 1024));
        put_cunit_status(&mut raw, offset, CUNIT_STATUS_COMPLETED);
        raw[offset + 0x132] = gather_slot;
        raw[offset + worker_resource_type_offset] = kind;
    }

    let facts = iter_cunit_unit_facts_with_queued_orders(&Layout::X64, 
        &raw,
        0x2000_0000,
        None,
        false,
        ValidationStatus::StrongCandidate,
        |_, _| None::<Vec<u8>>,
    );
    let carried: Vec<_> = facts
        .iter()
        .map(|fact| {
            let cunit = fact.cunit.as_ref().expect("cunit fact");
            (cunit.is_carrying_resource, cunit.resource_cargo_kind)
        })
        .collect();
    assert_eq!(
        carried,
        vec![(Some(false), None), (Some(true), Some(1)), (Some(true), Some(2))]
    );
}

#[test]
fn decodes_hatchery_larva_regeneration_train_time() {
    // Official UnitUpdate.cpp (Zerg_Hatchery/Lair/Hive case) publishes
    // remainingTrainTime = larvaTimer * 9 + ((orderQueueTimer + 8) % 9)
    // even though isTraining stays false for zerg resource depots.
    let mut raw = vec![0; CUNIT_STRIDE * 2];
    put_cunit_record(
        &mut raw,
        0,
        1,
        131,
        CUNIT_ORDER_IDLE,
        1250 * 256,
        0,
        (1024, 1024),
    );
    put_cunit_status(&mut raw, 0, CUNIT_STATUS_COMPLETED);
    raw[CUNIT_LARVA_TIMER_OFFSET] = 37;
    raw[CUNIT_ORDER_QUEUE_TIMER_OFFSET] = 4;

    let facts = iter_cunit_unit_facts_with_queued_orders(&Layout::X64, 
        &raw,
        0x2000_0000,
        None,
        false,
        ValidationStatus::StrongCandidate,
        |_, _| None::<Vec<u8>>,
    );

    let hatchery = facts
        .iter()
        .find(|fact| fact.unit_type_id == Some(131))
        .expect("hatchery fact");
    // 37 * 9 + ((4 + 8) % 9) = 333 + 3.
    assert_eq!(
        hatchery
            .production
            .as_ref()
            .and_then(|production| production.remaining_train_frames),
        Some(336)
    );
    // Hatcheries do not use the common build-queue training decoder, so the
    // queue must stay unavailable rather than decoding the building union.
    assert_eq!(
        hatchery
            .production
            .as_ref()
            .and_then(|production| production.training_queue.as_ref().map(|queue| queue.len())),
        None
    );
}

#[test]
fn incomplete_hatchery_keeps_larva_train_time_unavailable() {
    // Construction time is separate from larva regeneration: a newly
    // constructed Hatchery must keep remainingTrainTime unavailable.
    let mut raw = vec![0; CUNIT_STRIDE];
    put_cunit_record(
        &mut raw,
        0,
        1,
        131,
        CUNIT_ORDER_IDLE,
        1250 * 256,
        0,
        (1024, 1024),
    );
    put_cunit_status(&mut raw, 0, 0);
    raw[CUNIT_LARVA_TIMER_OFFSET] = 37;
    raw[CUNIT_ORDER_QUEUE_TIMER_OFFSET] = 4;

    let facts = iter_cunit_unit_facts(&Layout::X64, 
        &raw,
        None,
        None,
        false,
        ValidationStatus::StrongCandidate,
    );

    assert_eq!(
        facts[0]
            .production
            .as_ref()
            .and_then(|production| production.remaining_train_frames),
        None
    );
}

#[test]
fn morphing_depots_keep_larva_timer_when_raw_completed_is_cleared() {
    for (unit_type, target_type, order, slot) in [
        (131, 132u16, CUNIT_ORDER_INCOMPLETE_MORPHING, 0),
        (132, 133, CUNIT_ORDER_INCOMPLETE_MORPHING, 4),
        (131, 132, CUNIT_ORDER_ZERG_BUILDING_MORPH, 1),
        (132, 133, CUNIT_ORDER_ZERG_BUILDING_MORPH, 2),
    ] {
        let mut raw = vec![0; CUNIT_STRIDE];
        put_cunit_record(&mut raw, 0, 1, unit_type, order, 1250 * 256, 0, (1024, 1024));
        // Raw Completed=0, Hatchery->Lair, timer15/qTimer5.
        put_cunit_status(&mut raw, 0, 0x140002);
        raw[CUNIT_BUILD_QUEUE_SLOT_OFFSET] = slot;
        let queue_offset = CUNIT_BUILD_QUEUE_TYPE_OFFSET + usize::from(slot) * 2;
        raw[queue_offset..queue_offset + 2].copy_from_slice(&target_type.to_le_bytes());
        raw[CUNIT_BUILD_UNIT_REMAINING_TIME_OFFSET..][..2]
            .copy_from_slice(&1469u16.to_le_bytes());
        raw[CUNIT_LARVA_TIMER_OFFSET] = 15;
        raw[CUNIT_ORDER_QUEUE_TIMER_OFFSET] = 5;
        let facts = iter_cunit_unit_facts(&Layout::X64, &raw, None, None, false, ValidationStatus::StrongCandidate);
        let production = facts[0].production.as_ref().expect("depot production");
        assert_eq!(production.is_completed, Some(false));
        assert_eq!(production.build_type_id, Some(target_type));
        assert_eq!(production.remaining_train_frames, Some(139));
        assert_eq!(production.remaining_build_frames, Some(1469));
    }
}

#[test]
fn incomplete_depots_require_a_valid_lair_or_hive_morph_for_larva_timer() {
    for (unit_type, target_type, order, slot) in [
        (131, 131u16, CUNIT_ORDER_ZERG_BIRTH, 0),
        (131, 131, CUNIT_ORDER_INCOMPLETE_MORPHING, 0),
        (131, 131, CUNIT_ORDER_ZERG_BUILDING_MORPH, 0),
        (131, 133, CUNIT_ORDER_INCOMPLETE_MORPHING, 0),
        (132, 133, CUNIT_ORDER_IDLE, 0),
        (131, 132, CUNIT_ORDER_INCOMPLETE_MORPHING, 5),
    ] {
        let mut raw = vec![0; CUNIT_STRIDE];
        put_cunit_record(&mut raw, 0, 1, unit_type, order, 1250 * 256, 0, (1024, 1024));
        put_cunit_status(&mut raw, 0, 0x140002);
        raw[CUNIT_BUILD_QUEUE_SLOT_OFFSET] = slot;
        raw[CUNIT_BUILD_QUEUE_TYPE_OFFSET..][..2].copy_from_slice(&target_type.to_le_bytes());
        raw[CUNIT_LARVA_TIMER_OFFSET] = 15;
        raw[CUNIT_ORDER_QUEUE_TIMER_OFFSET] = 5;
        let facts = iter_cunit_unit_facts(&Layout::X64, &raw, None, None, false, ValidationStatus::StrongCandidate);
        assert_eq!(facts[0].production.as_ref().unwrap().remaining_train_frames, None);
    }
}

#[test]
fn morphing_depot_still_caps_larva_timer_at_three_connected_larvae() {
    let base = 0x2000_0000;
    let mut raw = vec![0; CUNIT_STRIDE * 4];
    put_cunit_record(&mut raw, 0, 1, 132, CUNIT_ORDER_INCOMPLETE_MORPHING,
        1800 * 256, 0, (1024, 1024));
    put_cunit_status(&mut raw, 0, 0x140002);
    raw[CUNIT_BUILD_QUEUE_TYPE_OFFSET..][..2].copy_from_slice(&133u16.to_le_bytes());
    raw[CUNIT_BUILD_UNIT_REMAINING_TIME_OFFSET..][..2].copy_from_slice(&1469u16.to_le_bytes());
    raw[CUNIT_LARVA_TIMER_OFFSET] = 15;
    raw[CUNIT_ORDER_QUEUE_TIMER_OFFSET] = 5;
    for index in 1..=3 {
        let offset = CUNIT_STRIDE * index;
        put_cunit_record(&mut raw, offset, 1, 35, CUNIT_ORDER_IDLE, 25 * 256, 0, (1040, 1040));
        put_usize(&mut raw, offset + CUNIT_CONNECTED_UNIT_PTR_OFFSET, base + 0x10);
    }
    let facts = iter_cunit_unit_facts_with_queued_orders_profile(&Layout::X64, 
        &raw, base, None, false, ValidationStatus::Validated, true,
        |_, _| None::<Vec<u8>>,
    );
    let production = facts.iter().find(|fact| fact.unit_type_id == Some(132))
        .unwrap().production.as_ref().unwrap();
    assert_eq!(production.remaining_train_frames, Some(0));
    assert_eq!(production.remaining_build_frames, Some(1469));
}

#[test]
fn hatchery_larva_cap_zeros_train_time_at_three_connected_larva() {
    // Official GameUnits.cpp augmentUnitData clears remainingTrainTime once a
    // hatchery has three connected larva (regeneration stops while full).
    let cunit_base = 0x2000_0000;
    let hatchery_offset = 0;
    let mut raw = vec![0; CUNIT_STRIDE * 6];
    put_cunit_record(
        &mut raw,
        hatchery_offset,
        1,
        133,
        CUNIT_ORDER_IDLE,
        2500 * 256,
        0,
        (1024, 1024),
    );
    put_cunit_status(&mut raw, hatchery_offset, CUNIT_STATUS_COMPLETED);
    raw[hatchery_offset + CUNIT_LARVA_TIMER_OFFSET] = 12;
    raw[hatchery_offset + CUNIT_ORDER_QUEUE_TIMER_OFFSET] = 7;
    for larva_index in 0..3 {
        let larva_offset = CUNIT_STRIDE * (larva_index + 1);
        put_cunit_record(
            &mut raw,
            larva_offset,
            1,
            35,
            CUNIT_ORDER_IDLE,
            25 * 256,
            0,
            (1040, 1040),
        );
        // SCR stores the larva->hatchery link as the selection-table pointer
        // (record + 0x10); the shared target normalizer accepts both forms.
        put_usize(
            &mut raw,
            larva_offset + CUNIT_CONNECTED_UNIT_PTR_OFFSET,
            cunit_base + hatchery_offset + 0x10,
        );
    }

    let facts = iter_cunit_unit_facts_with_queued_orders_profile(&Layout::X64, 
        &raw,
        cunit_base,
        None,
        false,
        ValidationStatus::Validated,
        true,
        |_, _| None::<Vec<u8>>,
    );

    let hatchery = facts
        .iter()
        .find(|fact| fact.unit_type_id == Some(133))
        .expect("hive fact");
    assert_eq!(
        hatchery
            .production
            .as_ref()
            .and_then(|production| production.remaining_train_frames),
        Some(0)
    );

    // Once the third larva loses its parent link only two remain connected,
    // so the raw countdown stays visible:
    // 12 * 9 + ((7 + 8) % 9) = 108 + 6.
    raw[CUNIT_STRIDE * 3 + CUNIT_CONNECTED_UNIT_PTR_OFFSET..][..8].fill(0);
    let facts = iter_cunit_unit_facts_with_queued_orders_profile(&Layout::X64, 
        &raw,
        cunit_base,
        None,
        false,
        ValidationStatus::Validated,
        true,
        |_, _| None::<Vec<u8>>,
    );
    let hatchery = facts
        .iter()
        .find(|fact| fact.unit_type_id == Some(133))
        .expect("hive fact");
    assert_eq!(
        hatchery
            .production
            .as_ref()
            .and_then(|production| production.remaining_train_frames),
        Some(114)
    );
}

#[test]
fn distinguishes_known_empty_queue_from_unreadable_queue_without_losing_unit() {
    let base = 0x1000_0000;
    let mut raw = vec![0; CUNIT_STRIDE];
    put_cunit_record(&mut raw, 0, 1, 7, 6, 15360, 0, (320, 640));
    let known = iter_cunit_unit_facts_with_queued_orders(&Layout::X64, &raw, base, Some(1), false,
        ValidationStatus::Validated, |_, _| None::<Vec<u8>>);
    assert_eq!(known.len(), 1);
    assert_eq!(known[0].order.as_ref().unwrap().queue_count, Some(0));
    assert!(known[0].order.as_ref().unwrap().queued_orders.is_empty());

    raw[CUNIT_ORDER_QUEUE_COUNT_OFFSET] = 1;
    put_usize(&mut raw, CUNIT_ORDER_QUEUE_HEAD_OFFSET, 0x3000_0000);
    put_usize(&mut raw, CUNIT_ORDER_QUEUE_TAIL_OFFSET, 0x3000_0000);
    let unknown = iter_cunit_unit_facts_with_queued_orders(&Layout::X64, &raw, base, Some(1), false,
        ValidationStatus::Validated, |_, _| None::<Vec<u8>>);
    assert_eq!(unknown.len(), 1);
    assert_eq!(unknown[0].position, known[0].position);
    assert_eq!(unknown[0].vitals, known[0].vitals);
    assert_eq!(unknown[0].order.as_ref().unwrap().queue_count, None);
    assert!(unknown[0].order.as_ref().unwrap().queued_orders.is_empty());
}

#[test]
fn x86_layout_decodes_the_same_scenario_records_as_x64() {
    use bwrapi_layout::Layout;
    use bwrapi_internal_model::facts::UnitFact;

    // One logical scenario is encoded twice — once with the x64 pool geometry
    // (0x1e8 stride, 8-byte pointers, +0x10 selection adjustment) and once
    // with the x86 geometry (0x150 stride, 4-byte pointers, no
    // adjustment) — and the single layout-driven reader path must produce the
    // same facts for both bitnesses.
    fn scenario(layout: &Layout) -> Vec<UnitFact> {
        let cunit = &layout.cunit;
        let base = 0x2000_0000;
        let node = 0x3000_0000;
        let ptr_bytes = layout.pointer_bytes;
        fn put_u16(raw: &mut [u8], offset: usize, value: u16) {
            raw[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
        }
        fn put_u32(raw: &mut [u8], offset: usize, value: u32) {
            raw[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        fn put_i32(raw: &mut [u8], offset: usize, value: i32) {
            raw[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        fn put_ptr(raw: &mut [u8], offset: usize, value: usize, pointer_bytes: usize) {
            match pointer_bytes {
                8 => raw[offset..offset + 8].copy_from_slice(&(value as u64).to_le_bytes()),
                _ => raw[offset..offset + 4].copy_from_slice(&(value as u32).to_le_bytes()),
            }
        }

        let mut raw = vec![0u8; cunit.record_stride * 2];
        // Record 0: a live Terran SCV ordering at record 1 with one queued move.
        raw[cunit.owner_offset] = 1;
        raw[cunit.order_offset] = 3;
        put_u16(&mut raw, cunit.type_offset, 7);
        put_i32(&mut raw, cunit.hit_points_offset, 15360);
        put_u32(&mut raw, cunit.shields_offset, 0);
        put_u32(&mut raw, cunit.status_flags_offset, CUNIT_STATUS_COMPLETED);
        put_u32(&mut raw, cunit.detection_player_flags_offset, u32::MAX);
        put_u16(&mut raw, cunit.position_x_offset, 64);
        put_u16(&mut raw, cunit.position_y_offset, 96);
        raw[cunit.movement_flags_offset] = CUNIT_MOVEMENT_FLAG_MOVING;
        raw[cunit.order_timer_offset] = 7;
        put_u16(&mut raw, cunit.order_target_position_x_offset, 32);
        put_u16(&mut raw, cunit.order_target_position_y_offset, 48);
        put_u16(&mut raw, cunit.move_target_position_x_offset, 64);
        put_u16(&mut raw, cunit.move_target_position_y_offset, 96);
        put_u16(&mut raw, cunit.next_movement_waypoint_x_offset, 40);
        put_u16(&mut raw, cunit.next_movement_waypoint_y_offset, 56);
        put_u16(&mut raw, cunit.next_target_waypoint_x_offset, 48);
        put_u16(&mut raw, cunit.next_target_waypoint_y_offset, 64);
        // The order target uses the selection-table form each pool stores:
        // record +0x10 on x64, the direct record pointer on x86.
        put_ptr(
            &mut raw,
            cunit.order_target_unit_ptr_offset,
            base + cunit.record_stride + cunit.selection_ptr_adjustment,
            ptr_bytes,
        );
        put_ptr(
            &mut raw,
            cunit.move_target_unit_ptr_offset,
            base + cunit.record_stride,
            ptr_bytes,
        );
        raw[cunit.ground_weapon_cooldown_offset] = 1;
        raw[cunit.air_weapon_cooldown_offset] = 2;
        raw[cunit.spell_cooldown_offset] = 3;
        raw[cunit.kill_count_offset] = 1;
        raw[cunit.last_attacking_player_id_offset] = 8;
        raw[cunit.rank_increase_offset] = 2;
        raw[cunit.secondary_order_timer_offset] = 4;
        raw[cunit.order_queue_timer_offset] = 3;
        put_u16(&mut raw, cunit.button_set_id_offset, 7);
        raw[cunit.worker_resource_type_offset] = 1;
        // The x86 worker-union carry-count byte (0xaf) is the high byte of the
        // adjacent previous-hit-points u16 (0xae); keep both layouts on a
        // combination that can exist instead of an impossible overlap.
        put_u16(&mut raw, cunit.previous_hit_points_raw_offset, 1);
        raw[cunit.current_direction_1_offset] = 104;
        raw[cunit.turn_radius_offset] = 40;
        raw[cunit.velocity_direction_offset] = 104;
        put_u16(&mut raw, cunit.flingy_id_offset, 5);
        raw[cunit.flingy_movement_type_offset] = 0;
        put_u32(&mut raw, cunit.flingy_top_speed_raw_offset, 1280);
        put_i32(&mut raw, cunit.velocity_x_offset, 16);
        put_i32(&mut raw, cunit.velocity_y_offset, -24);
        put_u16(&mut raw, cunit.flingy_acceleration_offset, 100);
        // Profile-gated closed status tail.
        put_u16(&mut raw, cunit.remove_timer_offset, 9);
        put_u16(&mut raw, cunit.defense_matrix_damage_offset, 7);
        raw[cunit.stim_timer_offset] = 5;
        raw[cunit.lockdown_timer_offset] = 6;
        raw[cunit.parasite_flags_offset] = 1;

        // Record 1: a live Terran Command Center holding a rally point.
        let cc = cunit.record_stride;
        raw[cc + cunit.owner_offset] = 1;
        raw[cc + cunit.order_offset] = 23;
        put_u16(&mut raw, cc + cunit.type_offset, 106);
        put_i32(&mut raw, cc + cunit.hit_points_offset, 384000);
        put_u32(&mut raw, cc + cunit.status_flags_offset, CUNIT_STATUS_COMPLETED);
        put_u16(&mut raw, cc + cunit.position_x_offset, 512);
        put_u16(&mut raw, cc + cunit.position_y_offset, 640);
        put_u16(&mut raw, cc + cunit.rally_position_x_offset, 512);
        put_u16(&mut raw, cc + cunit.rally_position_y_offset, 640);

        // One-node COrder queue encoded with the layout's COrder offsets.
        let mut corder = vec![0u8; layout.corder.read_bytes];
        put_ptr(&mut corder, layout.corder.prev_offset, 0, ptr_bytes);
        put_ptr(&mut corder, layout.corder.next_offset, 0, ptr_bytes);
        put_u16(&mut corder, layout.corder.order_offset, 6);
        put_u16(&mut corder, layout.corder.unit_type_offset, 228);
        put_u16(&mut corder, layout.corder.target_x_offset, 352);
        put_u16(&mut corder, layout.corder.target_y_offset, 672);
        put_ptr(&mut corder, layout.corder.target_unit_ptr_offset, 0, ptr_bytes);
        raw[cunit.order_queue_count_offset] = 1;
        put_ptr(&mut raw, cunit.order_queue_head_offset, node, ptr_bytes);
        put_ptr(&mut raw, cunit.order_queue_tail_offset, node, ptr_bytes);

        iter_cunit_unit_facts_with_queued_orders_profile(
            layout,
            &raw,
            base,
            None,
            false,
            ValidationStatus::Validated,
            true,
            |address, size| {
                if address == node && size == layout.corder.read_bytes {
                    Some(corder.clone())
                } else if address == base && size == cunit.scan_min_bytes {
                    Some(raw[..cunit.scan_min_bytes].to_vec())
                } else {
                    None
                }
            },
        )
    }

    // Records and tags carry pool offsets, which differ by stride; normalize
    // both to per-pool ordinals before comparing the decoded facts.
    fn retag(tag: &Option<String>, stride: usize) -> Option<String> {
        tag.as_ref().map(|tag| {
            tag.rsplit(':')
                .next()
                .and_then(|offset| offset.parse::<usize>().ok())
                .map_or_else(|| tag.clone(), |offset| format!("cunit:record:{}", offset / stride))
        })
    }
    fn normalize(mut fact: UnitFact, stride: usize) -> UnitFact {
        fact.source_address = None;
        fact.record_index = fact.record_index.map(|offset| offset / stride);
        if let Some(order) = fact.order.as_mut() {
            order.target_unit_tag = retag(&order.target_unit_tag, stride);
            order.move_target_unit_tag = retag(&order.move_target_unit_tag, stride);
            order.rally_unit_tag = retag(&order.rally_unit_tag, stride);
        }
        if let Some(cunit) = fact.cunit.as_mut() {
            cunit.sub_unit_tag = retag(&cunit.sub_unit_tag, stride);
            cunit.resource_target_unit_tag = retag(&cunit.resource_target_unit_tag, stride);
            cunit.power_up_unit_tag = retag(&cunit.power_up_unit_tag, stride);
            cunit.bunker_unit_tag = retag(&cunit.bunker_unit_tag, stride);
            cunit.nydus_exit_unit_tag = retag(&cunit.nydus_exit_unit_tag, stride);
            cunit.permanent_addon_unit_tag = retag(&cunit.permanent_addon_unit_tag, stride);
            cunit.hangar_parent_unit_tag = retag(&cunit.hangar_parent_unit_tag, stride);
        }
        if let Some(production) = fact.production.as_mut() {
            production.addon_unit_tag = retag(&production.addon_unit_tag, stride);
            production.build_unit_tag = retag(&production.build_unit_tag, stride);
        }
        fact.transport_unit_tag = retag(&fact.transport_unit_tag, stride);
        fact
    }

    let x64: Vec<_> = scenario(&Layout::X64)
        .into_iter()
        .map(|fact| normalize(fact, Layout::X64.cunit.record_stride))
        .collect();
    let x86: Vec<_> = scenario(&Layout::X86)
        .into_iter()
        .map(|fact| normalize(fact, Layout::X86.cunit.record_stride))
        .collect();

    assert_eq!(x64.len(), 2);
    assert_eq!(x86.len(), 2);
    assert_eq!(
        x64[0]
            .order
            .as_ref()
            .and_then(|order| order.target_unit_tag.as_deref()),
        Some("cunit:record:1")
    );
    assert_eq!(
        x86[0]
            .order
            .as_ref()
            .and_then(|order| order.target_unit_tag.as_deref()),
        Some("cunit:record:1")
    );
    assert_eq!(x64, x86);
}
