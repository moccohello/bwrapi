use bwrapi_layout::read_pointer;

use super::{i16_at, u16_at};
use super::pool::Pool;
use crate::columns::Cell;
use crate::engine::Target;
use crate::tables::*;

pub(super) fn render(target: &Target, pool: &Pool, table: &mut Render) {
    for slot in 0..pool.slots() {
        if pool.is_live(slot) {
            let record = pool.record(slot);
            let (x, y, visible_to, animation, frame_set) = render_sprite(target, pool, record);
            table.push(RenderRow { unit: pool.id(slot), x, y, visible_to, animation, frame_set });
        }
    }
}

/// Where the unit's sprite is drawn, in map pixels, and who sees it: the
/// main image's map position and the sprite's visibility flags byte, plus
/// the main image's animation and frame set (the BWAPI attack-frame inputs).
/// The sprite's own position field is encoded in SCR, so the read chain is
/// CUnit -> CSprite -> CImage, and the image's parent link must name the
/// sprite. Two bounded reads per unit; anything unreadable or inconsistent
/// leaves that field unknown.
fn render_sprite(target: &Target, pool: &Pool, record: &[u8]) -> (i16, i16, u16, u8, u16) {
    let unknown = (i16::UNKNOWN, i16::UNKNOWN, u16::UNKNOWN, u8::UNKNOWN, u16::UNKNOWN);
    let layout = target.layout;
    let c = pool.cunit;
    let sprite = match read_pointer(record, c.sprite_ptr_offset, pool.pointer_bytes) {
        Some(sprite) if sprite != 0 => sprite,
        _ => return unknown,
    };
    // One read spans the main-image pointer and the visibility flags byte.
    let csprite = &layout.csprite;
    let start = csprite.main_image_ptr_offset.min(csprite.visibility_flags_offset);
    let end = (csprite.main_image_ptr_offset + pool.pointer_bytes)
        .max(csprite.visibility_flags_offset + 1);
    let mut sprite_raw = [0u8; 64];
    let Some(raw) = sprite_raw.get_mut(..end - start) else { return unknown };
    if target.read(sprite + start, raw).is_err() {
        return unknown;
    }
    let visible_to = u16::from(raw[csprite.visibility_flags_offset - start]);
    let image_unknown = (i16::UNKNOWN, i16::UNKNOWN, visible_to, u8::UNKNOWN, u16::UNKNOWN);
    let image = match read_pointer(raw, csprite.main_image_ptr_offset - start, pool.pointer_bytes) {
        Some(image) if image != 0 => image,
        _ => return image_unknown,
    };
    // One read spans the animation byte, the frameSet/frame pair, the map
    // position that follows them, and the parent link.
    let cimage = &layout.cimage;
    let first = cimage.animation_offset.min(cimage.frame_set_offset);
    let map_x = cimage.frame_set_offset + 4;
    let Some(span) = (cimage.parent_sprite_ptr_offset + pool.pointer_bytes)
        .checked_sub(first) else { return image_unknown };
    let mut image_raw = [0u8; 96];
    let Some(raw) = image_raw.get_mut(..span) else { return image_unknown };
    if target.read(image + first, raw).is_err() {
        return image_unknown;
    }
    if read_pointer(raw, cimage.parent_sprite_ptr_offset - first, pool.pointer_bytes) != Some(sprite) {
        return image_unknown;
    }
    (
        i16_at(raw, map_x - first),
        i16_at(raw, map_x + 2 - first),
        visible_to,
        raw[cimage.animation_offset - first],
        u16_at(raw, cimage.frame_set_offset - first),
    )
}
