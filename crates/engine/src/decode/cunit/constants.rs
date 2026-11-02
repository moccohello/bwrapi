use bwrapi_internal_model::orders;

use bwrapi_layout::Layout;

/// Every offset below is a view of `Layout::X64`; readers that have a
/// `&Layout` use that instead.
const X64_CUNIT: bwrapi_layout::CunitLayout = Layout::X64.cunit;

/// One SCR CUnit slot (not the 0x150-byte stride of the original game).
/// Record indices and CUnit pointer validation use this.
pub const CUNIT_RECORD_STRIDE: usize = X64_CUNIT.record_stride;
/// The validated SCR pool has 3400 logical CUnit records. The allocation is
/// larger, so readers must not treat trailing bytes as compatibility slots.
pub const CUNIT_RECORD_COUNT: usize = 3400;
/// Two adjacent CUnit slots are read together so relationship lookups remain bounded.
pub const CUNIT_READ_WINDOW_BYTES: usize = X64_CUNIT.read_window_bytes;
/// One SCR CUnit slot; use CUNIT_READ_WINDOW_BYTES for a two-record read.
pub const CUNIT_STRIDE: usize = CUNIT_RECORD_STRIDE;
pub const CUNIT_ALIGNMENTS: [usize; 2] = [0x0, X64_CUNIT.record_stride];
/// SCR relationship/selection pointers can address record+0x10. Normalize
/// before making a canonical `cunit:record:<offset>` tag.
pub const CUNIT_SELECTION_PTR_ADJUSTMENT: usize = X64_CUNIT.selection_ptr_adjustment;
pub const CUNIT_HIT_POINTS_OFFSET: usize = X64_CUNIT.hit_points_offset;
pub const CUNIT_POSITION_X_OFFSET: usize = X64_CUNIT.position_x_offset;
pub const CUNIT_POSITION_Y_OFFSET: usize = X64_CUNIT.position_y_offset;
// CFlingy's current_speed is a signed POINT (two i32 values), not two i16s.
pub const CUNIT_VELOCITY_X_OFFSET: usize = X64_CUNIT.velocity_x_offset;
pub const CUNIT_VELOCITY_Y_OFFSET: usize = X64_CUNIT.velocity_y_offset;
pub const CUNIT_OWNER_OFFSET: usize = X64_CUNIT.owner_offset;
pub const CUNIT_ORDER_OFFSET: usize = X64_CUNIT.order_offset;
pub const CUNIT_ORDER_TARGET_UNIT_PTR_OFFSET: usize = X64_CUNIT.order_target_unit_ptr_offset;
pub const CUNIT_SHIELDS_OFFSET: usize = X64_CUNIT.shields_offset;
pub const CUNIT_TYPE_OFFSET: usize = X64_CUNIT.type_offset;
pub const CUNIT_ORDER_QUEUE_HEAD_OFFSET: usize = X64_CUNIT.order_queue_head_offset;
pub const CUNIT_ORDER_QUEUE_TAIL_OFFSET: usize = X64_CUNIT.order_queue_tail_offset;
pub const CUNIT_ORDER_QUEUE_COUNT_OFFSET: usize = X64_CUNIT.order_queue_count_offset;
pub const CUNIT_ORDER_QUEUE_TIMER_OFFSET: usize = X64_CUNIT.order_queue_timer_offset;
pub const CUNIT_CONNECTED_UNIT_PTR_OFFSET: usize = X64_CUNIT.connected_unit_ptr_offset;
pub const CUNIT_BUILD_QUEUE_TYPE_OFFSET: usize = X64_CUNIT.build_queue_type_offset;
pub const CUNIT_NUCLEAR_SILO_TYPE_ID: u16 = 108;
pub const CUNIT_BUNKER_TYPE_ID: u16 = 125;
pub const CUNIT_NYDUS_CANAL_TYPE_ID: u16 = 134;
pub const CUNIT_MOVEMENT_STATE_MOVE_TO_LEGAL: u8 = 0x0d;
pub const CUNIT_BUILD_QUEUE_SLOT_OFFSET: usize = X64_CUNIT.build_queue_slot_offset;
pub const CUNIT_BUILD_UNIT_REMAINING_TIME_OFFSET: usize =
    X64_CUNIT.build_unit_remaining_time_offset;
pub const CUNIT_BUILD_QUEUE_SLOTS: usize = 5;
pub const CUNIT_UNIT_TYPE_NONE: u16 = 228;
pub const CUNIT_PRODUCTION_REMAINING_TIME_OFFSET: usize =
    X64_CUNIT.production_remaining_time_offset;
