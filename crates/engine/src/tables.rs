//! Source tables. A frame carries the tables of the sources it was asked for.
//! Unit ids are `(slot + 1) | uniqueness << 16` (see `identity`); 0 is "no
//! unit".
//!
//! The `///` comments of the tables and columns are the column reference of
//! the public C ABI: the generated header, bindings and `docs/C-ABI.md`
//! print them. Positions are map pixels unless a comment says build tiles
//! (32 pixels) or walk tiles (8 pixels). Unit types, orders, techs, upgrades
//! and bullet types use the game's ids, which are BWAPI's numbering.

use crate::columns::{table, ColumnInfo, Table, Text256, Text32};

/// Source bits for `Engine::subscribe`.
pub mod source {
    pub const PLAYERS: u64 = 1 << 0;
    pub const UNITS: u64 = 1 << 1;
    pub const UPGRADES: u64 = 1 << 2;
    pub const TECHS: u64 = 1 << 3;
    pub const UNIT_COUNTS: u64 = 1 << 4;
    pub const BULLETS: u64 = 1 << 5;
    pub const VISION: u64 = 1 << 6;
    pub const VIEWPORT: u64 = 1 << 7;
    pub const SELECTION: u64 = 1 << 8;
    pub const ORDERS: u64 = 1 << 9;
    pub const RENDER: u64 = 1 << 10;
    pub const GAME: u64 = 1 << 11;
    pub const FORCES: u64 = 1 << 12;
    pub const START_LOCATIONS: u64 = 1 << 13;
    pub const UNIT_AVAILABILITY: u64 = 1 << 14;
    pub const TERRAIN: u64 = 1 << 15;
    pub const WALKABILITY: u64 = 1 << 16;
    pub const REGIONS: u64 = 1 << 17;
    pub const REGION_NEIGHBORS: u64 = 1 << 18;
}

table!(
    /// One row per active player, player ids 0..7; empty, closed and left
    /// computer slots are left out, and the neutral player (11) has no row.
    Players, PlayerRow {
        /// Game player id 0..7.
        id: u8,
        /// 0 Zerg, 1 Terran, 2 Protoss; other values are the game's race byte
        /// (BWAPI Races numbering).
        race: u8,
        /// Current minerals.
        minerals: i32,
        /// Current gas.
        gas: i32,
        /// Player name as the game stores it: UTF-8, or the client's legacy
        /// code page for names it localizes (a Korean client's computer names
        /// are CP949). Empty when unknown.
        name: Text32,
        /// Game player type byte (BWAPI PlayerTypes numbering).
        player_type: u8,
        /// Force 1..4; 0 none.
        force: u8,
        /// Game color index.
        color: u8,
        /// Zerg supply used, in half units (twice the supply the game shows).
        supply_used_zerg: u32,
        /// Terran supply used, in half units.
        supply_used_terran: u32,
        /// Protoss supply used, in half units.
        supply_used_protoss: u32,
        /// Zerg supply total, in half units.
        supply_total_zerg: u32,
        /// Terran supply total, in half units.
        supply_total_terran: u32,
        /// Protoss supply total, in half units.
        supply_total_protoss: u32,
        /// Start location x in build tiles: the top-left tile of the start
        /// resource depot; unknown when the player has none or the slot is
        /// unaligned, off the map or repeated.
        start_x: i16,
        /// Start location y in build tiles.
        start_y: i16,
        /// Raw victory status byte: 3 victory; 1, 2, 4 and 6 defeat (as BWAPI
        /// reads it, for players 0..7).
        victory: u8,
        /// 1 when the player left the game.
        left_game: u8,
        /// Minerals gathered this match.
        gathered_minerals: i32,
        /// Gas gathered this match.
        gathered_gas: i32,
        /// The game's unit score.
        unit_score: i32,
        /// The game's kill score.
        kill_score: i32,
        /// The game's building score.
        building_score: i32,
        /// The game's razing score.
        razing_score: i32,
        /// Score the map's triggers set.
        custom_score: i32,
        /// Bit `p` set: allied with player `p`.
        allies: u16,
        /// BWAPI match-start participation: the player owns a unit, or a
        /// trigger whose conditions can be met gives or creates units for
        /// it. Read on game frame 0 only, while the start units and trigger
        /// lists are still the match's own; unknown on later frames and for
        /// trigger modes it cannot read.
        participating: u8,
        /// Minerals paid for repairs this match; unknown without the in-game
        /// DLL.
        repaired_minerals: i32,
        /// Gas paid for repairs this match; unknown without the in-game DLL.
        repaired_gas: i32,
        /// Minerals got back from cancels this match; unknown without the
        /// in-game DLL.
        refunded_minerals: i32,
        /// Gas got back from cancels this match; unknown without the in-game
        /// DLL.
        refunded_gas: i32,
    }
);

