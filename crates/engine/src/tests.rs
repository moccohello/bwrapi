use bwrapi_internal_model::adapted::ValidationStatus;
use bwrapi_internal_model::facts::UnitFact;
use bwrapi_layout::{BulletLayout, Layout};
use bwrapi_memory::{InProcessMemory, ProcessModule};

use crate::decode::cunit::{
    iter_cunit_unit_facts, iter_cunit_unit_facts_with_queued_orders_profile,
    CUNIT_ORDER_INCOMPLETE_BUILDING, CUNIT_ORDER_RESEARCH, CUNIT_ORDER_TRAIN,
    CUNIT_ORDER_UPGRADE, CUNIT_STATUS_COMPLETED,
    CUNIT_STATUS_IN_BUILDING, CUNIT_STATUS_IN_TRANSPORT,
};
use crate::fake::{in_game, put_unit, write_in_game_image, write_unit, FakeMemory, Shared, POOL, ROOT};
use crate::columns::{Cell, Table};
use crate::decode::bwgame::*;
use crate::decode::map::BWGAME_MAP_WIDTH_OFFSET;
use crate::tables::{source, UnitRow};
use crate::{frame_flag, unit_slot, Engine, Error, Frame};

fn engine(memory: &std::sync::Arc<FakeMemory>, sources: u64) -> Engine {
    let mut engine = Engine::with_memory(Box::new(Shared(std::sync::Arc::clone(memory)))).unwrap();
    engine.subscribe(sources);
    engine
}

fn put_ptr(memory: &FakeMemory, slot: usize, field: usize, target_slot: usize) {
    let c = &Layout::X64.cunit;
    let target = (POOL + target_slot * c.record_stride) as u64;
    memory.patch(POOL + slot * c.record_stride + field, &target.to_le_bytes());
}

fn row_of(frame: &Frame, unit_type: u16) -> UnitRow {
    let index = frame
        .units
        .unit_type
        .iter()
        .position(|found| *found == unit_type)
        .expect("unit type in frame");
    frame.units.row(index).expect("row")
}

fn reference_facts(memory: &FakeMemory, slots: usize) -> Vec<UnitFact> {
    let c = &Layout::X64.cunit;
    let raw = memory.read_exact(POOL, slots * c.record_stride).unwrap();
    iter_cunit_unit_facts(&Layout::X64, &raw, Some(POOL), None, false, ValidationStatus::Validated)
}

/// The reference decoder under the direct-status profile the engine reads.
fn profile_facts(memory: &FakeMemory, slots: usize) -> Vec<UnitFact> {
    let c = &Layout::X64.cunit;
    let raw = memory.read_exact(POOL, slots * c.record_stride).unwrap();
    iter_cunit_unit_facts_with_queued_orders_profile(
        &Layout::X64,
        &raw,
        POOL,
        None,
        false,
        ValidationStatus::Validated,
        true,
        |address, size| memory.read_exact(address, size).ok(),
    )
}

#[test]
fn outside_a_match_acquire_reports_not_in_game_and_a_dead_process_gone() {
    let memory = in_game(4);
    memory.patch(memory.module.base_address + Layout::X64.scalars.session_is_in_game_offset, &0u32.to_le_bytes());
    assert_eq!(engine(&memory, source::UNITS).acquire().err(), Some(Error::NotInGame));

    let empty = std::sync::Arc::new(FakeMemory::new());
    empty.kill();
    assert_eq!(engine(&empty, source::UNITS).acquire().err(), Some(Error::Gone));
}

#[test]
fn units_are_decoded_into_columns_with_stable_ids() {
    let memory = in_game(8);
    put_unit(&memory, 2, 7, 0, 100, 200, 5);
    put_unit(&memory, 5, 37, 1, 300, 400, 9);
    let mut engine = engine(&memory, source::UNITS);

    let frame = engine.acquire().unwrap();
    assert_eq!(frame.units.unit_type, [7, 37]);
    assert_eq!(frame.units.owner, [0, 1]);
    assert_eq!(frame.units.x, [100, 300]);
    assert_eq!(frame.units.y, [200, 400]);
    assert_eq!(frame.units.id.iter().map(|id| unit_slot(*id)).collect::<Vec<_>>(), [2, 5]);
    let first_ids = frame.units.id.clone();
    drop(frame);

    // Slot 5 is reused by a new unit: new id; slot 2 keeps its id.
    put_unit(&memory, 5, 37, 1, 300, 400, 10);
    let frame = engine.acquire().unwrap();
    assert_eq!(frame.units.id[0], first_ids[0]);
    assert_ne!(frame.units.id[1], first_ids[1]);
}

#[test]
fn unsubscribed_sources_are_not_read() {
    let memory = in_game(8);
    put_unit(&memory, 0, 7, 0, 1, 1, 1);
    let mut engine = engine(&memory, source::VIEWPORT);
    let frame = engine.acquire().unwrap();
    assert!(frame.units.id.is_empty());
    assert_eq!(memory.reads_in(POOL, 8 * Layout::X64.cunit.record_stride), 0);
}

/// Native Unit pointer of `slot`: CUnit record + selection adjustment.
fn native_unit(slot: usize) -> u64 {
    let c = &Layout::X64.cunit;
    (POOL + slot * c.record_stride + c.selection_ptr_adjustment) as u64
}

/// Chains `slots` through the record links at `prev`/`next`.
fn link_native_list(memory: &FakeMemory, slots: &[usize], prev: usize, next: usize) {
    let stride = Layout::X64.cunit.record_stride;
    for (position, slot) in slots.iter().enumerate() {
        let record = POOL + slot * stride;
        let previous = if position == 0 { 0 } else { native_unit(slots[position - 1]) };
        let following = slots.get(position + 1).map_or(0, |next| native_unit(*next));
        memory.patch(record + prev, &previous.to_le_bytes());
        memory.patch(record + next, &following.to_le_bytes());
    }
}

/// Publishes the game's live-unit lists: active, hidden, dying, revealer
/// (first/last in the native list header block), invisible, pylon (first
/// pointers at their module RVAs). The header block also carries the pool
/// array-pointer and capacity globals inside it, so their values are
/// rewritten into the block the header range shadows.
fn put_live_unit_lists(memory: &FakeMemory, slots: usize, header_lists: [&[usize]; 4], invisible: &[usize], pylon: &[usize]) {
    let c = &Layout::X64.cunit;
    let lists = c.unit_lists.expect("x64 native unit lists");
    memory.put_module(c.native_unit_list_header_module_offset, &vec![0; c.native_unit_list_header_bytes]);
    let header = memory.module.base_address + c.native_unit_list_header_module_offset;
    let inside = |rva: usize| header + (rva - c.native_unit_list_header_module_offset);
    memory.patch(inside(c.array_pointer_rva), &((POOL + c.selection_ptr_adjustment) as u64).to_le_bytes());
    memory.patch(inside(c.capacity_pointer_rva), &(slots as u32).to_le_bytes());
    for (index, slots) in header_lists.iter().enumerate() {
        link_native_list(memory, slots, lists.header_list_prev_link_offset, lists.header_list_next_link_offset);
        let head = slots.first().map_or(0, |slot| native_unit(*slot));
        let tail = slots.last().map_or(0, |slot| native_unit(*slot));
        memory.patch(header + lists.header_list_first_offsets[index], &head.to_le_bytes());
        memory.patch(header + lists.header_list_last_offsets[index], &tail.to_le_bytes());
    }
    for (slots, first, prev, next) in [
        (invisible, lists.invisible_list_first_module_rva, lists.invisible_list_prev_link_offset, lists.invisible_list_next_link_offset),
        (pylon, lists.pylon_list_first_module_rva, lists.pylon_list_prev_link_offset, lists.pylon_list_next_link_offset),
    ] {
        link_native_list(memory, slots, prev, next);
        let head = slots.first().map_or(0, |slot| native_unit(*slot));
        memory.put_module(first, &head.to_le_bytes());
    }
}

