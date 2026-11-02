use bwrapi_layout::read_pointer;

use super::pool::Pool;
use super::{i16_at, u16_at};
use crate::engine::Target;
use crate::tables::*;

const SELECTION_SLOTS: usize = 12;

/// The local player's selection, from the table the game draws it from.
pub(super) fn selection(target: &Target, pool: &Pool, table: &mut Selection) -> bool {
    let width = target.layout.pointer_bytes;
    let mut raw = [0; SELECTION_SLOTS * 8];
    let raw = &mut raw[..SELECTION_SLOTS * width];
    if target.read_module(target.layout.selection.ui_table, raw).is_err() {
        return false;
    }
    for slot in 0..SELECTION_SLOTS {
        match read_pointer(raw, slot * width, width) {
            Some(0) | None => {}
            Some(pointer) => table.push(SelectionRow { unit: pool.pointer_id(pointer) }),
        }
    }
    true
}

/// Queued orders of every live unit, in queue order. The list can hold more
/// nodes than the unit's queue count; only the counted ones are queued orders.
pub(super) fn orders(target: &Target, pool: &Pool, table: &mut Orders) {
    let c = pool.cunit;
    let corder = &target.layout.corder;
    let width = target.layout.pointer_bytes;
    let mut node = [0; 0x40];
    let node = &mut node[..corder.read_bytes];
    for slot in 0..pool.slots() {
        if !pool.is_live(slot) {
            continue;
        }
        let record = pool.record(slot);
        let (Some(mut at), Some(tail)) = (
            read_pointer(record, c.order_queue_head_offset, width),
            read_pointer(record, c.order_queue_tail_offset, width),
        ) else {
            continue;
        };
        let unit = pool.id(slot);
        let count = usize::from(record[c.order_queue_count_offset]);
        let mut index = 0;
        while at != 0 && index < count && target.read(at, node).is_ok() {
            table.push(OrderRow {
                unit,
                index: index as u8,
                order: node[corder.order_offset],
                unit_type: u16_at(node, corder.unit_type_offset),
                x: i16_at(node, corder.target_x_offset),
                y: i16_at(node, corder.target_y_offset),
                target: read_pointer(node, corder.target_unit_ptr_offset, width).map_or(0, |pointer| pool.pointer_id(pointer)),
            });
            if at == tail {
                break;
            }
            at = read_pointer(node, corder.next_offset, width).unwrap_or(0);
            index += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use bwrapi_layout::Layout;

    use crate::fake::{in_game, put_unit, Shared, MODULE_BASE, POOL};
    use crate::tables::source;
    use crate::{unit_id, Engine};

    const NODES: usize = 0x5_0000_0000;

    #[test]
    fn selection_names_the_selected_units() {
        let layout = &Layout::X64;
        let memory = in_game(8);
        put_unit(&memory, 3, 7, 0, 10, 10, 4);
        let mut table = [0u8; 12 * 8];
        // The game stores selection pointers as record + 0x10.
        let pointer = POOL + 3 * layout.cunit.record_stride + layout.cunit.selection_ptr_adjustment;
        table[..8].copy_from_slice(&(pointer as u64).to_le_bytes());
        memory.put(MODULE_BASE + layout.selection.ui_table, &table);
        let mut engine = Engine::with_memory(Box::new(Shared(memory.clone()))).unwrap();
        engine.subscribe(source::SELECTION);
        let frame = engine.acquire().unwrap();
        assert_eq!(frame.selection.unit, [unit_id(3, 4)]);
        assert_eq!(frame.sources, source::SELECTION);
    }

    #[test]
    fn an_unreadable_selection_table_leaves_the_source_out() {
        let mut engine = Engine::with_memory(Box::new(Shared(in_game(4)))).unwrap();
        engine.subscribe(source::SELECTION | source::UNITS);
        assert_eq!(engine.acquire().unwrap().sources, source::UNITS);
    }

    #[test]
    fn orders_follow_each_queue_from_head_to_tail() {
        let layout = &Layout::X64;
        let (c, o) = (&layout.cunit, &layout.corder);
        let memory = in_game(8);
        put_unit(&memory, 1, 7, 0, 10, 10, 2);
        put_unit(&memory, 2, 0, 0, 20, 20, 5);
        let node = |next: usize, order: u8, x: i16, target: usize| {
            let mut raw = vec![0; o.read_bytes];
            raw[o.next_offset..][..8].copy_from_slice(&(next as u64).to_le_bytes());
            raw[o.order_offset] = order;
            raw[o.target_x_offset..][..2].copy_from_slice(&x.to_le_bytes());
            raw[o.target_unit_ptr_offset..][..8].copy_from_slice(&(target as u64).to_le_bytes());
            raw
        };
        let target = POOL + 2 * c.record_stride + c.selection_ptr_adjustment;
        memory.put(NODES, &node(NODES + 0x100, 6, 100, 0));
        memory.put(NODES + 0x100, &node(0, 10, 200, target));
        let record = POOL + c.record_stride;
        memory.patch(record + c.order_queue_head_offset, &(NODES as u64).to_le_bytes());
        memory.patch(record + c.order_queue_tail_offset, &((NODES + 0x100) as u64).to_le_bytes());
        memory.patch(record + c.order_queue_count_offset, &[2]);

        let mut engine = Engine::with_memory(Box::new(Shared(memory.clone()))).unwrap();
        engine.subscribe(source::ORDERS);
        let frame = engine.acquire().unwrap();
        assert_eq!(frame.orders.unit, [unit_id(1, 2); 2]);
        assert_eq!(frame.orders.index, [0, 1]);
        assert_eq!(frame.orders.order, [6, 10]);
        assert_eq!(frame.orders.x, [100, 200]);
        assert_eq!(frame.orders.target, [0, unit_id(2, 5)]);
    }
}
