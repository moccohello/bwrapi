//! Match-long tables: read once when a match starts, then shared by every
//! frame of the match.

use std::sync::Arc;

use bwrapi_internal_model::adapted::RegionPathingState;

use crate::columns::{flag, Cell, Table};
use crate::decode::map::{
    self, dimensions, ACTIVE_TILE_CELL_BYTES, BWGAME_MAP_DIMENSIONS_BYTES, BWGAME_MAP_WIDTH_OFFSET,
};
use crate::engine::{State, Target};
use crate::tables::*;

/// Reads the terrain, walkability and region tables of the current match.
pub(super) fn read(target: &Target, block: &mut Vec<u8>) -> MatchTables {
    let mut tables = MatchTables::default();
    if target.root == 0 {
        return tables;
    }
    let layout = target.layout;
    let mut size = [0; BWGAME_MAP_DIMENSIONS_BYTES];
    if target.read(target.root + BWGAME_MAP_WIDTH_OFFSET, &mut size).is_err() {
        return tables;
    }
    let Some((width, height)) = dimensions(&size) else { return tables };
    let Some(active_tiles) = target.module_pointer(layout.map.active_tile_pointer_module_offset) else {
        return tables;
    };
    block.resize(width as usize * height as usize * ACTIVE_TILE_CELL_BYTES, 0);
    if target.read(active_tiles, block).is_err() {
        return tables;
    }
    let Some((buildability, ground_height)) = map::static_terrain_tiles(block, width, height) else {
        return tables;
    };
    let Some(creep) = map::dynamic_creep_tiles(block, width, height) else { return tables };

    let mut terrain = Terrain::default();
    let cells = buildability.values.iter().zip(&ground_height.values).zip(&creep.values);
    for ((buildable, ground), has_creep) in cells {
        terrain.push(TerrainRow {
            buildable: flag(buildable.unwrap_or(false)),
            ground_height: ground.map(|value| value as u8).unwrap_or(u8::MAX),
            creep: flag(has_creep.unwrap_or(false)),
            region: u16::UNKNOWN,
            region2: u16::UNKNOWN,
            split_mask: u16::UNKNOWN,
        });
    }

    if let Some((pathing, neighbors, spans)) = read_sai_pathing(target, block, width, height) {
        for (row, tile) in pathing.tiles.iter().enumerate() {
            terrain.region[row] = tile.primary_region_id;
            terrain.region2[row] = tile.secondary_region_id.unwrap_or(tile.primary_region_id);
            terrain.split_mask[row] = tile.minitile_mask;
        }
        if let Some(walkability) = map::walkability_tiles(&pathing) {
            let mut table = Walkability::default();
            for walkable in &walkability.values {
                table.push(WalkabilityRow { walkable: flag(walkable.unwrap_or(false)) });
            }
            tables.walkability = Arc::new(table);
        }
        let mut regions = Regions::default();
        for (region, span) in pathing.regions.iter().zip(spans) {
            regions.push(RegionRow {
                id: region.region_id,
                group: region.group_id,
                center_x: region.center.x as i16,
                center_y: region.center.y as i16,
                left: region.bounds_left as i16,
                top: region.bounds_top as i16,
                right: region.bounds_right as i16,
                bottom: region.bounds_bottom as i16,
                accessible: flag(region.accessible),
                higher_ground: flag(region.higher_ground),
                priority: region.defense_priority,
                neighbor_start: span.0,
                neighbor_count: span.1,
            });
        }
        tables.regions = Arc::new(regions);
        tables.region_neighbors = Arc::new(neighbors);
    }
    tables.terrain = Arc::new(terrain);
    tables
}

/// The match tables of this match, read on first use.
pub(super) fn get(target: &Target, state: &mut State) -> MatchTables {
    if state.shared_root != target.root {
        state.shared = read(target, &mut state.block);
        state.shared_root = target.root;
    }
    state.shared.clone()
}

/// Reads the SAI pathing allocation and the per-region neighbor lists. The
/// allocation and the module pointer are reread as a fence after the neighbor
/// reads; a mismatch drops every region table of this match.
fn read_sai_pathing(
    target: &Target,
    block: &mut Vec<u8>,
    width: u32,
    height: u32,
) -> Option<(RegionPathingState, RegionNeighbors, Vec<(u32, u16)>)> {
    let layout = target.layout;
    let pointer = target.module_pointer(layout.map.sai_pathing_pointer_module_offset)?;
    block.resize(layout.map.sai_pathing_allocation_bytes, 0);
    target.read(pointer, block).ok()?;
    let pathing = map::sai_region_pathing(block, width, height, layout)?;
    let region_count = pathing.regions.len();
    let mut neighbors = RegionNeighbors::default();
    let mut spans = Vec::with_capacity(region_count);
    for region in &pathing.regions {
        spans.push(neighbor_span(target, block, usize::from(region.region_id), region_count, &mut neighbors));
    }
    let mut fresh = vec![0u8; block.len()];
    if target.read(pointer, &mut fresh).is_err()
        || fresh != *block
        || target.module_pointer(layout.map.sai_pathing_pointer_module_offset) != Some(pointer)
    {
        return None;
    }
    Some((pathing, neighbors, spans))
}

/// Appends one region's neighbor ids to `neighbors` and returns their row
/// span, or unknown columns when the list cannot be read consistently.
fn neighbor_span(
    target: &Target,
    raw: &[u8],
    region_id: usize,
    region_count: usize,
    neighbors: &mut RegionNeighbors,
) -> (u32, u16) {
    let unknown = (u32::UNKNOWN, u16::UNKNOWN);
    let Some((count, pointer)) = map::sai_region_neighbor_source(raw, region_id, target.layout) else {
        return unknown;
    };
    if count == 0 {
        return (neighbors.rows() as u32, 0);
    }
    let Some(bytes) = usize::from(count).checked_mul(2) else { return unknown };
    let Some(pointer) = usize::try_from(pointer).ok().filter(|pointer| *pointer != 0 && *pointer % 2 == 0) else {
        return unknown;
    };
    // The external list is read twice and compared before it is trusted.
    let (mut first, mut second) = (vec![0u8; bytes], vec![0u8; bytes]);
    if target.read(pointer, &mut first).is_err()
        || target.read(pointer, &mut second).is_err()
        || first != second
    {
        return unknown;
    }
    let ids: Vec<u16> = first
        .chunks_exact(2)
        .map(|bytes| u16::from_le_bytes([bytes[0], bytes[1]]) & 0x1fff)
        .collect();
    if ids.iter().any(|id| usize::from(*id) >= region_count) {
        return unknown;
    }
    let start = neighbors.rows() as u32;
    for id in ids {
        neighbors.push(RegionNeighborRow { region: id });
    }
    (start, u16::from(count))
}