table!(
    /// One row per live unit of the game's unit pool, in pool slot order.
    /// Unit id columns other than `id` are 0 for no unit and unknown when the
    /// game's link points outside the unit pool. `*_timer` columns are the
    /// game's raw timer values, as BWAPI reports them.
    Units, UnitRow {
        /// Unit id, `(slot + 1) | uniqueness << 16`; stable for the unit's life.
        id: u64,
        /// Unit type (BWAPI UnitTypes numbering).
        unit_type: u16,
        /// Owning player id.
        owner: u8,
        /// Center x in map pixels.
        x: i16,
        /// Center y in map pixels.
        y: i16,
        /// Hit points in 1/256 units; the shown value is this / 256, rounded
        /// up.
        hit_points: i32,
        /// Shields in 1/256 units, rounded up for the shown value; 0 for types
        /// without shields.
        shields: i32,
        /// Energy in 1/256 units, rounded up for the shown value; unknown for
        /// types without energy.
        energy: u16,
        /// Remaining minerals or gas; unknown for non-resource types.
        resources: u16,
        /// Current order (BWAPI Orders numbering).
        order: u8,
        /// Unit the current order targets; 0 none.
        order_target: u64,
        /// Target position x of the current order, map pixels.
        order_target_x: i16,
        /// Target position y of the current order, map pixels.
        order_target_y: i16,
        /// Secondary order (BWAPI Orders numbering); unknown when out of range.
        secondary_order: u8,
        /// Raw CUnit status flags.
        status_flags: u32,
        /// Bit `p` set: detected by player `p`.
        detected_by: u32,
        /// 1 when the unit is completed; a morphing unit reads 0.
        completed: u8,
        /// 1 when burrowed.
        burrowed: u8,
        /// 1 when cloaked and not burrowed.
        cloaked: u8,
        /// 1 for an air unit or a lifted building.
        in_air: u8,
        /// 1 for a hallucination.
        hallucination: u8,
        /// 1 when the unit accepts new orders.
        interruptible: u8,
        /// 0 for a Protoss building without power; 1 otherwise.
        powered: u8,
        /// Status bit: the unit cannot be damaged.
        invincible: u8,
        /// 1 inside a transport or a Bunker; unknown when that carrier cannot
        /// be resolved.
        loaded: u8,
        /// Transport or Bunker carrying the unit; 0 none.
        transport: u64,
        /// Addon of a Terran building; 0 none.
        addon: u64,
        /// Hatchery of a larva; 0 none.
        hatchery: u64,
        /// Unit type being built, trained or morphed into; 228 (None) when
        /// nothing.
        build_type: u16,
        /// Frames left to construct or morph this unit.
        remaining_build_frames: u32,
        /// 1 while the unit researches a tech.
        researching: u8,
        /// 1 while the unit researches an upgrade.
        upgrading: u8,
        /// Spider mines of a Vulture; unknown for other types.
        spider_mines: u8,
        /// Lockdown timer.
        lockdown_timer: u8,
        /// Maelstrom timer.
        maelstrom_timer: u8,
        /// Stasis Field timer.
        stasis_timer: u8,
        /// Irradiate timer.
        irradiate_timer: u8,
        /// Plague timer.
        plague_timer: u8,
        /// Ensnare timer.
        ensnare_timer: u8,
        /// Defensive Matrix timer.
        defense_matrix_timer: u8,
        /// Time left before a temporary unit (a hallucination, a Broodling)
        /// is removed; 0 for a lasting unit.
        remove_timer: u16,
        /// Rally point x in map pixels, for unit types that rally; unknown
        /// otherwise or while no rally point is set.
        rally_x: i16,
        /// Rally point y in map pixels; unknown as `rally_x`.
        rally_y: i16,
        /// Unit the rally point follows; 0 none, unknown for types that do
        /// not rally.
        rally_unit: u64,
        /// Refinery/Extractor/Assimilator a gas worker gathers from; 0 none.
        gas_building: u64,
        /// Frames until a hatchery spawns its next larva; 0 with three larvae;
        /// unknown for other types.
        larva_frames: u32,
        /// Frames left on the unit at the front of the training queue.
        remaining_train_frames: u32,
        /// Training queue slot 0 (front), a unit type; unknown past the
        /// queue's end.
        train_queue_0: u16,
        /// Training queue slot 1, a unit type; unknown past the queue's end.
        train_queue_1: u16,
        /// Training queue slot 2, a unit type; unknown past the queue's end.
        train_queue_2: u16,
        /// Training queue slot 3, a unit type; unknown past the queue's end.
        train_queue_3: u16,
        /// Training queue slot 4, a unit type; unknown past the queue's end.
        train_queue_4: u16,
        /// Tech being researched (BWAPI TechTypes numbering); unknown when
        /// none.
        tech: u8,
        /// Frames left on the research; unknown when none.
        remaining_research_frames: u32,
        /// Upgrade in progress (BWAPI UpgradeTypes numbering); unknown when
        /// none.
        upgrade: u8,
        /// Frames left on the upgrade; unknown when none.
        remaining_upgrade_frames: u32,
        /// Unit this unit is constructing (a building, an addon, a nydus
        /// exit); 0 none.
        build_unit: u64,
        /// Unit the movement heads for; 0 none.
        move_target: u64,
        /// Movement target x in map pixels.
        move_target_x: i16,
        /// Movement target y in map pixels.
        move_target_y: i16,
        /// Turret of a Siege Tank or Goliath; 0 none.
        sub_unit: u64,
        /// Carrier of an Interceptor; 0 none; unknown for every other type.
        carrier: u64,
        /// Exit of a Nydus Canal; 0 none.
        nydus_exit: u64,
        /// Flag or mineral chunk a completed worker carries; 0 none; unknown for
        /// every other unit.
        power_up: u64,
        /// Order timer.
        order_timer: u16,
        /// Stim Packs timer.
        stim_timer: u8,
        /// Psionic Storm timer.
        storm_timer: u8,
        /// Damage the Defensive Matrix still absorbs, in 1/256 units.
        defense_matrix_damage: u16,
        /// Frames until the ground weapon can fire again.
        ground_cooldown: u8,
        /// Frames until the air weapon can fire again.
        air_cooldown: u8,
        /// Spell cooldown.
        spell_cooldown: u8,
        /// Units this unit killed.
        kill_count: u16,
        /// Acid spores on the unit.
        acid_spores: u8,
        /// Scarabs a Reaver holds; unknown for other types.
        scarabs: u8,
        /// Interceptors a Carrier has, docked or out; unknown for other types.
        interceptors: u8,
        /// Resource group of a resource container; unknown for other types.
        resource_group: u8,
        /// Resource a worker carries: 0 none, 1 gas, 2 minerals; unknown for
        /// other types.
        carry: u8,
        /// Player that last attacked the unit; unknown when none.
        last_attacker: u8,
        /// 1 while the unit was recently attacked (the game flashes it on the
        /// minimap).
        recently_attacked: u8,
        /// Game button set id.
        button_set: u16,
        /// Hit points at the previous update, in 1/256 units.
        previous_hit_points: u16,
        /// 1 while being healed.
        being_healed: u8,
        /// 1 while a worker gathers; unknown for other types.
        gathering: u8,
        /// A worker gathers from or waits in line at this resource
        /// container; unknown for every other type.
        being_gathered: u8,
        /// 1 when blinded by Optical Flare.
        blind: u8,
        /// 1 when parasited.
        parasited: u8,
        /// 1 when a Nuclear Silo holds a ready nuke.
        has_nuke: u8,
        /// 1 when the unit is stuck and the game moves it to a legal spot.
        stuck: u8,
        /// 1 while moving.
        moving: u8,
        /// 1 while accelerating.
        accelerating: u8,
        /// 1 while braking.
        braking: u8,
        /// Facing direction 0..255, 1/256 turn per step; BWAPI's angle is
        /// `(facing - 64) * pi / 128`.
        facing: u16,
        /// Velocity x in 1/256 pixel per frame.
        velocity_x: i32,
        /// Velocity y in 1/256 pixel per frame.
        velocity_y: i32,
    }
);

