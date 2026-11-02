use bwrapi_internal_model::adapted::{Position, RegionPathingState, RegionState, RegionTileState, TerrainGrid, ValidationStatus};

use bwrapi_layout::{read_pointer, Layout};

pub const BWGAME_MAP_WIDTH_OFFSET: usize = 0xe4;
pub const BWGAME_MAP_DIMENSIONS_BYTES: usize = 4;
pub const ACTIVE_TILE_CELL_BYTES: usize = 4;
pub const ACTIVE_TILE_UNWALKABLE_BIT: u8 = 0x04;
pub const ACTIVE_TILE_HAS_CREEP_BIT: u8 = 0x40;
pub const ACTIVE_TILE_ALWAYS_UNBUILDABLE_BIT: u8 = 0x80;
pub const ACTIVE_TILE_GROUND_HEIGHT_MASK: u8 = 0x07;
pub const ACTIVE_TILE_TEMPORARY_CREEP_BIT: u8 = 0x40;
pub const ACTIVE_TILE_OCCUPIED_BIT: u8 = 0x08;
const REGION_CAPACITY: usize = 5000;

pub fn dimensions(raw: &[u8]) -> Option<(u32, u32)> {
    if raw.len() != 4 { return None; }
    let w = u32::from(u16::from_le_bytes(raw[0..2].try_into().ok()?));
    let h = u32::from(u16::from_le_bytes(raw[2..4].try_into().ok()?));
    // StarCraft map dimensions are build-tile dimensions and may reach the
    // 256x256 pathing grid.  Keep the bound aligned with Pathing's fixed grid.
    (w > 0 && h > 0 && w <= 256 && h <= 256).then_some((w,h))
}
/// Decodes the build-tile fields of BWAPI's ActiveTile layout.
///
/// The walkability bit is deliberately excluded: it is a minitile (8px)
/// property, while this grid is a build-tile (32px) grid.
pub fn static_terrain_tiles(raw: &[u8], width: u32, height: u32) -> Option<(TerrainGrid<bool>, TerrainGrid<i32>)> {
    let (w,h)=(usize::try_from(width).ok()?,usize::try_from(height).ok()?);
    let count=w.checked_mul(h)?;
    if raw.len()!=count.checked_mul(ACTIVE_TILE_CELL_BYTES)? { return None; }
    let status=ValidationStatus::Validated;
    let buildability=raw.chunks_exact(ACTIVE_TILE_CELL_BYTES).map(|cell| {
        Some(cell[2] & (ACTIVE_TILE_ALWAYS_UNBUILDABLE_BIT | ACTIVE_TILE_UNWALKABLE_BIT) == 0)
    }).collect();
    let ground_height=raw.chunks_exact(ACTIVE_TILE_CELL_BYTES).map(|cell| {
        Some(i32::from(cell[3] & ACTIVE_TILE_GROUND_HEIGHT_MASK))
    }).collect();
    Some((
        TerrainGrid::new(w,h,buildability,status).ok()?,
        TerrainGrid::new(w,h,ground_height,status).ok()?,
    ))
}

/// Decodes the per-publication creep bit from the current ActiveTile cells.
/// Buildability and ground height intentionally live in `static_terrain_tiles`
/// so callers can retain them across semantic publications.
pub fn dynamic_creep_tiles(raw: &[u8], width: u32, height: u32) -> Option<TerrainGrid<bool>> {
    let (w,h)=(usize::try_from(width).ok()?,usize::try_from(height).ok()?);
    let count=w.checked_mul(h)?;
    if raw.len()!=count.checked_mul(ACTIVE_TILE_CELL_BYTES)? { return None; }
    let creep=raw.chunks_exact(ACTIVE_TILE_CELL_BYTES).map(|cell| {
        Some(cell[2] & ACTIVE_TILE_HAS_CREEP_BIT != 0 || cell[3] & ACTIVE_TILE_TEMPORARY_CREEP_BIT != 0)
    }).collect();
    TerrainGrid::new(w,h,creep,ValidationStatus::Validated).ok()
}

/// Derives BWAPI-compatible 8px walkability from the validated SAI region
/// split table. Every build tile contributes a row-major 4x4 minitile block.
pub fn walkability_tiles(pathing: &RegionPathingState) -> Option<TerrainGrid<bool>> {
    if pathing.status != ValidationStatus::Validated || pathing.width_tiles == 0 || pathing.height_tiles == 0 {
        return None;
    }
    let expected_tiles = pathing.width_tiles.checked_mul(pathing.height_tiles)?;
    if pathing.tiles.len() != expected_tiles || pathing.regions.is_empty() {
        return None;
    }
    let walk_width = pathing.width_tiles.checked_mul(4)?;
    let walk_height = pathing.height_tiles.checked_mul(4)?;
    let count = walk_width.checked_mul(walk_height)?;
    let mut values = Vec::with_capacity(count);
    for walk_y in 0..walk_height {
        for walk_x in 0..walk_width {
            // BWAPI excludes these engine-border minitiles despite their
            // underlying static minitile flags.
            let engine_border = walk_y >= walk_height.saturating_sub(4)
                || (walk_y >= walk_height.saturating_sub(8)
                    && (walk_x < 20 || walk_x >= walk_width.saturating_sub(20)));
            if engine_border {
                values.push(Some(false));
                continue;
            }
            let tile_x = walk_x / 4;
            let tile_y = walk_y / 4;
            let tile = pathing.tiles.get(tile_x + tile_y.checked_mul(pathing.width_tiles)?)?;
            let minitile = (walk_x & 3) + (walk_y & 3) * 4;
            let region_id = if tile.minitile_mask & (1u16 << minitile) != 0 {
                tile.secondary_region_id?
            } else {
                tile.primary_region_id
            };
            let region = pathing.regions.get(usize::from(region_id))?;
            if region.region_id != region_id {
                return None;
            }
            values.push(Some(region.accessible));
        }
    }
    TerrainGrid::new(walk_width, walk_height, values, ValidationStatus::Validated).ok()
}