#[test]
fn the_pool_read_covers_only_bands_of_listed_units() {
    let memory = in_game(64);
    put_unit(&memory, 1, 7, 0, 10, 10, 1);
    put_unit(&memory, 3, 7, 0, 20, 20, 2);
    put_unit(&memory, 10, 7, 1, 30, 30, 3);
    put_unit(&memory, 20, 7, 1, 40, 40, 4);
    // Live-looking records no list reaches: one inside the band the walk
    // reads, one past it; neither may become a row.
    put_unit(&memory, 2, 7, 0, 15, 15, 9);
    put_unit(&memory, 30, 7, 0, 50, 50, 5);
    put_live_unit_lists(&memory, 64, [&[1, 20, 3], &[], &[], &[]], &[10], &[]);
    let mut engine = engine(&memory, source::UNITS);
    let frame = engine.acquire().unwrap();
    let slots: Vec<u16> = frame.units.id.iter().map(|id| unit_slot(*id)).collect();
    assert_eq!(slots, [1, 3, 10, 20]);
    let stride = Layout::X64.cunit.record_stride;
    let reads = memory.reads.lock().unwrap();
    let pool_reads: Vec<&(usize, usize)> =
        reads.iter().filter(|(address, _)| (POOL..POOL + 64 * stride).contains(address)).collect();
    // One band read covers every listed slot and stops well short of the pool.
    assert_eq!(pool_reads.len(), 1);
    assert_eq!(pool_reads[0], &(POOL + 1 * stride, 32 * stride));
}

#[test]
fn a_broken_list_makes_the_read_take_the_whole_pool() {
    let memory = in_game(64);
    put_unit(&memory, 1, 7, 0, 10, 10, 1);
    put_unit(&memory, 3, 7, 0, 20, 20, 2);
    put_unit(&memory, 30, 7, 0, 50, 50, 5);
    put_live_unit_lists(&memory, 64, [&[1, 3], &[], &[], &[]], &[], &[]);
    // The chain's second node does not name the first as its previous unit.
    let links = Layout::X64.cunit.unit_lists.expect("x64 native unit lists");
    memory.patch(POOL + 3 * Layout::X64.cunit.record_stride + links.header_list_prev_link_offset, &native_unit(7).to_le_bytes());
    let mut engine = engine(&memory, source::UNITS);
    let frame = engine.acquire().unwrap();
    let slots: Vec<u16> = frame.units.id.iter().map(|id| unit_slot(*id)).collect();
    assert_eq!(slots, [1, 3, 30]);
    let stride = Layout::X64.cunit.record_stride;
    let reads = memory.reads.lock().unwrap();
    assert_eq!(reads.iter().filter(|(address, size)| *address == POOL && *size == 64 * stride).count(), 1);
}

#[test]
fn lists_that_name_no_unit_cannot_bound_the_pool() {
    let memory = in_game(8);
    put_unit(&memory, 2, 7, 0, 10, 10, 1);
    put_live_unit_lists(&memory, 8, [&[], &[], &[], &[]], &[], &[]);
    let mut engine = engine(&memory, source::UNITS);
    let frame = engine.acquire().unwrap();
    // No listed unit proves nothing, so the whole pool is read and scanned.
    let slots: Vec<u16> = frame.units.id.iter().map(|id| unit_slot(*id)).collect();
    assert_eq!(slots, [2]);
    let stride = Layout::X64.cunit.record_stride;
    assert_eq!(memory.reads.lock().unwrap().iter().any(|(address, size)| *address == POOL && *size == 8 * stride), true);
}

#[test]
fn a_new_root_starts_a_new_match() {
    let memory = in_game(4);
    let mut engine = engine(&memory, 0);
    let first = engine.acquire().unwrap().match_id;
    assert_eq!(engine.acquire().unwrap().match_id, first);
    let layout = &Layout::X64;
    memory.put(ROOT + 0x10_0000, &vec![0; crate::decode::bwgame::BWGAME_ROOT_READ_BYTES]);
    memory.patch(
        memory.module.base_address + layout.menu_root.pointer,
        &((ROOT + 0x10_0000) as u64 ^ layout.menu_root.xor_key).to_le_bytes(),
    );
    assert_eq!(engine.acquire().unwrap().match_id, first + 1);
}

#[test]
fn held_frames_are_not_reused_and_released_frames_are() {
    let memory = in_game(4);
    let mut engine = engine(&memory, source::UNITS);
    let held = engine.acquire().unwrap();
    let next = engine.acquire().unwrap();
    assert!(!std::sync::Arc::ptr_eq(&held, &next));
    assert_eq!(next.sequence, held.sequence + 1);
    let address = std::sync::Arc::as_ptr(&next);
    drop(next);
    assert_eq!(std::sync::Arc::as_ptr(&engine.acquire().unwrap()), address);
    assert_eq!(held.flags & frame_flag::TORN, 0);
}

#[test]
fn a_bunker_passenger_is_loaded_and_names_the_bunker() {
    let memory = in_game(8);
    let c = &Layout::X64.cunit;
    put_unit(&memory, 1, 125, 0, 500, 500, 7);
    put_unit(&memory, 2, 0, 0, 500, 500, 3);
    memory.patch(POOL + c.record_stride + c.status_flags_offset, &CUNIT_STATUS_COMPLETED.to_le_bytes());
    memory.patch(
        POOL + 2 * c.record_stride + c.status_flags_offset,
        &(CUNIT_STATUS_IN_BUILDING | CUNIT_STATUS_IN_TRANSPORT).to_le_bytes(),
    );
    put_ptr(&memory, 2, c.connected_unit_ptr_offset, 1);

    let mut engine = engine(&memory, source::UNITS);
    let frame = engine.acquire().unwrap();
    let bunker = row_of(&frame, 125);
    let marine = row_of(&frame, 0);
    assert_eq!(marine.loaded, 1);
    assert_eq!(marine.transport, bunker.id);
    assert_eq!(bunker.loaded, 0);
    assert_eq!(bunker.transport, 0);

    let facts = reference_facts(&memory, 8);
    let marine_fact = facts.iter().find(|fact| fact.unit_type_id == Some(0)).unwrap();
    assert_eq!(marine_fact.is_loaded, Some(true));
    assert_eq!(marine_fact.transport_unit_tag, Some(format!("cunit:record:{}", c.record_stride)));
}

#[test]
fn an_attached_addon_and_one_under_construction_name_the_addon() {
    let memory = in_game(8);
    let c = &Layout::X64.cunit;
    put_unit(&memory, 0, 106, 0, 100, 100, 1);
    put_unit(&memory, 1, 107, 0, 200, 100, 2);
    put_unit(&memory, 2, 113, 0, 300, 100, 3);
    put_unit(&memory, 3, 120, 0, 400, 100, 4);
    put_ptr(&memory, 0, c.building_addon_ptr_offset, 1);
    put_ptr(&memory, 2, c.build_unit_ptr_offset, 3);

    let mut engine = engine(&memory, source::UNITS);
    let frame = engine.acquire().unwrap();
    assert_eq!(row_of(&frame, 106).addon, row_of(&frame, 107).id);
    assert_eq!(row_of(&frame, 113).addon, row_of(&frame, 120).id);
    assert_eq!(row_of(&frame, 107).addon, 0);

    let facts = reference_facts(&memory, 8);
    let center_fact = facts.iter().find(|fact| fact.unit_type_id == Some(106)).unwrap();
    let factory_fact = facts.iter().find(|fact| fact.unit_type_id == Some(113)).unwrap();
    assert_eq!(
        center_fact.cunit.as_ref().unwrap().permanent_addon_unit_tag,
        Some(format!("cunit:record:{}", c.record_stride))
    );
    assert_eq!(
        factory_fact.production.as_ref().unwrap().addon_unit_tag,
        Some(format!("cunit:record:{}", 3 * c.record_stride))
    );
}