table!(
    /// Unit counts per player and type; rows with every count zero are left out.
    UnitCounts, UnitCountRow {
        /// Player id.
        player: u8,
        /// Unit type (BWAPI UnitTypes numbering).
        unit_type: u16,
        /// Units of the type the player has, incomplete ones included.
        all: u32,
        /// Completed units of the type the player has.
        completed: u32,
        /// Units of the type the player killed.
        killed: u32,
        /// Units of the type the player lost.
        dead: u32,
    }
);

table!(
    /// Upgrade state per active player and upgrade type; upgrades a player
    /// cannot get (maximum level 0) and rows with a level above the maximum
    /// or a maximum above 3 are left out.
    Upgrades, UpgradeRow {
        /// Player id.
        player: u8,
        /// Upgrade type (BWAPI UpgradeTypes numbering).
        upgrade_type: u16,
        /// Current level.
        level: u8,
        /// Highest level the player can reach.
        max_level: u8,
        /// 1 while the next level is being researched.
        in_progress: u8,
        /// Frames left while in progress, 0 when not; unknown when no
        /// building is seen working on it.
        remaining_frames: u32,
    }
);

table!(
    /// Tech state per active player and tech type.
    Techs, TechRow {
        /// Player id.
        player: u8,
        /// Tech type (BWAPI TechTypes numbering).
        tech_type: u16,
        /// 1 when researched.
        researched: u8,
        /// 1 when the player may research it.
        available: u8,
        /// 1 while being researched.
        in_progress: u8,
        /// Frames left while in progress, 0 when not; unknown when no
        /// building is seen working on it.
        remaining_frames: u32,
    }
);

