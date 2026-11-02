//! The CUnit pool, read once per frame for every source that needs units.

use bwrapi_layout::{read_pointer, CunitLayout};

use super::{i32_at, u16_at};
use crate::columns::Cell;
use crate::decode::cunit::cunit_record_offset_from_target_ptr;
use crate::engine::{State, Target};
use crate::identity::unit_id;

const UNIT_TYPES: u16 = 228;
const MAX_PLAYER: u8 = 11;

/// Records one read covers past its first needed record, and the gap between
/// a needed record and an existing band still worth one call: one call costs
/// about as much as ~40 records of transfer, and live records cluster, so
/// covering ahead of the walk cuts calls without meaningful extra bytes.
const READ_AHEAD: usize = 32;
const MERGE_GAP: usize = 16;

pub(crate) struct Pool<'a> {
    pub raw: &'a [u8],
    pub base: usize,
    pub pointer_bytes: usize,
    pub cunit: &'a CunitLayout,
    /// Slots the lists reached this frame (fallback: every slot).
    pub reached: &'a [u8],
}

/// Address of the first CUnit record, from the game's pool pointer.
pub(crate) fn base(target: &Target) -> Option<usize> {
    let cunit = &target.layout.cunit;
    target
        .module_pointer(cunit.array_pointer_rva)
        .and_then(|pointer| pointer.checked_sub(cunit.selection_ptr_adjustment))
        .filter(|base| *base != 0)
}

/// Live records the pool holds: the game's capacity, capped by the region the
/// layout verified.
pub(crate) fn capacity(target: &Target) -> Option<usize> {
    let cunit = &target.layout.cunit;
    let capacity = target.module_u32(cunit.capacity_pointer_rva)? as usize;
    Some(capacity.min(cunit.region_bytes / cunit.record_stride))
}

/// Reads the pool into `state.cunits` and records its base in
/// `state.pool_base` (0 when it could not be read). The buffer stays slot
/// indexed at full capacity; the game's live-unit lists say which records are
/// alive, and only bands of records around them are refreshed. When the
/// lists cannot be read or do not validate, the whole pool is read and every
/// slot counts as reached, which is the whole-pool scan.
pub(super) fn read(target: &Target, state: &mut State) {
    let cunit = &target.layout.cunit;
    state.pool_base = 0;
    let Some(base) = base(target) else {
        return;
    };
    let slots = capacity(target).unwrap_or(0);
    if slots == 0 {
        return;
    }
    // A different pool (new match) must not keep the previous match's bytes.
    if state.pool_base_of != base || state.cunits.len() != slots * cunit.record_stride {
        state.pool_base_of = base;
        state.cunits.clear();
        state.cunits.resize(slots * cunit.record_stride, 0);
    }
    state.reached.clear();
    state.reached.resize(slots, 0);
    if read_listed_bands(target, state, base, slots) {
        state.pool_base = base;
    } else if target.read(base, &mut state.cunits).is_ok() {
        state.reached.fill(1);
        state.pool_base = base;
    }
}

/// One doubly-linked list of CUnit records: native Unit pointers for the
/// first (and last) node, links at record offsets inside each node.
struct ListShape {
    first: usize,
    last: Option<usize>,
    prev_link: usize,
    next_link: usize,
}

/// Walks the game's live-unit lists and refreshes bands of records around
/// every slot they reach; `state.reached` marks the reached slots (as the
/// owning list id plus one). `false` when a head cannot be read or a chain
/// breaks — the caller then reads the whole pool, so a broken list never
/// loses units, it only costs the read.
fn read_listed_bands(target: &Target, state: &mut State, base: usize, slots: usize) -> bool {
    let cunit = &target.layout.cunit;
    let Some(lists) = cunit.unit_lists else {
        return false;
    };
    state.block.clear();
    state.block.resize(cunit.native_unit_list_header_bytes, 0);
    if target.read_module(cunit.native_unit_list_header_module_offset, &mut state.block).is_err() {
        return false;
    }
    let width = target.layout.pointer_bytes;
    let header_pointer = |at: usize| read_pointer(&state.block, at, width);
    let invisible_first = match target.module_pointer(lists.invisible_list_first_module_rva) {
        Some(pointer) => pointer,
        None => return false,
    };
    let pylon_first = match target.module_pointer(lists.pylon_list_first_module_rva) {
        Some(pointer) => pointer,
        None => return false,
    };
    let mut shapes = Vec::with_capacity(6);
    for (first, last) in lists.header_list_first_offsets.into_iter().zip(lists.header_list_last_offsets) {
        let (Some(first), Some(last)) = (header_pointer(first), header_pointer(last)) else {
            return false;
        };
        shapes.push(ListShape {
            first,
            last: Some(last),
            prev_link: lists.header_list_prev_link_offset,
            next_link: lists.header_list_next_link_offset,
        });
    }
    shapes.push(ListShape {
        first: invisible_first,
        last: None,
        prev_link: lists.invisible_list_prev_link_offset,
        next_link: lists.invisible_list_next_link_offset,
    });
    shapes.push(ListShape {
        first: pylon_first,
        last: None,
        prev_link: lists.pylon_list_prev_link_offset,
        next_link: lists.pylon_list_next_link_offset,
    });
    state.block.clear();
    // A block that names no unit at all proves nothing about the pool — a
    // zeroed or foreign header (fixtures, an image without the lists) — so
    // only lists that reach at least one record may bound the read.
    if shapes.iter().all(|shape| shape.first == 0) {
        return false;
    }
    // Record ranges refreshed so far; the walk extends them as it meets
    // nearby slots.
    let mut bands: Vec<(usize, usize)> = Vec::new();
    for (id, shape) in shapes.into_iter().enumerate() {
        if shape.last.is_some_and(|last| (last == 0) != (shape.first == 0))
            || !walk_list(target, state, base, slots, &mut bands, id, &shape)
        {
            return false;
        }
    }
    true
}