#[test]
fn a_larva_names_its_hatchery() {
    let memory = in_game(4);
    let c = &Layout::X64.cunit;
    put_unit(&memory, 0, 35, 5, 100, 100, 1);
    put_unit(&memory, 1, 131, 5, 100, 120, 2);
    put_ptr(&memory, 0, c.connected_unit_ptr_offset, 1);

    let mut engine = engine(&memory, source::UNITS);
    let frame = engine.acquire().unwrap();
    let larva = row_of(&frame, 35);
    assert_eq!(larva.hatchery, row_of(&frame, 131).id);
    assert_eq!(row_of(&frame, 131).hatchery, 0);
    assert_eq!(larva.loaded, 0);
    assert_eq!(larva.transport, 0);
}

#[test]
fn an_idle_unit_builds_nothing_and_has_no_train_time() {
    let memory = in_game(8);
    put_unit(&memory, 0, 41, 0, 100, 100, 1);
    put_unit(&memory, 1, 111, 0, 200, 200, 2);
    memory.patch(POOL + Layout::X64.cunit.record_stride + Layout::X64.cunit.status_flags_offset, &CUNIT_STATUS_COMPLETED.to_le_bytes());

    let mut engine = engine(&memory, source::UNITS);
    let frame = engine.acquire().unwrap();
    for unit_type in [41, 111] {
        let row = row_of(&frame, unit_type);
        assert_eq!(row.build_type, 228);
        assert_eq!(row.remaining_train_frames, 0);
    }
}

#[test]
fn a_barracks_training_a_marine_reports_the_queue_and_countdowns() {
    let memory = in_game(8);
    let c = &Layout::X64.cunit;
    put_unit(&memory, 0, 111, 0, 100, 100, 1);
    put_unit(&memory, 1, 0, 0, 100, 140, 2);
    memory.patch(POOL + c.status_flags_offset, &CUNIT_STATUS_COMPLETED.to_le_bytes());
    memory.patch(POOL + c.build_queue_slot_offset, &0u8.to_le_bytes());
    memory.patch(POOL + c.build_queue_type_offset, &0u16.to_le_bytes());
    memory.patch(POOL + c.build_queue_type_offset + 2, &1u16.to_le_bytes());
    for slot in 2..5 {
        memory.patch(POOL + c.build_queue_type_offset + slot * 2, &228u16.to_le_bytes());
    }
    memory.patch(POOL + c.secondary_order_offset, &CUNIT_ORDER_TRAIN.to_le_bytes());
    put_ptr(&memory, 0, c.build_unit_ptr_offset, 1);
    memory.patch(POOL + c.record_stride + c.build_unit_remaining_time_offset, &300u16.to_le_bytes());

    let mut engine = engine(&memory, source::UNITS);
    let frame = engine.acquire().unwrap();
    let barracks = row_of(&frame, 111);
    assert_eq!(barracks.build_type, 0);
    // Construction and training countdowns are separate columns.
    assert_eq!(barracks.remaining_build_frames, 0);
    assert_eq!(barracks.remaining_train_frames, 300);
    assert_eq!((barracks.train_queue_0, barracks.train_queue_1), (0, 1));
    assert_eq!(
        (barracks.train_queue_2, barracks.train_queue_3, barracks.train_queue_4),
        (u16::UNKNOWN, u16::UNKNOWN, u16::UNKNOWN)
    );
    assert_eq!(barracks.researching, 0);
    assert_eq!(barracks.upgrading, 0);

    let facts = profile_facts(&memory, 8);
    let production = facts
        .iter()
        .find(|fact| fact.unit_type_id == Some(111))
        .unwrap()
        .production
        .as_ref()
        .unwrap();
    assert_eq!(production.training_queue, Some(vec![0, 1]));
    assert_eq!(production.remaining_train_frames, Some(300));
    assert_eq!(production.remaining_build_frames, Some(0));
}

#[test]
fn research_and_upgrade_orders_set_their_flags() {
    let memory = in_game(4);
    let c = &Layout::X64.cunit;
    put_unit(&memory, 0, 111, 0, 100, 100, 1);
    put_unit(&memory, 1, 111, 0, 200, 100, 2);
    memory.patch(POOL + c.status_flags_offset, &CUNIT_STATUS_COMPLETED.to_le_bytes());
    memory.patch(POOL + c.order_offset, &CUNIT_ORDER_RESEARCH.to_le_bytes());
    memory.patch(POOL + c.production_remaining_time_offset, &200u16.to_le_bytes());
    memory.patch(POOL + c.research_tech_type_offset, &1u8.to_le_bytes());
    memory.patch(POOL + c.record_stride + c.status_flags_offset, &CUNIT_STATUS_COMPLETED.to_le_bytes());
    memory.patch(POOL + c.record_stride + c.order_offset, &CUNIT_ORDER_UPGRADE.to_le_bytes());
    memory.patch(POOL + c.record_stride + c.production_remaining_time_offset, &150u16.to_le_bytes());
    memory.patch(POOL + c.record_stride + c.upgrade_type_offset, &5u8.to_le_bytes());

    let mut engine = engine(&memory, source::UNITS);
    let frame = engine.acquire().unwrap();
    assert_eq!(frame.units.researching, [1, 0]);
    assert_eq!(frame.units.upgrading, [0, 1]);
    assert_eq!(frame.units.tech, [1, u8::UNKNOWN]);
    assert_eq!(frame.units.remaining_research_frames, [200, u32::UNKNOWN]);
    assert_eq!(frame.units.upgrade, [u8::UNKNOWN, 5]);
    assert_eq!(frame.units.remaining_upgrade_frames, [u32::UNKNOWN, 150]);

    let facts = reference_facts(&memory, 4);
    let researching = facts
        .iter()
        .find(|fact| fact.record_index == Some(0))
        .unwrap()
        .production
        .as_ref()
        .unwrap();
    assert_eq!(researching.tech_type_id, Some(1));
    assert_eq!(researching.remaining_research_frames, Some(200));
    let upgrading = facts
        .iter()
        .find(|fact| fact.record_index == Some(c.record_stride))
        .unwrap()
        .production
        .as_ref()
        .unwrap();
    assert_eq!(upgrading.upgrade_type_id, Some(5));
    assert_eq!(upgrading.remaining_upgrade_frames, Some(150));
}

#[test]
fn an_unfinished_building_reports_its_own_type_and_countdown() {
    let memory = in_game(4);
    let c = &Layout::X64.cunit;
    put_unit(&memory, 0, 131, 5, 100, 100, 1);
    memory.patch(POOL + c.order_offset, &CUNIT_ORDER_INCOMPLETE_BUILDING.to_le_bytes());
    memory.patch(POOL + c.build_unit_remaining_time_offset, &400u16.to_le_bytes());

    let mut engine = engine(&memory, source::UNITS);
    let frame = engine.acquire().unwrap();
    let hatchery = row_of(&frame, 131);
    assert_eq!(hatchery.build_type, 131);
    assert_eq!(hatchery.remaining_build_frames, 400);
    assert_eq!(hatchery.completed, 0);

    let facts = reference_facts(&memory, 4);
    let production = facts
        .iter()
        .find(|fact| fact.unit_type_id == Some(131))
        .unwrap()
        .production
        .as_ref()
        .unwrap();
    assert_eq!(production.build_type_id, Some(131));
    assert_eq!(production.remaining_build_frames, Some(400));
}

const ALL: u64 = source::UPGRADES | source::TECHS | source::BULLETS | source::VISION;
const TILES: usize = 0x4_0000_0000;
const SPRITE: usize = 0x5_0000_0000;
const BULLET_POOL: usize = 0x6_0000_0000;
const UNIT_SPRITE: usize = 0x8_0000_0000;
const UNIT_IMAGE: usize = 0x8_0001_0000;