pub const CUNIT_RESEARCH_TECH_TYPE_OFFSET: usize = X64_CUNIT.research_tech_type_offset;
pub const CUNIT_UPGRADE_TYPE_OFFSET: usize = X64_CUNIT.upgrade_type_offset;
// Hatchery-family-only larva regeneration timer (u8) in 9-frame ticks, not
// frames: remainingTrainTime = larvaTimer * 9 + ((orderQueueTimer + 8) % 9)
// for Hatchery/Lair/Hive.
pub const CUNIT_LARVA_TIMER_OFFSET: usize = X64_CUNIT.larva_timer_offset;
/// Zerg Hatchery, Lair, Hive — the larva-regenerating resource depots.
pub const CUNIT_ZERG_HATCHERY_FAMILY_TYPE_IDS: [u16; 3] = [131, 132, 133];
/// Zerg Larva.
pub const CUNIT_ZERG_LARVA_TYPE_ID: u16 = 35;
pub const CUNIT_STATUS_FLAGS_OFFSET: usize = X64_CUNIT.status_flags_offset;
pub const CUNIT_STATUS_COMPLETED: u32 = 0x0000_0001;
pub const CUNIT_STATUS_IN_AIR: u32 = 0x0000_0004;
pub const CUNIT_STATUS_BURROWED: u32 = 0x0000_0010;
pub const CUNIT_STATUS_IN_BUILDING: u32 = 0x0000_0020;
pub const CUNIT_STATUS_IN_TRANSPORT: u32 = 0x0000_0040;
pub const CUNIT_STATUS_CANNOT_RECEIVE_ORDERS: u32 = 0x0000_1000;
pub const CUNIT_STATUS_PROTOSS_UNPOWERED: u32 = 0x0000_0400;
pub const CUNIT_STATUS_REQUIRES_DETECTION: u32 = 0x0000_0100;
pub const CUNIT_STATUS_CLOAKED: u32 = 0x0000_0200;
pub const CUNIT_STATUS_HALLUCINATION: u32 = 0x4000_0000;
pub const CUNIT_STATUS_IS_GATHERING: u32 = 0x0080_0000;
pub const CUNIT_STATUS_INVINCIBLE: u32 = 0x0400_0000;
pub const CUNIT_MOVEMENT_FLAG_ACCELERATING: u8 = 0x02;
pub const CUNIT_MOVEMENT_FLAG_BRAKING: u8 = 0x04;
pub const CUNIT_MOVEMENT_FLAG_MOVING: u8 = 0x10;
pub const CUNIT_RALLY_POSITION_X_OFFSET: usize = X64_CUNIT.rally_position_x_offset;
pub const CUNIT_RALLY_POSITION_Y_OFFSET: usize = X64_CUNIT.rally_position_y_offset;
pub const CUNIT_RALLY_UNIT_PTR_OFFSET: usize = X64_CUNIT.rally_unit_ptr_offset;
pub const CUNIT_STIM_TIMER_OFFSET: usize = X64_CUNIT.stim_timer_offset;
pub const CUNIT_ORDER_IDLE: u8 = order_id_u8(23);
pub const CUNIT_ORDER_TRAIN: u8 = order_id_u8(38);
pub const CUNIT_ORDER_CONSTRUCTING: u8 = order_id_u8(30);
pub const CUNIT_ORDER_PLACE_BUILDING: u8 = order_id_u8(30);
pub const CUNIT_ORDER_ZERG_BIRTH: u8 = order_id_u8(41);
pub const CUNIT_ORDER_ZERG_UNIT_MORPH: u8 = order_id_u8(42);
pub const CUNIT_ORDER_ZERG_BUILDING_MORPH: u8 = order_id_u8(43);
pub const CUNIT_ORDER_INCOMPLETE_BUILDING: u8 = order_id_u8(44);
pub const CUNIT_ORDER_INCOMPLETE_MORPHING: u8 = order_id_u8(45);
pub const CUNIT_ORDER_BUILDING_IN_PROGRESS: u8 = order_id_u8(48);
pub const CUNIT_ORDER_DRONE_LAND: u8 = order_id_u8(70);
pub const CUNIT_ORDER_BUILD_ADDON: u8 = order_id_u8(37);
pub const CUNIT_ORDER_RESEARCH: u8 = order_id_u8(75);
pub const CUNIT_ORDER_UPGRADE: u8 = order_id_u8(76);
pub const CUNIT_TECH_TYPE_MIN: u8 = 0;
pub const CUNIT_TECH_TYPE_MAX: u8 = 45;
pub const CUNIT_UPGRADE_TYPE_MIN: u8 = 0;
pub const CUNIT_UPGRADE_TYPE_MAX: u8 = 60;
pub const CUNIT_SCAN_MIN_BYTES: usize = X64_CUNIT.scan_min_bytes;
pub const CORDER_READ_BYTES: usize = Layout::X64.corder.read_bytes;
pub const CORDER_PREV_OFFSET: usize = Layout::X64.corder.prev_offset;
pub const CORDER_NEXT_OFFSET: usize = Layout::X64.corder.next_offset;
pub const CORDER_ORDER_OFFSET: usize = Layout::X64.corder.order_offset;
pub const CORDER_UNIT_TYPE_OFFSET: usize = Layout::X64.corder.unit_type_offset;
pub const CORDER_TARGET_X_OFFSET: usize = Layout::X64.corder.target_x_offset;
pub const CORDER_TARGET_Y_OFFSET: usize = Layout::X64.corder.target_y_offset;
pub const CORDER_TARGET_UNIT_PTR_OFFSET: usize = Layout::X64.corder.target_unit_ptr_offset;
pub const CORDER_MAX_PUBLIC_QUEUE_COUNT: u8 = 8;
pub const CORDER_MAX_LINKED_NODE_COUNT: usize = 32;

pub(super) const CUNIT_MAX_UNIT_HIT_POINTS_RAW: i32 = 5_000_000;
pub(super) const CUNIT_MAX_MINERAL_HIT_POINTS_RAW: i32 = 50_000_000;
pub(super) const CUNIT_MAX_SHIELDS_RAW: u32 = 5_000_000;

const fn order_id_u8(order_id: u16) -> u8 {
    assert!(order_id < orders::ORDER_COUNT as u16);
    order_id as u8
}
