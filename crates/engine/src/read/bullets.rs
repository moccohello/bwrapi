use bwrapi_layout::read_pointer;

use super::pool::Pool;
use super::{i16_at, i32_at, u16_at};
use crate::columns::Cell;
use crate::decode::bullet::*;
use crate::engine::{State, Target};
use crate::tables::*;

const MAX_PLAYER: u8 = 11;

/// Active bullets of the pool. Ids are slot + 1. Source and target unit ids
/// need the CUnit pool of this frame, else they are unknown.
/// `visible_to` is the bullet sprite's visibility flag byte.
pub(super) fn bullets(target: &Target, state: &mut State, table: &mut Bullets) {
    let layout = &target.layout.bullet;
    // One buffer: the root block, then the record array.
    let block = &mut state.block;
    block.resize(layout.root_block_bytes + layout.capacity * layout.record_bytes, 0);
    let (root, pool) = block.split_at_mut(layout.root_block_bytes);
    if target.read_module(layout.root_block_module_offset, root).is_err() {
        return;
    }
    let Some(base) = bullet_root_identity(layout, root).map(|identity| identity.pool_base) else { return };
    if target.read(base, pool).is_err() {
        return;
    }
    let Some(active) = validated_bullet_active_slots(layout, root, pool) else { return };

    let cunits = (state.pool_base != 0).then(|| Pool {
        raw: &state.cunits,
        base: state.pool_base,
        pointer_bytes: target.layout.pointer_bytes,
        cunit: &target.layout.cunit,
        reached: &state.reached,
    });
    for slot in active {
        let record = &pool[slot * layout.record_bytes..][..layout.record_bytes];
        let relation = |field: usize| cunits.as_ref().map_or(u64::UNKNOWN, |cunits| cunits.target_id(record, field));
        let source = relation(layout.source_unit_ptr_offset);
        let sprite = read_pointer(record, layout.sprite_ptr_offset, layout.pointer_bytes).unwrap_or(0);
        let mut visibility = [0];
        let visible = sprite != 0
            && target.read(sprite + target.layout.csprite.visibility_flags_offset, &mut visibility).is_ok();
        table.push(BulletRow {
            id: slot as u64 + 1,
            bullet_type: u16_at(record, layout.type_offset),
            // The player is known only while a source unit exists.
            owner: match record[layout.source_player_offset] {
                player if source != 0 && player <= MAX_PLAYER => player,
                _ => u8::UNKNOWN,
            },
            x: i16_at(record, layout.position_x_offset),
            y: i16_at(record, layout.position_y_offset),
            source,
            target: relation(layout.target_unit_ptr_offset),
            remove_timer: record[layout.remove_timer_offset],
            visible_to: if visible { u32::from(visibility[0]) } else { u32::UNKNOWN },
            target_x: i16_at(record, layout.target_position_x_offset),
            target_y: i16_at(record, layout.target_position_y_offset),
            velocity_x: i32_at(record, layout.velocity_x_offset),
            velocity_y: i32_at(record, layout.velocity_y_offset),
            direction: record[layout.velocity_direction_offset],
        });
    }
}