/// Marks the eight identity slots of `memory` as players with the slot's id.
fn all_players_active(memory: &FakeMemory) {
    use crate::decode::scalars::PLAYER_IDENTITY_ENTRY_STRIDE;
    let mut identities = vec![0u8; 8 * PLAYER_IDENTITY_ENTRY_STRIDE];
    for (slot, entry) in identities.chunks_exact_mut(PLAYER_IDENTITY_ENTRY_STRIDE).enumerate() {
        entry[..4].copy_from_slice(&(slot as u32).to_le_bytes());
        entry[8] = 1;
    }
    memory.put_module(Layout::X64.scalars.player_identity_table_offset, &identities);
}

#[test]
fn upgrades_and_techs_are_decoded_per_player() {
    let memory = in_game(4);
    all_players_active(&memory);
    // Player 2: SC upgrade 5 at level 1/3 in progress; BW upgrade 46+6 at 2/3. Player 0 has none.
    memory.patch(ROOT + BWGAME_CURRENT_UPGRADE_SC_OFFSET + 2 * BWGAME_SC_UPGRADE_COUNT + 5, &[1]);
    memory.patch(ROOT + BWGAME_MAX_UPGRADE_SC_OFFSET + 2 * BWGAME_SC_UPGRADE_COUNT + 5, &[3]);
    memory.patch(ROOT + BWGAME_UPGRADE_IN_PROGRESS_OFFSET + 2 * BWGAME_UPGRADE_IN_PROGRESS_STRIDE, &[1 << 5]);
    memory.patch(ROOT + BWGAME_CURRENT_UPGRADE_BW_OFFSET + 2 * BWGAME_BW_UPGRADE_COUNT + 6, &[2]);
    memory.patch(ROOT + BWGAME_MAX_UPGRADE_BW_OFFSET + 2 * BWGAME_BW_UPGRADE_COUNT + 6, &[3]);
    // Player 2: SC tech 4 researched and available; BW tech 24+3 available and being researched.
    memory.patch(ROOT + BWGAME_TECH_RESEARCHED_SC_OFFSET + 2 * BWGAME_SC_TECH_COUNT + 4, &[1]);
    memory.patch(ROOT + BWGAME_TECH_AVAILABLE_SC_OFFSET + 2 * BWGAME_SC_TECH_COUNT + 4, &[1]);
    memory.patch(ROOT + BWGAME_TECH_AVAILABLE_BW_OFFSET + 2 * BWGAME_BW_TECH_COUNT + 3, &[1]);
    memory.patch(ROOT + BWGAME_TECH_RESEARCHING_BITSET_OFFSET + 2 * BWGAME_TECH_BITSET_STRIDE + 3, &[1 << 3]);
    // Player 2: SC tech 2 being researched, in the same bitset.
    memory.patch(ROOT + BWGAME_TECH_RESEARCHING_BITSET_OFFSET + 2 * BWGAME_TECH_BITSET_STRIDE, &[1 << 2]);
    let frame = engine(&memory, ALL).acquire().unwrap();

    let upgrades = &frame.upgrades;
    assert_eq!(upgrades.player, [2, 2]);
    assert_eq!(upgrades.upgrade_type, [5, 52]);
    assert_eq!(upgrades.level, [1, 2]);
    assert_eq!(upgrades.max_level, [3, 3]);
    assert_eq!(upgrades.in_progress, [1, 0]);
    assert_eq!(upgrades.remaining_frames, [u32::MAX, 0]);

    let techs = &frame.techs;
    assert_eq!(techs.rows(), 8 * BWGAME_TOTAL_TECH_COUNT);
    let row = |player: usize, tech: usize| techs.row(player * BWGAME_TOTAL_TECH_COUNT + tech).unwrap();
    let state = |player, tech| {
        let row = row(player, tech);
        (row.researched, row.available, row.in_progress, row.remaining_frames)
    };
    assert_eq!(state(2, 4), (1, 1, 0, 0));
    assert_eq!(state(2, 27), (0, 1, 1, u32::MAX));
    assert_eq!(state(2, 2), (0, 0, 1, u32::MAX));
    assert_eq!(state(0, 27), (0, 0, 0, 0));
}

#[test]
fn upgrades_and_techs_are_not_read_unless_subscribed() {
    let memory = in_game(4);
    let frame = engine(&memory, source::UNITS).acquire().unwrap();
    assert_eq!((frame.upgrades.rows(), frame.techs.rows()), (0, 0));
    assert_eq!(memory.reads_in(ROOT + BWGAME_TECH_AVAILABLE_SC_OFFSET, BWGAME_UPGRADE_IN_PROGRESS_OFFSET), 0);
}

const BULLET: BulletLayout = Layout::X64.bullet;
const BULLET_POOL_BYTES: usize = BULLET.capacity * BULLET.record_bytes;

/// One active bullet in `active`; every other slot is on the free list.
fn put_bullets(memory: &FakeMemory, active: usize, fill: impl Fn(&mut [u8])) {
    let record = |slot: usize| BULLET_POOL + slot * BULLET.record_bytes;
    let put = |raw: &mut [u8], at: usize, value: usize| raw[at..at + 8].copy_from_slice(&(value as u64).to_le_bytes());
    let mut pool = vec![0u8; BULLET_POOL_BYTES];
    let free: Vec<usize> = (0..BULLET.capacity).filter(|slot| *slot != active).collect();
    for (at, slot) in free.iter().enumerate() {
        let prev = if at == 0 { 0 } else { record(free[at - 1]) };
        let next = free.get(at + 1).map_or(0, |next| record(*next));
        put(&mut pool, slot * BULLET.record_bytes + BULLET.prev_offset, prev);
        put(&mut pool, slot * BULLET.record_bytes + BULLET.next_offset, next);
    }
    pool[active * BULLET.record_bytes + BULLET.active_marker_offset] = 1;
    fill(&mut pool[active * BULLET.record_bytes..][..BULLET.record_bytes]);
    memory.put(BULLET_POOL, &pool);
    let mut root = [0u8; BULLET.root_block_bytes];
    put(&mut root, BULLET.root_free_head_offset, record(free[0]));
    put(&mut root, BULLET.root_free_tail_offset, record(*free.last().unwrap()));
    put(&mut root, BULLET.root_pool_base_offset, BULLET_POOL);
    put(&mut root, BULLET.root_active_head_offset, record(active));
    put(&mut root, BULLET.root_active_tail_offset, record(active));
    memory.put_module(BULLET.root_block_module_offset, &root);
}

#[test]
fn bullets_are_decoded_with_unit_relations_and_visibility() {
    let memory = in_game(8);
    put_unit(&memory, 2, 7, 0, 100, 200, 5);
    memory.put(SPRITE, &[0; 0x20]);
    memory.patch(SPRITE + Layout::X64.csprite.visibility_flags_offset, &[0b101]);
    let target_ptr = POOL + 2 * Layout::X64.cunit.record_stride + Layout::X64.cunit.selection_ptr_adjustment;
    put_bullets(&memory, 7, |record| {
        record[BULLET.type_offset..][..2].copy_from_slice(&144u16.to_le_bytes());
        record[BULLET.position_x_offset..][..2].copy_from_slice(&(-5i16).to_le_bytes());
        record[BULLET.position_y_offset..][..2].copy_from_slice(&77i16.to_le_bytes());
        record[BULLET.source_player_offset] = 3;
        record[BULLET.remove_timer_offset] = 9;
        record[BULLET.sprite_ptr_offset..][..8].copy_from_slice(&(SPRITE as u64).to_le_bytes());
        record[BULLET.target_unit_ptr_offset..][..8].copy_from_slice(&(target_ptr as u64).to_le_bytes());
        record[BULLET.source_unit_ptr_offset..][..8].copy_from_slice(&(target_ptr as u64).to_le_bytes());
    });

    let frame = engine(&memory, source::UNITS | source::BULLETS).acquire().unwrap();
    let bullets = &frame.bullets;
    assert_eq!(bullets.id, [8]);
    assert_eq!((bullets.bullet_type[0], bullets.x[0], bullets.y[0]), (144, -5, 77));
    assert_eq!((bullets.owner[0], bullets.remove_timer[0], bullets.visible_to[0]), (3, 9, 0b101));
    assert_eq!(bullets.source, frame.units.id[..1]);
    assert_eq!(bullets.target, bullets.source);

    // Bullets alone still name units: the pool is read for them too.
    let alone = engine(&memory, source::BULLETS).acquire().unwrap();
    assert_eq!(alone.bullets.source, bullets.source);
}

