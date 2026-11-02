//! Reference CUnit decoder. The engine's unit columns (`read::units`) are
//! checked against it by the engine tests and `examples/decoder_compare.rs`.

mod constants;
mod helpers;
mod pattern;
mod production;
mod queue_rally;
mod record;

pub use constants::*;
pub use pattern::{
    is_ordinary_building_addon_pair, is_ordinary_sub_unit_pair, is_worker,
};
pub use production::{
    derive_cunit_in_progress_facts, plausible_cunit_record, protoss_unit_type_requires_power,
    unit_type_has_energy, unit_type_has_shields,
};
pub use queue_rally::{
    active_rally_position, cunit_record_offset_from_direct_ptr, cunit_record_offset_from_target_ptr,
    unit_type_can_rally,
};
pub use record::{
    iter_cunit_unit_facts, iter_cunit_unit_facts_with_queued_orders,
    iter_cunit_unit_facts_with_queued_orders_profile,
};

#[cfg(test)]
mod tests;
