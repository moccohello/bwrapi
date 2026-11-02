use crate::columns::flag;
use crate::decode::map::{
    dimensions, ACTIVE_TILE_CELL_BYTES, ACTIVE_TILE_HAS_CREEP_BIT,
    ACTIVE_TILE_OCCUPIED_BIT, ACTIVE_TILE_TEMPORARY_CREEP_BIT, BWGAME_MAP_DIMENSIONS_BYTES,
    BWGAME_MAP_WIDTH_OFFSET,
};
use crate::engine::{Frame, State, Target};

/// Per-tile visible/explored player masks and the map size. A set bit of the
/// tile flags means "not visible" / "not explored" (replays: all bits set), so
/// the masks are the inverted bytes.
pub(super) fn vision(target: &Target, state: &mut State, frame: &mut Frame) {
    if target.root == 0 {
        return;
    }
    let mut size = [0; BWGAME_MAP_DIMENSIONS_BYTES];
    if target.read(target.root + BWGAME_MAP_WIDTH_OFFSET, &mut size).is_err() {
        return;
    }
    let tiles = target.module_pointer(target.layout.map.active_tile_pointer_module_offset).filter(|tiles| *tiles != 0);
    let (Some((width, height)), Some(tiles)) = (dimensions(&size), tiles) else { return };
    frame.map_width = width as u16;
    frame.map_height = height as u16;
    let raw = &mut state.block;
    raw.resize(width as usize * height as usize * ACTIVE_TILE_CELL_BYTES, 0);
    if target.read(tiles, raw).is_err() {
        return;
    }
    frame.vision.visible.extend(raw.chunks_exact(ACTIVE_TILE_CELL_BYTES).map(|cell| u16::from(!cell[0])));
    frame.vision.explored.extend(raw.chunks_exact(ACTIVE_TILE_CELL_BYTES).map(|cell| u16::from(!cell[1])));
    frame.vision.creep.extend(raw.chunks_exact(ACTIVE_TILE_CELL_BYTES).map(|cell| {
        flag(cell[2] & ACTIVE_TILE_HAS_CREEP_BIT != 0 || cell[3] & ACTIVE_TILE_TEMPORARY_CREEP_BIT != 0)
    }));
    frame.vision.occupied.extend(raw.chunks_exact(ACTIVE_TILE_CELL_BYTES).map(|cell| flag(cell[3] & ACTIVE_TILE_OCCUPIED_BIT != 0)));
}