#[test]
fn bullets_are_not_read_unless_subscribed() {
    let memory = in_game(4);
    put_bullets(&memory, 0, |_| {});
    let frame = engine(&memory, source::UNITS).acquire().unwrap();
    assert_eq!(frame.bullets.id.len(), 0);
    assert_eq!(memory.reads_in(BULLET_POOL, BULLET_POOL_BYTES), 0);
    let frame = engine(&memory, source::BULLETS).acquire().unwrap();
    assert_eq!(frame.bullets.id, [1]);
}

#[test]
fn vision_masks_and_map_size_are_decoded() {
    let memory = in_game(4);
    memory.patch(ROOT + BWGAME_MAP_WIDTH_OFFSET, &[3, 0, 2, 0]);
    memory.put_module(Layout::X64.map.active_tile_pointer_module_offset, &(TILES as u64).to_le_bytes());
    // Tile flag bytes: a set bit means not visible / not explored.
    let mut tiles = vec![0xffu8; 6 * 4];
    tiles[0] = !0b0000_0101;
    tiles[1] = !0b0000_0001;
    tiles[4 * 5] = 0;
    tiles[4 * 5 + 1] = 0;
    memory.put(TILES, &tiles);
    let frame = engine(&memory, source::VISION).acquire().unwrap();
    assert_eq!((frame.map_width, frame.map_height), (3, 2));
    assert_eq!(frame.vision.visible, [0b101, 0, 0, 0, 0, 0xff]);
    assert_eq!(frame.vision.explored, [0b001, 0, 0, 0, 0, 0xff]);
}

#[test]
fn vision_is_not_read_unless_subscribed() {
    let memory = in_game(4);
    memory.patch(ROOT + BWGAME_MAP_WIDTH_OFFSET, &[3, 0, 2, 0]);
    memory.put_module(Layout::X64.map.active_tile_pointer_module_offset, &(TILES as u64).to_le_bytes());
    memory.put(TILES, &[0; 24]);
    let frame = engine(&memory, source::UNITS).acquire().unwrap();
    assert_eq!((frame.map_width, frame.vision.visible.len()), (0, 0));
    assert_eq!(memory.reads_in(TILES, 24), 0);
    assert_eq!(memory.reads_in(ROOT + BWGAME_MAP_WIDTH_OFFSET, 4), 0);
}

#[test]
fn an_upgrade_in_progress_takes_its_remaining_frames_from_the_building() {
    let c = &Layout::X64.cunit;
    let memory = in_game(4);
    all_players_active(&memory);
    memory.patch(ROOT + BWGAME_MAX_UPGRADE_SC_OFFSET + 2 * BWGAME_SC_UPGRADE_COUNT + 5, &[3]);
    memory.patch(ROOT + BWGAME_UPGRADE_IN_PROGRESS_OFFSET + 2 * BWGAME_UPGRADE_IN_PROGRESS_STRIDE, &[1 << 5]);
    put_unit(&memory, 1, 122, 2, 50, 50, 1);
    let record = POOL + c.record_stride;
    memory.patch(record + c.order_offset, &[76]);
    memory.patch(record + c.upgrade_type_offset, &[5]);
    memory.patch(record + c.production_remaining_time_offset, &300u16.to_le_bytes());
    let frame = engine(&memory, source::UPGRADES).acquire().unwrap();
    assert_eq!(frame.upgrades.upgrade_type, [5]);
    assert_eq!(frame.upgrades.remaining_frames, [300]);
}

#[test]
fn a_rally_building_reports_its_rally_point_and_unit() {
    let memory = in_game(8);
    let c = &Layout::X64.cunit;
    put_unit(&memory, 0, 111, 0, 100, 100, 1);
    put_unit(&memory, 1, 0, 0, 300, 300, 2);
    memory.patch(POOL + c.status_flags_offset, &CUNIT_STATUS_COMPLETED.to_le_bytes());
    memory.patch(POOL + c.rally_position_x_offset, &500i16.to_le_bytes());
    memory.patch(POOL + c.rally_position_y_offset, &600i16.to_le_bytes());

    let mut engine = engine(&memory, source::UNITS);
    let frame = engine.acquire().unwrap();
    let barracks = row_of(&frame, 111);
    assert_eq!((barracks.rally_x, barracks.rally_y), (500, 600));
    assert_eq!(barracks.rally_unit, 0, "no rally unit pointer");
    // A unit that cannot rally carries unknown rally columns.
    assert_eq!(row_of(&frame, 0).rally_x, i16::UNKNOWN);
    assert_eq!(row_of(&frame, 0).rally_unit, u64::UNKNOWN);

    // A rallied unit names that unit; a lifted building loses the active
    // position but keeps the target.
    put_ptr(&memory, 0, c.rally_unit_ptr_offset, 1);
    memory.patch(POOL + c.order_offset, &6u8.to_le_bytes());
    let frame = engine.acquire().unwrap();
    let barracks = row_of(&frame, 111);
    assert_eq!((barracks.rally_x, barracks.rally_y), (i16::UNKNOWN, i16::UNKNOWN));
    assert_eq!(barracks.rally_unit, row_of(&frame, 0).id);

    let facts = reference_facts(&memory, 8);
    let order = facts
        .iter()
        .find(|fact| fact.unit_type_id == Some(111))
        .unwrap()
        .order
        .as_ref()
        .unwrap();
    assert_eq!(order.rally_position, None);
    assert_eq!(order.rally_unit_tag.as_deref(), Some("cunit:record:488"));
}

#[test]
fn a_gathering_worker_names_its_gas_building() {
    let memory = in_game(8);
    let c = &Layout::X64.cunit;
    put_unit(&memory, 0, 7, 2, 100, 100, 1);
    put_unit(&memory, 1, 110, 2, 100, 120, 2);
    memory.patch(POOL + c.order_offset, &80u8.to_le_bytes());
    put_ptr(&memory, 0, c.gas_resource_ptr_offset, 1);

    let mut engine = engine(&memory, source::UNITS);
    let frame = engine.acquire().unwrap();
    assert_eq!(row_of(&frame, 7).gas_building, row_of(&frame, 110).id);

    // Only gather orders carry the relation.
    memory.patch(POOL + c.order_offset, &3u8.to_le_bytes());
    let frame = engine.acquire().unwrap();
    assert_eq!(row_of(&frame, 7).gas_building, 0);

    // The target must be a same-owner gas building.
    memory.patch(POOL + c.order_offset, &80u8.to_le_bytes());
    memory.patch(POOL + c.record_stride + c.owner_offset, &[3u8]);
    let frame = engine.acquire().unwrap();
    assert_eq!(row_of(&frame, 7).gas_building, 0);
}

