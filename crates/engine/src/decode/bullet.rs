use std::collections::HashSet;

use bwrapi_layout::{read_pointer, BulletLayout};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BulletRootIdentity {
    free_head: usize,
    free_tail: usize,
    pub(crate) pool_base: usize,
    active_head: usize,
    active_tail: usize,
}

/// Full partition proof for a frozen source pool, never a lifetime proof.
pub fn validated_bullet_active_slots(layout: &BulletLayout, root: &[u8], pool: &[u8]) -> Option<Vec<usize>> {
    let identity = bullet_root_identity(layout, root)?;
    if identity.pool_base == 0 || identity.pool_base % layout.pointer_bytes != 0
        || pool.len() != layout.capacity * layout.record_bytes { return None; }
    let free = read_list_slots(layout, pool, identity.pool_base, identity.free_head, identity.free_tail, None)?;
    let active = read_list_slots(layout, pool, identity.pool_base, identity.active_head, identity.active_tail, Some(1))?;
    let free_set = free.into_iter().collect::<HashSet<_>>();
    let active_set = active.iter().copied().collect::<HashSet<_>>();
    (free_set.is_disjoint(&active_set)
        && free_set.len().checked_add(active_set.len())? == layout.capacity).then_some(active)
}

fn read_list_slots(
    layout: &BulletLayout,
    pool_raw: &[u8],
    pool_base: usize,
    head: usize,
    tail: usize,
    required_marker: Option<u32>,
) -> Option<Vec<usize>> {
    if head == 0 || tail == 0 {
        return (head == 0 && tail == 0).then(Vec::new);
    }

    let mut slots = Vec::new();
    let mut seen = HashSet::new();
    let mut previous = 0usize;
    let mut current = head;
    while slots.len() < layout.capacity {
        let slot_index = bullet_slot_from_pointer(layout, current, pool_base)?;
        if !seen.insert(slot_index) {
            return None;
        }
        let offset = slot_index.checked_mul(layout.record_bytes)?;
        if read_pointer(pool_raw, offset + layout.prev_offset, layout.pointer_bytes)? != previous {
            return None;
        }
        if required_marker.is_some_and(|marker| {
            u32_at(pool_raw, offset + layout.active_marker_offset) != Some(marker)
        }) {
            return None;
        }
        let next = read_pointer(pool_raw, offset + layout.next_offset, layout.pointer_bytes)?;
        slots.push(slot_index);
        if current == tail {
            return (next == 0).then_some(slots);
        }
        if next == 0 {
            return None;
        }
        previous = current;
        current = next;
    }
    None
}

pub(crate) fn bullet_root_identity(layout: &BulletLayout, raw: &[u8]) -> Option<BulletRootIdentity> {
    if raw.len() != layout.root_block_bytes {
        return None;
    }
    let pointer = |offset| read_pointer(raw, offset, layout.pointer_bytes);
    Some(BulletRootIdentity {
        free_head: pointer(layout.root_free_head_offset)?,
        free_tail: pointer(layout.root_free_tail_offset)?,
        pool_base: pointer(layout.root_pool_base_offset)?,
        active_head: pointer(layout.root_active_head_offset)?,
        active_tail: pointer(layout.root_active_tail_offset)?,
    })
}

fn bullet_slot_from_pointer(layout: &BulletLayout, pointer: usize, pool_base: usize) -> Option<usize> {
    let offset = pointer.checked_sub(pool_base)?;
    if offset % layout.record_bytes != 0 {
        return None;
    }
    let slot_index = offset / layout.record_bytes;
    (slot_index < layout.capacity).then_some(slot_index)
}

fn u32_at(raw: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        raw.get(offset..offset.checked_add(4)?)?.try_into().ok()?,
    ))
}