table!(
    /// One row per active bullet: projectiles and weapon effects.
    Bullets, BulletRow {
        /// Bullet id: its pool slot + 1.
        id: u64,
        /// Bullet type (BWAPI BulletTypes numbering).
        bullet_type: u16,
        /// Player of the source unit; unknown without one.
        owner: u8,
        /// Position x in map pixels.
        x: i16,
        /// Position y in map pixels.
        y: i16,
        /// Unit that fired it; 0 none.
        source: u64,
        /// Unit it flies at; 0 none.
        target: u64,
        /// Time left before the bullet is removed.
        remove_timer: u8,
        /// Bit `p` set: visible to player `p`.
        visible_to: u32,
        /// Target position x in map pixels.
        target_x: i16,
        /// Target position y in map pixels.
        target_y: i16,
        /// Raw velocity x (1/256 pixel per frame).
        velocity_x: i32,
        /// Raw velocity y (1/256 pixel per frame).
        velocity_y: i32,
        /// Raw direction (0..255, 0 = up).
        direction: u8,
    }
);

table!(
    /// One row per build tile, row-major (`y * map_width + x`).
    Vision, VisionRow {
        /// Bit `p` set: tile visible to player `p` (players 0..7).
        visible: u16,
        /// Bit `p` set: tile explored by player `p` (players 0..7).
        explored: u16,
        /// Creep on the tile now, the map's own creep included.
        creep: u8,
        /// A building stands on the tile.
        occupied: u8,
    }
);

table!(
    /// One row: the local screen and the cursor.
    Viewport, ViewportRow {
        /// Left edge of the local screen in map pixels.
        x: i32,
        /// Top edge of the local screen in map pixels.
        y: i32,
        /// Cursor x in game screen pixels; unknown where the build has no
        /// cursor address.
        mouse_x: i32,
        /// Cursor y in game screen pixels; unknown as `mouse_x`.
        mouse_y: i32,
    }
);

table!(
    /// The local player's current selection, in selection order.
    Selection, SelectionRow {
        /// Selected unit id.
        unit: u64,
    }
);

table!(
    /// Queued orders after each unit's current order, in queue order.
    Orders, OrderRow {
        /// Unit whose queue holds the order.
        unit: u64,
        /// Position in the queue, from 0.
        index: u8,
        /// Queued order (BWAPI Orders numbering).
        order: u8,
        /// Unit type to build, for build orders.
        unit_type: u16,
        /// Target position x in map pixels.
        x: i16,
        /// Target position y in map pixels.
        y: i16,
        /// Target unit; 0 none.
        target: u64,
    }
);