#[test]
fn a_hatchery_counts_down_larva_until_three_are_connected() {
    let memory = in_game(8);
    let c = &Layout::X64.cunit;
    put_unit(&memory, 0, 131, 5, 100, 100, 1);
    put_unit(&memory, 4, 0, 0, 50, 50, 9);
    memory.patch(POOL + c.status_flags_offset, &CUNIT_STATUS_COMPLETED.to_le_bytes());
    memory.patch(POOL + c.larva_timer_offset, &[37u8]);
    memory.patch(POOL + c.order_queue_timer_offset, &[4u8]);

    let mut engine = engine(&memory, source::UNITS);
    let frame = engine.acquire().unwrap();
    // 37 * 9 + (4 + 8) % 9 = 336; a non-hatchery type stays unknown.
    assert_eq!(row_of(&frame, 131).larva_frames, 336);
    assert_eq!(row_of(&frame, 0).larva_frames, u32::UNKNOWN);

    let facts = reference_facts(&memory, 8);
    let production = facts
        .iter()
        .find(|fact| fact.unit_type_id == Some(131))
        .unwrap()
        .production
        .as_ref()
        .unwrap();
    assert_eq!(production.remaining_train_frames, Some(336));

    // Two larvae keep the countdown; the third clears it.
    for slot in 1..=3 {
        put_unit(&memory, slot, 35, 5, 100, 110, slot as u8);
        put_ptr(&memory, slot, c.connected_unit_ptr_offset, 0);
        let frame = engine.acquire().unwrap();
        let expected = if slot < 3 { 336 } else { 0 };
        assert_eq!(row_of(&frame, 131).larva_frames, expected);
    }

    // BWAPI publishes the countdown in every state, under construction too.
    memory.patch(POOL + c.status_flags_offset, &0u32.to_le_bytes());
    memory.patch(POOL + 3 * c.record_stride + c.hit_points_offset, &0i32.to_le_bytes());
    let frame = engine.acquire().unwrap();
    assert_eq!(row_of(&frame, 131).larva_frames, 336);
}

#[test]
fn relation_columns_name_their_targets_per_the_reference() {
    let memory = in_game(8);
    let c = &Layout::X64.cunit;
    // A Goliath and its turret are a supported sub-unit pair.
    put_unit(&memory, 0, 3, 0, 100, 100, 1);
    put_unit(&memory, 1, 4, 0, 100, 100, 2);
    put_ptr(&memory, 0, c.sub_unit_ptr_offset, 1);
    // A marine with a move target on the ghost.
    put_unit(&memory, 2, 0, 0, 200, 200, 3);
    put_unit(&memory, 3, 1, 0, 300, 300, 4);
    put_ptr(&memory, 2, c.move_target_unit_ptr_offset, 3);
    memory.patch(POOL + 2 * c.record_stride + c.move_target_position_x_offset, &280i16.to_le_bytes());
    memory.patch(POOL + 2 * c.record_stride + c.move_target_position_y_offset, &290i16.to_le_bytes());
    // A factory constructing a machine shop names it as its build unit.
    put_unit(&memory, 4, 113, 0, 400, 100, 5);
    put_unit(&memory, 5, 120, 0, 450, 100, 6);
    put_ptr(&memory, 4, c.build_unit_ptr_offset, 5);
    // One completed nydus canal names its pair.
    put_unit(&memory, 6, 134, 5, 500, 500, 7);
    put_unit(&memory, 7, 134, 5, 600, 500, 8);
    memory.patch(POOL + 6 * c.record_stride + c.status_flags_offset, &CUNIT_STATUS_COMPLETED.to_le_bytes());
    memory.patch(POOL + 7 * c.record_stride + c.status_flags_offset, &CUNIT_STATUS_COMPLETED.to_le_bytes());
    put_ptr(&memory, 6, c.resource_amount_offset, 7);

    let mut engine = engine(&memory, source::UNITS);
    let frame = engine.acquire().unwrap();
    assert_eq!(row_of(&frame, 3).sub_unit, row_of(&frame, 4).id);
    assert_eq!(row_of(&frame, 4).sub_unit, 0);
    let marine = row_of(&frame, 0);
    assert_eq!(marine.move_target, row_of(&frame, 1).id);
    assert_eq!((marine.move_target_x, marine.move_target_y), (280, 290));
    assert_eq!(row_of(&frame, 113).build_unit, row_of(&frame, 120).id);
    let canal = row_of(&frame, 134);
    assert_eq!(canal.nydus_exit, frame.units.id[7]);
    assert_eq!(frame.units.nydus_exit[7], 0, "the other end has no exit set");

    let facts = profile_facts(&memory, 8);
    let fact = |unit_type: u16| {
        facts
            .iter()
            .find(|fact| fact.unit_type_id == Some(unit_type))
            .unwrap()
    };
    let slot_tag = |tag: &Option<String>| tag.as_deref().and_then(parse_record_slot);
    assert_eq!(slot_tag(&fact(3).cunit.as_ref().unwrap().sub_unit_tag), Some(1));
    assert_eq!(slot_tag(&fact(0).order.as_ref().unwrap().move_target_unit_tag), Some(3));
    assert_eq!(
        fact(0).order.as_ref().unwrap().move_target_position.map(|position| (position.x, position.y)),
        Some((280, 290))
    );
    assert_eq!(slot_tag(&fact(113).production.as_ref().unwrap().build_unit_tag), Some(5));
    assert_eq!(slot_tag(&fact(134).cunit.as_ref().unwrap().nydus_exit_unit_tag), Some(7));

    // A move target that names the unit itself stays unknown.
    put_ptr(&memory, 2, c.move_target_unit_ptr_offset, 2);
    let frame = engine.acquire().unwrap();
    assert_eq!(row_of(&frame, 0).move_target, u64::UNKNOWN);
}

/// Slot of a "cunit:record:<offset>" tag.
fn parse_record_slot(tag: &str) -> Option<usize> {
    tag.rsplit(':').next()?.parse::<usize>().ok().map(|offset| offset / Layout::X64.cunit.record_stride)
}

#[test]
fn a_completed_worker_names_its_power_up() {
    let memory = in_game(4);
    let c = &Layout::X64.cunit;
    put_unit(&memory, 0, 7, 0, 100, 100, 1);
    put_unit(&memory, 1, 215, 11, 100, 120, 2);
    memory.patch(POOL + c.status_flags_offset, &CUNIT_STATUS_COMPLETED.to_le_bytes());
    memory.patch(POOL + c.record_stride + c.status_flags_offset, &CUNIT_STATUS_COMPLETED.to_le_bytes());
    put_ptr(&memory, 0, c.worker_power_up_ptr_offset, 1);

    let mut engine = engine(&memory, source::UNITS);
    let frame = engine.acquire().unwrap();
    assert_eq!(row_of(&frame, 7).power_up, row_of(&frame, 215).id);

    let facts = profile_facts(&memory, 4);
    let worker = facts.iter().find(|fact| fact.unit_type_id == Some(7)).unwrap().cunit.as_ref().unwrap();
    assert_eq!(worker.has_power_up, Some(true));
    assert_eq!(worker.power_up_unit_tag.as_deref().and_then(parse_record_slot), Some(1));

    // A worker without a power-up carries none.
    memory.patch(POOL + c.worker_power_up_ptr_offset, &0u64.to_le_bytes());
    let frame = engine.acquire().unwrap();
    assert_eq!(row_of(&frame, 7).power_up, 0);

    // An incomplete worker does not carry the relation at all.
    memory.patch(POOL + c.worker_power_up_ptr_offset, &((POOL + c.record_stride) as u64).to_le_bytes());
    memory.patch(POOL + c.status_flags_offset, &0u32.to_le_bytes());
    let frame = engine.acquire().unwrap();
    assert_eq!(row_of(&frame, 7).power_up, u64::UNKNOWN);
    assert_eq!(row_of(&frame, 215).power_up, u64::UNKNOWN, "non-workers carry no relation");
}