/// The SAI walk, for either bitness.
pub fn sai_region_pathing(raw: &[u8], width: u32, height: u32, layout: &Layout) -> Option<RegionPathingState> {
    let map = &layout.map;
    if raw.len()!=map.sai_pathing_allocation_bytes { return None; }
    let (w,h)=(usize::try_from(width).ok()?,usize::try_from(height).ok()?);
    let count=usize::try_from(u32_at(raw,0)?).ok()?;
    if count==0 || count>REGION_CAPACITY || w>256 || h>256 { return None; }
    let mut regions=Vec::with_capacity(count);
    for id in 0..count {
        let o=map.region_table.checked_add(id.checked_mul(map.region_stride)?)?;
        let access=u16_at(raw,o)?;
        let (accessible,higher_ground)=match access {0x1ff9=>(true,true),0x1ffa|0x1ffb=>(true,false),0x1ffd=>(false,false),_=>return None};
        let group_id=u16_at(raw,o+2)?;
        if group_id==0 || (usize::from(group_id)>count && group_id!=0x4000) { return None; }
        let (c,bx)=(o+map.region_center_offset,o+map.region_bounds_offset);
        let center=Position::new(i32::try_from(u32_at(raw,c)? >> 8).ok()?,i32::try_from(u32_at(raw,c+4)? >> 8).ok()?);
        let (l,t,r,b)=(i32::from(u16_at(raw,bx)?),i32::from(u16_at(raw,bx+2)?),i32::from(u16_at(raw,bx+4)?),i32::from(u16_at(raw,bx+6)?));
        if center.x<0||center.y<0||center.x>=i32::try_from(w.checked_mul(32)?).ok()?||center.y>=i32::try_from(h.checked_mul(32)?).ok()?||l>r||t>b||r>i32::try_from(w.checked_mul(32)?).ok()?||b>i32::try_from(h.checked_mul(32)?).ok()? { return None; }
        regions.push(RegionState{
            region_id:u16::try_from(id).ok()?, group_id, center,
            bounds_left:l, bounds_top:t, bounds_right:r, bounds_bottom:b,
            accessible, higher_ground,
            tile_count:u16_at(raw,o+4)?, path_count:*raw.get(o+6)?,
            neighbor_count:*raw.get(o+7)?,
            defense_priority:*raw.get(o+map.region_properties_offset)? & 0x7f,
            neighbor_ids:None,
        });
    }
    let mut tiles=Vec::with_capacity(w.checked_mul(h)?);
    for y in 0..h { for x in 0..w {
        let e=u16_at(raw,map.tile_grid+(y*256+x)*2)?;
        let tile=if e&0xe000==0 { if usize::from(e)>=count{return None}; RegionTileState{primary_region_id:e,secondary_region_id:None,minitile_mask:0} } else {
            let si=usize::from((e&0x1fff).checked_add((e&0xc000)>>1)?);
            let so=map.split_table.checked_add(si.checked_mul(6)?)?;
            let a=u16_at(raw,so+2)?; let b=u16_at(raw,so+4)?;
            if usize::from(a)>=count||usize::from(b)>=count{return None}; RegionTileState{primary_region_id:a,secondary_region_id:Some(b),minitile_mask:u16_at(raw,so)?}
        }; tiles.push(tile);
    }}
    Some(RegionPathingState
{width_tiles:w,height_tiles:h,regions,tiles,status:ValidationStatus::Validated})
}

/// The same record walk for either bitness; the pointer field width follows
/// the layout.
pub fn sai_region_neighbor_source(raw: &[u8], region_id: usize, layout: &Layout) -> Option<(u8, u64)> {
    let map = &layout.map;
    if raw.len() != map.sai_pathing_allocation_bytes { return None; }
    let count = usize::try_from(u32_at(raw, 0)?).ok()?;
    if region_id >= count || count == 0 || count > REGION_CAPACITY { return None; }
    let offset = map.region_table.checked_add(region_id.checked_mul(map.region_stride)?)?;
    Some((
        *raw.get(offset.checked_add(7)?)?,
        read_pointer(raw, offset.checked_add(map.region_neighbors_offset)?, map.pointer_bytes)? as u64,
    ))
}
fn u16_at(raw:&[u8],o:usize)->Option<u16>{Some(u16::from_le_bytes(raw.get(o..o.checked_add(2)?)?.try_into().ok()?))}
fn u32_at(raw:&[u8],o:usize)->Option<u32>{Some(u32::from_le_bytes(raw.get(o..o.checked_add(4)?)?.try_into().ok()?))}

#[cfg(test)]
mod tests {
    use super::dimensions;

    #[test]
    fn dimensions_match_the_256_tile_pathing_extent() {
        assert_eq!(dimensions(&[0, 1, 0, 1]), Some((256, 256)));
        assert_eq!(dimensions(&[0, 1, 0, 2]), None);
    }
}
