#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum ValidationStatus {
    #[default]
    Pending,
    DiagnosticOnly,
    StrongCandidate,
    Validated,
    Accepted,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Visibility {
    #[default]
    Unknown,
    Visible,
    Fogged,
    Hidden,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Position {

    pub x: i32,
    pub y: i32,
}

impl Position {
    pub fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerrainGrid<T> {
    pub width: usize,
    pub height: usize,
    pub values: Vec<Option<T>>,
    pub status: ValidationStatus,
}

impl<T> TerrainGrid<T> {
    pub fn new(
        width: usize,
        height: usize,
        values: Vec<Option<T>>,
        status: ValidationStatus,
    ) -> Result<Self, String> {
        if width == 0 || height == 0 {
            return Err("terrain grid width and height must be positive".to_owned());
        }
        if values.len() != width * height {
            return Err("terrain grid value count must match width * height".to_owned());
        }
        Ok(Self {
            width,
            height,
            values,
            status,
        })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RegionState {
    pub region_id: u16,
    pub group_id: u16,
    pub center: Position,
    pub bounds_left: i32,
    pub bounds_top: i32,
    pub bounds_right: i32,
    pub bounds_bottom: i32,
    pub accessible: bool,
    pub higher_ground: bool,
    pub tile_count: u16,
    pub path_count: u8,
    pub neighbor_count: u8,
    pub defense_priority: u8,
    /// `None` means that the external SAI neighbor array was unavailable or
    /// unstable; `Some(vec![])` is the known zero-neighbor case.
    pub neighbor_ids: Option<Vec<u16>>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RegionTileState { pub primary_region_id: u16, pub secondary_region_id: Option<u16>, pub minitile_mask: u16 }

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RegionPathingState {
    pub width_tiles: usize,
    pub height_tiles: usize,
    pub regions: Vec<RegionState>,
    pub tiles: Vec<RegionTileState>,
    pub status: ValidationStatus,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terrain_grid_validates_its_shape() {
        assert!(TerrainGrid::new(2, 1, vec![Some(true), None], ValidationStatus::Validated).is_ok());
        assert!(TerrainGrid::new(2, 2, vec![Some(true)], ValidationStatus::Pending).is_err());
        assert!(TerrainGrid::new(0, 1, Vec::<Option<bool>>::new(), ValidationStatus::Pending).is_err());
    }
}