#[test]
fn a_carrier_counts_interceptors_and_an_interceptor_names_its_carrier() {
    let memory = in_game(8);
    let c = &Layout::X64.cunit;
    put_unit(&memory, 0, 72, 0, 100, 100, 1);
    put_unit(&memory, 1, 73, 0, 100, 120, 2);
    // Carrier roster: one interceptor in hangar, none launched.
    put_ptr(&memory, 0, c.hangar_in_head_ptr_offset, 1);
    memory.patch(POOL + c.hangar_in_count_offset, &[1u8]);
    // Interceptor: parent link, in-hangar byte, and its list links.
    put_ptr(&memory, 1, c.hangar_in_head_ptr_offset, 0);
    memory.patch(POOL + c.record_stride + c.hangar_in_hangar_offset, &[1u8]);

    let mut engine = engine(&memory, source::UNITS);
    let frame = engine.acquire().unwrap();
    let carrier = row_of(&frame, 72);
    assert_eq!(carrier.interceptors, 1);
    assert_eq!(carrier.scarabs, u8::UNKNOWN);
    assert_eq!(row_of(&frame, 73).carrier, carrier.id);

    let facts = profile_facts(&memory, 8);
    let interceptor = facts.iter().find(|fact| fact.unit_type_id == Some(73)).unwrap();
    let hangar = interceptor.cunit.as_ref().unwrap();
    assert_eq!(hangar.has_hangar_parent, Some(true));
    assert_eq!(hangar.hangar_parent_unit_tag.as_deref().and_then(parse_record_slot), Some(0));
    assert_eq!(
        facts.iter().find(|fact| fact.unit_type_id == Some(72)).unwrap().cunit.as_ref().unwrap().interceptor_count,
        Some(1)
    );

    // A reaver reports its in-hangar scarab count only.
    put_unit(&memory, 2, 83, 0, 200, 200, 3);
    memory.patch(POOL + 2 * c.record_stride + c.hangar_in_count_offset, &[5u8]);
    let frame = engine.acquire().unwrap();
    assert_eq!(row_of(&frame, 83).scarabs, 5);
    assert_eq!(row_of(&frame, 83).interceptors, u8::UNKNOWN);
    assert_eq!(row_of(&frame, 83).carrier, u64::UNKNOWN);

    // An interceptor the roster does not list in its own hangar half stays
    // unknown.
    memory.patch(POOL + c.record_stride + c.hangar_in_hangar_offset, &[0u8]);
    let frame = engine.acquire().unwrap();
    assert_eq!(row_of(&frame, 73).carrier, u64::UNKNOWN);
}

#[test]
fn direct_status_timers_flags_and_motion_match_the_reference() {
    let memory = in_game(4);
    let c = &Layout::X64.cunit;
    put_unit(&memory, 0, 0, 0, 100, 100, 1);
    memory.patch(POOL + c.order_timer_offset, &[77u8]);
    memory.patch(POOL + c.stim_timer_offset, &[5u8]);
    memory.patch(POOL + c.storm_timer_offset, &[9u8]);
    memory.patch(POOL + c.defense_matrix_damage_offset, &512u16.to_le_bytes());
    memory.patch(POOL + c.ground_weapon_cooldown_offset, &[3u8]);
    memory.patch(POOL + c.air_weapon_cooldown_offset, &[4u8]);
    memory.patch(POOL + c.spell_cooldown_offset, &[6u8]);
    memory.patch(POOL + c.kill_count_offset, &[12u8]);
    memory.patch(POOL + c.acid_spore_count_offset, &[2u8]);
    memory.patch(POOL + c.last_attacking_player_id_offset, &[5u8]);
    memory.patch(POOL + c.last_event_color_offset, &[174u8]);
    memory.patch(POOL + c.last_event_timer_offset, &[3u8]);
    memory.patch(POOL + c.button_set_id_offset, &4113u16.to_le_bytes());
    memory.patch(POOL + c.previous_hit_points_raw_offset, &(45 * 256u16).to_le_bytes());
    memory.patch(POOL + c.is_being_healed_offset, &[1u8]);
    memory.patch(POOL + c.is_blind_offset, &[1u8]);
    memory.patch(POOL + c.parasite_flags_offset, &[2u8]);
    memory.patch(POOL + c.movement_state_offset, &[0x0du8]);
    memory.patch(POOL + c.movement_flags_offset, &[0x10 | 0x02]);
    memory.patch(POOL + c.current_direction_1_offset, &[64u8]);
    memory.patch(POOL + c.velocity_x_offset, &300i32.to_le_bytes());
    memory.patch(POOL + c.velocity_y_offset, &(-75i32).to_le_bytes());

    let mut engine = engine(&memory, source::UNITS);
    let frame = engine.acquire().unwrap();
    let marine = row_of(&frame, 0);
    assert_eq!(marine.order_timer, 77);
    assert_eq!((marine.stim_timer, marine.storm_timer), (5, 9));
    assert_eq!(marine.defense_matrix_damage, 512);
    assert_eq!((marine.ground_cooldown, marine.air_cooldown, marine.spell_cooldown), (3, 4, 6));
    assert_eq!(marine.kill_count, 12);
    assert_eq!(marine.acid_spores, 2);
    assert_eq!(marine.last_attacker, 5);
    assert_eq!(marine.recently_attacked, 1);
    assert_eq!(marine.button_set, 4113);
    assert_eq!(marine.previous_hit_points, 45 * 256);
    assert_eq!(marine.being_healed, 1);
    assert_eq!((marine.blind, marine.parasited), (1, 1));
    assert_eq!(marine.stuck, 1);
    assert_eq!((marine.moving, marine.accelerating, marine.braking), (1, 1, 0));
    assert_eq!(marine.facing, 64);
    assert_eq!((marine.velocity_x, marine.velocity_y), (300, -75));

    let facts = profile_facts(&memory, 4);
    let fact = facts.iter().find(|fact| fact.record_index == Some(0)).unwrap();
    let cunit = fact.cunit.as_ref().unwrap();
    let order = fact.order.as_ref().unwrap();
    let motion = fact.motion.as_ref().unwrap();
    assert_eq!(order.order_timer, Some(77));
    assert_eq!(cunit.stim_timer, Some(5));
    assert_eq!(cunit.storm_timer, Some(9));
    assert_eq!(cunit.defense_matrix_damage_raw, Some(512));
    assert_eq!(cunit.ground_weapon_cooldown, Some(3));
    assert_eq!(cunit.air_weapon_cooldown, Some(4));
    assert_eq!(cunit.spell_cooldown, Some(6));
    assert_eq!(cunit.kill_count, Some(12));
    assert_eq!(cunit.acid_spore_count, Some(2));
    assert_eq!(cunit.last_attacking_player_id, Some(5));
    assert_eq!(cunit.recently_attacked, Some(true));
    assert_eq!(cunit.button_set_id, Some(4113));
    assert_eq!(cunit.previous_hit_points_raw, Some(45 * 256));
    assert_eq!(cunit.is_being_healed, Some(true));
    assert_eq!(cunit.is_blind, Some(true));
    assert_eq!(cunit.is_parasited, Some(true));
    assert_eq!(cunit.is_stuck, Some(true));
    assert_eq!((motion.is_moving, motion.is_accelerating, motion.is_braking), (Some(true), Some(true), Some(false)));
    assert_eq!(motion.facing_direction_raw, Some(64));
    assert_eq!((motion.velocity_x_raw, motion.velocity_y_raw), (Some(300), Some(-75)));

    // An expired attack notification and an out-of-range attacker byte.
    memory.patch(POOL + c.last_event_timer_offset, &[0u8]);
    memory.patch(POOL + c.last_attacking_player_id_offset, &[12u8]);
    let frame = engine.acquire().unwrap();
    assert_eq!(row_of(&frame, 0).recently_attacked, 0);
    assert_eq!(row_of(&frame, 0).last_attacker, u8::UNKNOWN);
}

#[test]
fn a_nuclear_silo_with_a_ready_nuke_reports_it() {
    let memory = in_game(4);
    let c = &Layout::X64.cunit;
    put_unit(&memory, 0, 108, 0, 100, 100, 1);
    memory.patch(POOL + c.silo_nuke_ready_offset, &[1u8]);
    put_unit(&memory, 1, 0, 0, 200, 200, 2);

    let mut engine = engine(&memory, source::UNITS);
    let frame = engine.acquire().unwrap();
    assert_eq!(row_of(&frame, 108).has_nuke, 1);
    assert_eq!(row_of(&frame, 0).has_nuke, 0);

    let facts = profile_facts(&memory, 4);
    let silo = facts
        .iter()
        .find(|fact| fact.unit_type_id == Some(108))
        .unwrap()
        .cunit
        .as_ref()
        .unwrap();
    assert_eq!(silo.has_nuke, Some(true));
}