table!(
    /// One row: the running game.
    Game, GameRow {
        /// BWAPI GameTypes numbering; unknown when the build has no address
        /// for it.
        game_type: u8,
        /// Records the CUnit pool of this game holds.
        unit_slots: u16,
        /// 1 in a network match.
        multiplayer: u8,
        /// 1 in a Battle.net game.
        battle_net: u8,
        /// 1 while paused.
        paused: u8,
        /// Raw latency setting (0 low, 1 high, 2 extra high).
        latency: u8,
        /// Command latency in game frames.
        latency_frames: u32,
        /// Turns the game has sent; it sends one right before each frame it
        /// steps and keeps sending while paused.
        turns_sent: u32,
        /// QPC of the last turn send: the in-game watcher's stamp of the
        /// latest `turns_sent` change it saw. Unknown without the watcher or
        /// while its count is not this row's.
        last_turn_qpc: u64,
        /// Milliseconds per game frame at the current speed.
        frame_ms: u32,
        /// Game time in seconds, as the game counts it.
        elapsed_seconds: i32,
        /// Random seed the match started with.
        random_seed: u32,
        /// Frame count of the loaded replay.
        replay_frames: u32,
        /// Seconds left on the map's countdown timer.
        countdown_seconds: u32,
        /// Tick count (ms) of the local player's last save, 0 before the
        /// first.
        save_tick: u32,
        /// Name of the local player's last save.
        save_name: Text32,
        /// Map file name without its directory.
        map_file: Text256,
        /// Map file path as the game holds it.
        map_path: Text256,
        /// Map title.
        map_title: Text256,
    }
);

table!(
    /// The map's forces (teams).
    Forces, ForceRow {
        /// 1..4.
        id: u8,
        /// Force name.
        name: Text32,
    }
);

table!(
    /// Start locations of the map in slot order; unused and repeated ones are
    /// left out.
    StartLocations, StartLocationRow {
        /// Top-left build tile x of the start resource depot.
        x: i16,
        /// Top-left build tile y of the start resource depot.
        y: i16,
    }
);

table!(
    /// Whether each active player may build each unit type.
    UnitAvailability, UnitAvailabilityRow {
        /// Player id.
        player: u8,
        /// Unit type (BWAPI UnitTypes numbering).
        unit_type: u16,
        /// 1 when the player may build the type.
        available: u8,
    }
);

table!(
    /// Map tiles that do not change during a match, one row per build tile,
    /// row-major (`y * map_width + x`). Read once per match and shared by
    /// every frame.
    Terrain, TerrainRow {
        /// 1 when buildings may be placed on the tile.
        buildable: u8,
        /// 0 low, 1 low doodad, 2 high, 3 high doodad, 4 very high, 5 very high doodad.
        ground_height: u8,
        /// Creep when the match tables were first read: the match start for a
        /// session attached from frame 0, otherwise that moment.
        creep: u8,
        /// Region of the tile (`Regions.id`), or of its first part when the
        /// tile is split.
        region: u16,
        /// Region of the second part of a split tile; equals `region` otherwise.
        region2: u16,
        /// Mini-tiles (4x4, bit `y*4+x`) that belong to `region2`.
        split_mask: u16,
    }
);

table!(
    /// One row per walk tile (8 pixels), row-major over `map_width*4` columns
    /// and `map_height*4` rows. Read once per match.
    Walkability, WalkabilityRow {
        /// 1 when ground units can walk on the walk tile.
        walkable: u8,
    }
);

table!(
    /// Pathing regions; read once per match.
    Regions, RegionRow {
        /// Region id, as `Terrain.region` names it.
        id: u16,
        /// Regions in one group connect by ground.
        group: u16,
        /// Center x in map pixels.
        center_x: i16,
        /// Center y in map pixels.
        center_y: i16,
        /// Left edge of the bounding box, map pixels.
        left: i16,
        /// Top edge of the bounding box, map pixels.
        top: i16,
        /// Right edge of the bounding box, map pixels.
        right: i16,
        /// Bottom edge of the bounding box, map pixels.
        bottom: i16,
        /// 1 when ground units can stand in the region.
        accessible: u8,
        /// 1 for higher ground (BWAPI Region isHigherGround).
        higher_ground: u8,
        /// Defense priority (BWAPI Region defensePriority).
        priority: u8,
        /// First row of this region's neighbors in `RegionNeighbors`.
        neighbor_start: u32,
        /// Number of this region's rows in `RegionNeighbors`.
        neighbor_count: u16,
    }
);