/// Walks one list end to end, refreshing a band around every record it
/// reaches and marking the slot in `state.reached`. False when the chain
/// breaks: a pointer outside the pool, a slot twice in one list, a previous
/// link that does not name the walked node, or a last pointer the chain does
/// not end on.
fn walk_list(
    target: &Target,
    state: &mut State,
    base: usize,
    slots: usize,
    bands: &mut Vec<(usize, usize)>,
    id: usize,
    shape: &ListShape,
) -> bool {
    let cunit = &target.layout.cunit;
    let width = target.layout.pointer_bytes;
    let stride = cunit.record_stride;
    let mut previous = 0;
    let mut current = shape.first;
    while current != 0 {
        let slot = match cunit_record_offset_from_target_ptr(current, base, cunit) {
            Some(offset) if offset % stride == 0 => offset / stride,
            _ => return false,
        };
        if slot >= slots || state.reached[slot] == id as u8 + 1 {
            return false;
        }
        if !cover(target, state, base, slots, bands, slot) {
            return false;
        }
        let record = &state.cunits[slot * stride..][..stride];
        if read_pointer(record, shape.prev_link, width) != Some(previous) {
            return false;
        }
        state.reached[slot] = id as u8 + 1;
        previous = current;
        current = match read_pointer(record, shape.next_link, width) {
            Some(next) => next,
            None => return false,
        };
    }
    shape.last.is_none_or(|last| last == previous)
}

/// Refreshes a band of records around `slot` into the pool buffer: the
/// closest band within `MERGE_GAP` extends over the gap, otherwise a new
/// band opens covering `READ_AHEAD` records.
fn cover(target: &Target, state: &mut State, base: usize, slots: usize, bands: &mut Vec<(usize, usize)>, slot: usize) -> bool {
    if bands.iter().any(|&(start, end)| slot >= start && slot < end) {
        return true;
    }
    let stride = target.layout.cunit.record_stride;
    let mut low = slot;
    let mut high = (slot + READ_AHEAD).min(slots);
    let mut merged = false;
    for (start, end) in bands.iter_mut() {
        if *end <= slot && slot - *end < MERGE_GAP {
            low = *end;
            *end = high;
            merged = true;
            break;
        }
        if *start > slot && *start - slot <= MERGE_GAP {
            high = *start;
            low = slot.saturating_sub(READ_AHEAD);
            *start = low;
            merged = true;
            break;
        }
    }
    if !merged {
        bands.push((low, high));
    }
    target.read(base + low * stride, &mut state.cunits[low * stride..high * stride]).is_ok()
}

/// The pool read this frame.
pub(super) fn get<'a>(target: &Target, state: &'a State) -> Option<Pool<'a>> {
    (state.pool_base != 0).then(|| Pool {
        raw: &state.cunits,
        base: state.pool_base,
        pointer_bytes: target.layout.pointer_bytes,
        cunit: &target.layout.cunit,
        reached: &state.reached,
    })
}

impl Pool<'_> {
    pub fn record(&self, slot: usize) -> &[u8] {
        &self.raw[slot * self.cunit.record_stride..][..self.cunit.record_stride]
    }

    pub fn slots(&self) -> usize {
        self.raw.len() / self.cunit.record_stride
    }

    /// A slot the lists reached this frame whose record still reads as a
    /// live unit; unreached slots hold stale or zero bytes and never are rows.
    pub fn is_live(&self, slot: usize) -> bool {
        let c = self.cunit;
        slot < self.slots()
            && self.reached[slot] != 0
            && {
                let record = self.record(slot);
                record[c.owner_offset] <= MAX_PLAYER
                    && u16_at(record, c.type_offset) < UNIT_TYPES
                    && record[c.order_offset] != 0
                    && i32_at(record, c.hit_points_offset) > 0
            }
    }

    pub fn id(&self, slot: usize) -> u64 {
        unit_id(slot as u16, self.record(slot)[self.cunit.uniqueness_identifier_offset])
    }

    /// Slot a unit pointer refers to (record or selection pointer).
    pub fn slot_of(&self, pointer: usize) -> Option<usize> {
        cunit_record_offset_from_target_ptr(pointer, self.base, self.cunit)
            .map(|offset| offset / self.cunit.record_stride)
            .filter(|slot| *slot < self.slots())
    }

    /// Slot a unit pointer field of `record` refers to.
    pub fn target_slot(&self, record: &[u8], field: usize) -> Option<usize> {
        self.slot_of(read_pointer(record, field, self.pointer_bytes)?)
    }

    /// Id of the unit a pointer refers to: 0 for none, unknown when it is
    /// outside the pool.
    pub fn pointer_id(&self, pointer: usize) -> u64 {
        match pointer {
            0 => 0,
            _ => self.slot_of(pointer).map_or(u64::UNKNOWN, |slot| self.id(slot)),
        }
    }

    /// Id of the unit a pointer field of `record` refers to.
    pub fn target_id(&self, record: &[u8], field: usize) -> u64 {
        read_pointer(record, field, self.pointer_bytes).map_or(u64::UNKNOWN, |pointer| self.pointer_id(pointer))
    }
}