#[test]
fn a_gathering_worker_reports_cargo_and_resource_states() {
    let memory = in_game(4);
    let c = &Layout::X64.cunit;
    put_unit(&memory, 0, 7, 0, 100, 100, 1);
    memory.patch(
        POOL + c.status_flags_offset,
        &(CUNIT_STATUS_COMPLETED | crate::decode::cunit::CUNIT_STATUS_IS_GATHERING).to_le_bytes(),
    );
    memory.patch(POOL + c.worker_resource_type_offset, &[1u8]);
    put_unit(&memory, 1, 176, 11, 200, 200, 2);
    memory.patch(POOL + c.record_stride + c.status_flags_offset, &CUNIT_STATUS_COMPLETED.to_le_bytes());
    memory.patch(POOL + c.record_stride + c.resource_group_offset, &[7u8]);

    let mut engine = engine(&memory, source::UNITS);
    let frame = engine.acquire().unwrap();
    let worker = row_of(&frame, 7);
    assert_eq!(worker.gathering, 1);
    assert_eq!(worker.carry, 1);
    assert_eq!(row_of(&frame, 176).resource_group, 7);
    assert_eq!(worker.resource_group, u8::UNKNOWN, "only resource containers carry a group");
    assert_eq!(row_of(&frame, 176).being_gathered, 0);
    assert_eq!(worker.being_gathered, u8::UNKNOWN, "only resource containers are gathered");

    let facts = profile_facts(&memory, 4);
    let worker_cunit = facts
        .iter()
        .find(|fact| fact.unit_type_id == Some(7))
        .unwrap()
        .cunit
        .as_ref()
        .unwrap();
    assert_eq!(worker_cunit.is_gathering, Some(true));
    assert_eq!(worker_cunit.resource_cargo_kind, Some(1));
    assert_eq!(
        facts.iter().find(|fact| fact.unit_type_id == Some(176)).unwrap().cunit.as_ref().unwrap().resource_group,
        Some(7)
    );

    // A worker waiting in the gather queue, or one named by the
    // next-gatherer link, marks the container as being gathered.
    let mineral = POOL + c.record_stride;
    memory.patch(mineral + c.resource_gather_queue_count_offset, &[1u8]);
    assert_eq!(row_of(&engine.acquire().unwrap(), 176).being_gathered, 1);
    memory.patch(mineral + c.resource_gather_queue_count_offset, &[0u8]);
    put_ptr(&memory, 1, c.resource_next_gatherer_ptr_offset, 0);
    assert_eq!(row_of(&engine.acquire().unwrap(), 176).being_gathered, 1);
    memory.patch(mineral + c.resource_next_gatherer_ptr_offset, &0u64.to_le_bytes());

    // Zero is a known empty cargo; an invalid byte stays unknown.
    memory.patch(POOL + c.worker_resource_type_offset, &[0u8]);
    let frame = engine.acquire().unwrap();
    assert_eq!(row_of(&frame, 7).carry, 0);
    memory.patch(POOL + c.worker_resource_type_offset, &[3u8]);
    let frame = engine.acquire().unwrap();
    assert_eq!(row_of(&frame, 7).carry, u8::UNKNOWN);
}

#[test]
fn a_units_render_position_comes_from_its_sprites_main_image() {
    let memory = in_game(4);
    let layout = &Layout::X64;
    let c = &layout.cunit;
    put_unit(&memory, 0, 7, 0, 100, 200, 1);
    memory.put(UNIT_SPRITE, &vec![0; layout.csprite.main_image_ptr_offset + 8]);
    memory.put(UNIT_IMAGE, &vec![0; layout.cimage.parent_sprite_ptr_offset + 8]);
    memory.patch(UNIT_SPRITE + layout.csprite.main_image_ptr_offset, &(UNIT_IMAGE as u64).to_le_bytes());
    let map_x = layout.cimage.frame_set_offset + 4;
    memory.patch(UNIT_IMAGE + map_x, &120i16.to_le_bytes());
    memory.patch(UNIT_IMAGE + map_x + 2, &130i16.to_le_bytes());
    memory.patch(UNIT_IMAGE + layout.cimage.parent_sprite_ptr_offset, &(UNIT_SPRITE as u64).to_le_bytes());
    memory.patch(POOL + c.sprite_ptr_offset, &(UNIT_SPRITE as u64).to_le_bytes());
    memory.patch(UNIT_SPRITE + layout.csprite.visibility_flags_offset, &[0b101]);
    memory.patch(UNIT_IMAGE + layout.cimage.animation_offset, &[5u8]);
    memory.patch(UNIT_IMAGE + layout.cimage.frame_set_offset, &34u16.to_le_bytes());

    let mut engine = engine(&memory, source::RENDER);
    let render = |engine: &mut Engine| {
        let frame = engine.acquire().unwrap();
        (frame.render.x[0], frame.render.y[0], frame.render.visible_to[0])
    };
    assert_eq!(render(&mut engine), (120, 130, 0b101));
    let frame = engine.acquire().unwrap();
    assert_eq!((frame.render.animation[0], frame.render.frame_set[0]), (5, 34));

    // The image must belong to the unit's sprite; the flags byte survives.
    memory.patch(UNIT_IMAGE + layout.cimage.parent_sprite_ptr_offset, &((UNIT_SPRITE + 0x48) as u64).to_le_bytes());
    assert_eq!(render(&mut engine), (i16::UNKNOWN, i16::UNKNOWN, 0b101));
    let frame = engine.acquire().unwrap();
    assert_eq!((frame.render.animation[0], frame.render.frame_set[0]), (u8::UNKNOWN, u16::UNKNOWN));

    // No sprite leaves both unknown.
    memory.patch(POOL + c.sprite_ptr_offset, &0u64.to_le_bytes());
    assert_eq!(render(&mut engine), (i16::UNKNOWN, i16::UNKNOWN, u16::UNKNOWN));
}

#[test]
fn in_process_acquire_reads_the_same_tables_as_the_fake_backend() {
    let memory = in_game(8);
    put_unit(&memory, 2, 7, 0, 100, 200, 5);
    put_unit(&memory, 5, 37, 1, 300, 400, 9);
    let expected = engine(&memory, source::UNITS).acquire().unwrap();
    let expected = (
        expected.units.unit_type.clone(),
        expected.units.owner.clone(),
        expected.units.x.clone(),
        expected.units.y.clone(),
        expected.units.id.clone(),
    );

    // The same image as real buffers of this process, read in place.
    let layout = &Layout::X64;
    let stride = layout.cunit.record_stride;
    let module_image = vec![0u8; layout.module_size as usize];
    let root_block = vec![0u8; crate::decode::bwgame::BWGAME_ROOT_READ_BYTES];
    let pool_block = vec![0u8; 8 * stride + stride];
    let module_base = module_image.as_ptr() as usize;
    let root = root_block.as_ptr() as usize;
    // The pool base must be record-stride aligned for slot resolution.
    let pool = (pool_block.as_ptr() as usize + stride - 1) & !(stride - 1);
    let mut write = |address: usize, bytes: &[u8]| unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), address as *mut u8, bytes.len());
    };
    write_in_game_image(layout, module_base, root, pool, 8, &mut write);
    write_unit(layout, pool, 2, 7, 0, 100, 200, 5, &mut write);
    write_unit(layout, pool, 5, 37, 1, 300, 400, 9, &mut write);

    let mut engine = Engine::with_memory(Box::new(InProcessMemory::with_module(ProcessModule {
        base_address: module_base,
        size: layout.module_size as usize,
    }))).unwrap();
    engine.subscribe(source::UNITS);
    let frame = engine.acquire().unwrap();
    assert_eq!(
        (frame.units.unit_type.clone(), frame.units.owner.clone(), frame.units.x.clone(), frame.units.y.clone(), frame.units.id.clone()),
        expected
    );
    assert_eq!(frame.flags & frame_flag::TORN, 0);
    assert_eq!(engine.command_basis().map(|basis| basis.pool_base), Some(pool as u64));
}