table!(
    /// Neighbor lists of all regions, one neighbor per row;
    /// `Regions.neighbor_start` and `neighbor_count` select a region's rows.
    /// Read once per match.
    RegionNeighbors, RegionNeighborRow {
        /// Neighboring region id.
        region: u16,
    }
);

pub struct TableInfo {
    /// Frame field name.
    pub name: &'static str,
    /// Table type name.
    pub type_name: &'static str,
    /// The table's `///` comment.
    pub doc: &'static str,
    pub columns: &'static [ColumnInfo],
}

/// Lists every table: per-frame tables first, then match-long ones that
/// frames share through an `Arc`.
macro_rules! tables {
    ($($name:ident: $ty:ident,)* ; $($shared:ident: $shared_ty:ident,)*) => {
        /// Every table, in source bit order: table `i` belongs to source `1 << i`.
        pub const TABLES: &[TableInfo] = &[
            $(TableInfo { name: stringify!($name), type_name: stringify!($ty), doc: $ty::DOC, columns: $ty::COLUMNS },)*
            $(TableInfo { name: stringify!($shared), type_name: stringify!($shared_ty), doc: $shared_ty::DOC, columns: $shared_ty::COLUMNS },)*
        ];

        /// Per-frame tables; [`TABLES`] from this index on are match-long.
        pub const FRAME_TABLES: usize = [$(stringify!($name),)*].len();

        /// Tables read once per match.
        #[derive(Clone, Debug, Default)]
        pub struct MatchTables {
            $(pub $shared: std::sync::Arc<$shared_ty>,)*
        }

        impl MatchTables {
            /// Match table of source `1 << index`, unshared first so frames
            /// still holding the old one keep it unchanged.
            #[allow(unused_assignments)]
            pub(crate) fn table_mut(&mut self, index: usize) -> Option<&mut dyn crate::columns::ColumnsMut> {
                let mut at = FRAME_TABLES;
                $(
                    if index == at {
                        if std::sync::Arc::get_mut(&mut self.$shared).is_none() {
                            self.$shared = Default::default();
                        }
                        return std::sync::Arc::get_mut(&mut self.$shared).map(|table| table as _);
                    }
                    at += 1;
                )*
                None
            }
        }

        impl crate::Frame {
            /// Table of source `1 << index` (see [`TABLES`]).
            pub fn table(&self, index: usize) -> Option<&dyn crate::columns::Columns> {
                let tables: &[&dyn crate::columns::Columns] = &[$(&self.$name,)* $(&*self.shared.$shared,)*];
                tables.get(index).copied()
            }

            /// Per-frame table of source `1 << index`.
            #[allow(unused_assignments)]
            pub(crate) fn table_mut(&mut self, index: usize) -> Option<&mut dyn crate::columns::ColumnsMut> {
                let mut at = 0;
                $(
                    if index == at {
                        return Some(&mut self.$name);
                    }
                    at += 1;
                )*
                None
            }

            pub(crate) fn clear_tables(&mut self) {
                $(self.$name.clear();)*
            }
        }
    };
}

table!(
    /// Each live unit's sprite: where it is drawn, in map pixels (it differs
    /// from the unit position while the sprite is offset, for example lifting),
    /// and who sees it.
    Render, RenderRow {
        /// Unit whose sprite this is.
        unit: u64,
        /// Drawn position x in map pixels.
        x: i16,
        /// Drawn position y in map pixels.
        y: i16,
        /// Bit `p` set: the sprite is visible to player `p`.
        visible_to: u16,
        /// Iscript animation the sprite's main image plays (0 init,
        /// 2/3 ground/air attack init, 5/6 ground/air attack repeat, ...).
        animation: u8,
        /// Frame set the main image shows.
        frame_set: u16,
    }
);

tables! {
    players: Players,
    units: Units,
    upgrades: Upgrades,
    techs: Techs,
    unit_counts: UnitCounts,
    bullets: Bullets,
    vision: Vision,
    viewport: Viewport,
    selection: Selection,
    orders: Orders,
    render: Render,
    game: Game,
    forces: Forces,
    start_locations: StartLocations,
    unit_availability: UnitAvailability,
    ;
    terrain: Terrain,
    walkability: Walkability,
    regions: Regions,
    region_neighbors: RegionNeighbors,
}
