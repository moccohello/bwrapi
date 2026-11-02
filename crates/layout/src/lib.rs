//! Single source of truth for the SCR offsets and module RVAs that differ by
//! build: [`Layout::X64`] and [`Layout::X86`]. Readers and the in-game code
//! take a `&Layout`, so both builds share one code path. BWGame table offsets
//! are the same on both and live in the engine's `bwgame.rs`.

/// Width-aware little-endian pointer decode shared by the readers and the
/// runtime collectors: 8 bytes on x64, 4 bytes on x86.
pub fn read_pointer(bytes: &[u8], offset: usize, pointer_bytes: usize) -> Option<usize> {
    let raw = bytes.get(offset..offset.checked_add(pointer_bytes)?)?;
    match pointer_bytes {
        4 => u32::from_le_bytes(raw.try_into().ok()?).try_into().ok(),
        8 => usize::try_from(u64::from_le_bytes(raw.try_into().ok()?)).ok(),
        _ => None,
    }
}

/// CUnit pool geometry and every CUnit record field offset the readers decode.
/// Offsets that several type-gated unions alias (worker/resource, addon,
/// hangar) keep one named field per semantic view.
#[derive(Clone, Copy, Debug)]
pub struct CunitLayout {
    // Pool geometry.
    pub record_stride: usize,
    pub read_window_bytes: usize,
    pub region_bytes: usize,
    pub array_pointer_rva: usize,
    pub capacity_pointer_rva: usize,
    pub native_unit_list_header_module_offset: usize,
    pub native_unit_list_header_bytes: usize,
    pub unit_lists: Option<NativeUnitListsLayout>,
    pub selection_ptr_adjustment: usize,
    // Record fields, in record order.
    pub hit_points_offset: usize,
    pub sprite_ptr_offset: usize,
    pub move_target_position_x_offset: usize,
    pub move_target_position_y_offset: usize,
    pub move_target_unit_ptr_offset: usize,
    pub next_movement_waypoint_x_offset: usize,
    pub next_movement_waypoint_y_offset: usize,
    pub next_target_waypoint_x_offset: usize,
    pub next_target_waypoint_y_offset: usize,
    pub movement_flags_offset: usize,
    pub current_direction_1_offset: usize,
    pub turn_radius_offset: usize,
    pub velocity_direction_offset: usize,
    pub flingy_id_offset: usize,
    pub flingy_movement_type_offset: usize,
    pub position_x_offset: usize,
    pub position_y_offset: usize,
    pub halt_position_x_offset: usize,
    pub halt_position_y_offset: usize,
    pub flingy_top_speed_raw_offset: usize,
    pub flingy_current_speed1_raw_offset: usize,
    pub flingy_current_speed2_raw_offset: usize,
    pub velocity_x_offset: usize,
    pub velocity_y_offset: usize,
    pub flingy_acceleration_offset: usize,
    pub current_direction2_raw_offset: usize,
    pub velocity_direction2_raw_offset: usize,
    pub owner_offset: usize,
    pub order_offset: usize,
    pub order_timer_offset: usize,
    pub ground_weapon_cooldown_offset: usize,
    pub air_weapon_cooldown_offset: usize,
    pub spell_cooldown_offset: usize,
    pub order_target_position_x_offset: usize,
    pub order_target_position_y_offset: usize,
    pub order_target_unit_ptr_offset: usize,
    pub shields_offset: usize,
    pub type_offset: usize,
    pub sub_unit_ptr_offset: usize,
    pub order_queue_head_offset: usize,
    pub order_queue_tail_offset: usize,
    pub connected_unit_ptr_offset: usize,
    pub order_queue_count_offset: usize,
    pub order_queue_timer_offset: usize,
    pub attack_notify_timer_offset: usize,
    pub previous_unit_type_offset: usize,
    pub last_event_timer_offset: usize,
    pub last_event_color_offset: usize,
    pub kill_count_offset: usize,
    pub last_attacking_player_id_offset: usize,
    pub rank_increase_offset: usize,
    pub secondary_order_timer_offset: usize,
    pub ai_action_flags_offset: usize,
    pub user_action_flags_offset: usize,
    pub button_set_id_offset: usize,
    pub raw_is_cloaked_offset: usize,
    pub movement_state_offset: usize,
    pub build_queue_type_offset: usize,
    pub energy_raw_offset: usize,
    pub build_queue_slot_offset: usize,
    pub uniqueness_identifier_offset: usize,
    pub secondary_order_offset: usize,
    pub building_overlay_state_offset: usize,
    pub hit_points_gain_raw_offset: usize,
    pub shield_gain_raw_offset: usize,
    pub build_unit_remaining_time_offset: usize,
    pub previous_hit_points_raw_offset: usize,
    pub worker_resource_carry_count_offset: usize,
    pub production_remaining_time_offset: usize,
    pub research_tech_type_offset: usize,
    pub upgrade_type_offset: usize,
    pub larva_timer_offset: usize,
    pub building_landing_timer_offset: usize,
    pub gas_resource_ptr_offset: usize,
    pub building_upgrade_level_offset: usize,
    pub hangar_in_count_offset: usize,
    pub hangar_out_count_offset: usize,
    pub hangar_in_hangar_offset: usize,
    pub resource_gather_queue_count_offset: usize,
    pub resource_amount_offset: usize,
    pub silo_nuke_ready_offset: usize,
    pub status_flags_offset: usize,
    pub worker_resource_type_offset: usize,
    pub detection_player_flags_offset: usize,
    pub secondary_order_target_position_x_offset: usize,
    pub secondary_order_target_position_y_offset: usize,
    pub build_unit_ptr_offset: usize,
    pub rally_position_x_offset: usize,
    pub rally_position_y_offset: usize,
    pub rally_unit_ptr_offset: usize,
    pub remove_timer_offset: usize,
    pub defense_matrix_damage_offset: usize,
    pub defense_matrix_timer_offset: usize,
    pub stim_timer_offset: usize,
    pub ensnare_timer_offset: usize,
    pub lockdown_timer_offset: usize,
    pub irradiate_timer_offset: usize,
    pub stasis_timer_offset: usize,
    pub plague_timer_offset: usize,
    pub storm_timer_offset: usize,
    // Tail after the timed-effect block.
    pub irradiated_by_ptr_offset: usize,
    pub irradiated_by_ptr_bytes: usize,
    pub irradiate_player_id_offset: usize,
    pub parasite_flags_offset: usize,
    pub is_blind_offset: usize,
    pub maelstrom_timer_offset: usize,
    pub closed_status_end_offset: usize,
    pub acid_spore_count_offset: usize,
    pub pattern_end_offset: usize,
    pub scan_min_bytes: usize,
    // Type-gated union pointers whose slots alias in the record.
    pub worker_power_up_ptr_offset: usize,
    pub vulture_spider_mine_count_offset: usize,
    pub worker_resource_target_ptr_offset: usize,
    pub resource_gatherer_target_ptr_offset: usize,
    pub resource_next_gatherer_ptr_offset: usize,
    pub resource_gatherer_prev_ptr_offset: usize,
    pub resource_group_offset: usize,
    pub resource_gatherer_next_ptr_offset: usize,
    pub building_addon_ptr_offset: usize,
    pub building_addon_build_type_offset: usize,
    pub hangar_in_head_ptr_offset: usize,
    pub hangar_out_head_ptr_offset: usize,
    pub is_being_healed_offset: usize,
}

/// The game's doubly-linked lists of CUnit records: active, hidden, dying,
/// and revealer heads share one header block at
/// `CunitLayout::native_unit_list_header_module_offset`; the invisible and
/// pylon lists have only a first pointer at their own module RVA. List nodes
/// are native Units (`CUnit record + selection_ptr_adjustment`). `None` keeps
/// readers on the whole pool.
#[derive(Clone, Copy, Debug)]
pub struct NativeUnitListsLayout {
    /// First-pointer offsets of active, hidden, dying, revealer inside the
    /// header block.
    pub header_list_first_offsets: [usize; 4],
    /// Last-pointer offsets of the same lists inside the header block.
    pub header_list_last_offsets: [usize; 4],
    pub header_list_prev_link_offset: usize,
    pub header_list_next_link_offset: usize,
    pub invisible_list_first_module_rva: usize,
    pub invisible_list_prev_link_offset: usize,
    pub invisible_list_next_link_offset: usize,
    pub pylon_list_first_module_rva: usize,
    pub pylon_list_prev_link_offset: usize,
    pub pylon_list_next_link_offset: usize,
}

/// COrder pool nodes linked from a CUnit order queue.
#[derive(Clone, Copy, Debug)]
pub struct CorderLayout {
    pub read_bytes: usize,
    pub prev_offset: usize,
    pub next_offset: usize,
    pub order_offset: usize,
    pub unit_type_offset: usize,
    pub target_x_offset: usize,
    pub target_y_offset: usize,
    pub target_unit_ptr_offset: usize,
}

/// CSprite fields read through CUnit->CSprite chains.
#[derive(Clone, Copy, Debug)]
pub struct CspriteLayout {
    pub visibility_flags_offset: usize,
    pub main_image_ptr_offset: usize,
}

/// CImage fields read through CUnit->CSprite->CImage chains.
#[derive(Clone, Copy, Debug)]
pub struct CimageLayout {
    pub animation_offset: usize,
    pub frame_set_offset: usize,
    pub parent_sprite_ptr_offset: usize,
}

/// Module RVAs of the scalar session/player/map-module readers.
#[derive(Clone, Copy, Debug)]
pub struct ScalarsLayout {
    pub session_is_in_game_offset: usize,
    pub session_is_replay_offset: usize,
    pub session_paused_offset: usize,
    pub session_network_in_match_offset: usize,
    pub session_latency_raw_offset: usize,
    pub session_builtin_turn_latency_offset: usize,
    pub session_sync_active_offset: usize,
    pub session_turn_rate_offset: usize,
    pub turn_send_count_offset: usize,
    pub next_engine_step_tick_offset: usize,
    pub game_speed_pointer_offset: usize,
    pub game_speed_primary_delays_offset: usize,
    pub initial_random_seed_offset: usize,
    pub replay_frame_count_offset: usize,
    pub local_minerals_offset: usize,
    pub local_gas_offset: usize,
    pub supply_table_offset: usize,
    pub completed_unit_count_table_offset: usize,
    pub player_identity_table_offset: usize,
    pub current_player_id_cluster_offset: usize,
    pub viewport_map_x_offset: usize,
    pub viewport_map_y_offset: usize,
    pub chat_box_mode_offset: usize,
    pub screen_position_offset: usize,
    pub game_screen_offset: usize,
    pub order_pool_offset: usize,
    pub cheat_flags_offset: usize,
    pub campaign_state_offset: usize,
    pub local_game_result_offset: usize,
    pub pylon_state_offset: usize,
}

/// Bullet pool: a module root block holding the free and active list ends
/// and the pointer to one fixed record array, and the record fields the
/// readers decode. Records link through `prev`/`next` pointers; an active
/// record carries 1 in its u32 marker.
#[derive(Clone, Copy, Debug)]
pub struct BulletLayout {
    pub root_block_module_offset: usize,
    pub root_block_bytes: usize,
    pub record_bytes: usize,
    pub pointer_bytes: usize,
    /// Records in the pool array.
    pub capacity: usize,
    pub root_free_head_offset: usize,
    pub root_free_tail_offset: usize,
    pub root_pool_base_offset: usize,
    pub root_active_head_offset: usize,
    pub root_active_tail_offset: usize,
    pub prev_offset: usize,
    pub next_offset: usize,
    pub active_marker_offset: usize,
    pub sprite_ptr_offset: usize,
    /// u16 bullet type.
    pub type_offset: usize,
    /// i16 pixel position.
    pub position_x_offset: usize,
    pub position_y_offset: usize,
    /// i32 raw velocity and the u8 direction it was derived from.
    pub velocity_x_offset: usize,
    pub velocity_y_offset: usize,
    pub velocity_direction_offset: usize,
    /// u8 player of the source unit.
    pub source_player_offset: usize,
    /// i16 target position and the target unit pointer.
    pub target_position_x_offset: usize,
    pub target_position_y_offset: usize,
    pub target_unit_ptr_offset: usize,
    /// u8 frames left before removal.
    pub remove_timer_offset: usize,
    pub source_unit_ptr_offset: usize,
}

/// Map module pointers and SAI pathing-state internal offsets.
#[derive(Clone, Copy, Debug)]
pub struct MapLayout {
    pub pointer_bytes: usize,
    pub active_tile_pointer_module_offset: usize,
    pub sai_pathing_pointer_module_offset: usize,
    pub sai_pathing_allocation_bytes: usize,
    pub tile_grid: usize,
    pub split_table: usize,
    pub region_table: usize,
    pub region_stride: usize,
    /// Region record fields: the neighbor id list pointer, the 8.8 fixed-point
    /// center (x then y, u32 each), the bounds (left, top, right, bottom u16)
    /// and the properties byte.
    pub region_neighbors_offset: usize,
    pub region_center_offset: usize,
    pub region_bounds_offset: usize,
    pub region_properties_offset: usize,
}

/// Module RVAs of the lobby/network player tables and the race latch.
#[derive(Clone, Copy, Debug)]
pub struct PlayersLayout {
    pub storm_players: usize,
    pub storm_player_bytes: usize,
    pub net_player_to_unique: usize,
    pub net_player_to_game: usize,
    pub net_player_flags: usize,
    pub original_chk_player_types: usize,
    pub lobby_state: usize,
    pub in_lobby_or_game: usize,
    pub storm_races: usize,
    pub participation_table: usize,
    /// The trigger mode: a u32 flag and a pointer, next to each other in
    /// either order (`4 + Layout::pointer_bytes` bytes from here). All zero
    /// while the per-player trigger lists hold the map's triggers.
    pub participation_mode: usize,
}

/// Module RVAs of the selection tables.
#[derive(Clone, Copy, Debug)]
pub struct SelectionLayout {
    pub command_table: usize,
    pub ui_table: usize,
    pub player_selections: usize,
}

/// Module RVAs of the replay header the game type byte is read from.
#[derive(Clone, Copy, Debug)]
pub struct ReplayLayout {
    pub header: usize,
    pub header_bytes: usize,
}

/// The game's replay recorder: the commands it executes, appended to one
/// buffer for the whole match. `None` on a build whose recorder has not been
/// located; the readers then report the recorder-derived columns unknown.
#[derive(Clone, Copy, Debug)]
pub struct RecorderLayout {
    /// Module RVA of the pointer to the recorder object.
    pub object: usize,
    /// Object: u8 the recorder is enabled.
    pub enabled: usize,
    /// Object: pointer to the record buffer; the game moves it when the
    /// buffer outgrows its allocation, so every walk reads it again.
    pub buffer: usize,
    /// Object: u32 bytes of the buffer written.
    pub used: usize,
    /// Object: pointer to the first command byte of the frame the game has
    /// open; that frame's header is the five bytes right before it.
    pub open_frame: usize,
    /// Object: u32 frame number of the last record.
    pub last_frame: usize,
}

/// Byte length of one recorder command: `id` counts itself, and the
/// variable-length kinds count `second` (the byte after the id) entries of
/// their payload. `None` for an id the recorder never writes.
pub fn recorder_command_length(id: u8, second: u8) -> Option<usize> {
    match id {
        0x09 | 0x0a | 0x0b => Some(2 + second as usize * 2),
        0x63 | 0x64 | 0x65 => Some(2 + second as usize * 4),
        0x5c => Some(82),
        0x18 | 0x19 | 0x1b | 0x1c | 0x1d | 0x27 | 0x2a | 0x2e | 0x31 | 0x33 | 0x34 | 0x36 | 0x5a => Some(1),
        0x0f | 0x1a | 0x1e | 0x21 | 0x22 | 0x25 | 0x26 | 0x28 | 0x2b | 0x2c | 0x2d | 0x30 | 0x32 | 0x57 => Some(2),
        0x0d | 0x13 | 0x1f | 0x20 | 0x23 | 0x29 | 0x35 => Some(3),
        0x0e | 0x12 | 0x2f | 0x58 | 0x5d | 0x62 => Some(5),
        0x0c => Some(8),
        0x14 | 0x56 => Some(10),
        0x15 => Some(11),
        0x60 => Some(12),
        0x61 => Some(13),
        _ => None,
    }
}

/// Encoded BWGame root pointer: `read(pointer) ^ xor_key`.
#[derive(Clone, Copy, Debug)]
pub struct MenuRootLayout {
    pub pointer: usize,
    pub xor_key: u64,
}

/// Where the game keeps the cursor position in game screen pixels:
/// `read(read(module + pointer)) + offset` holds x then y as i32.
#[derive(Clone, Copy, Debug)]
pub struct MouseLayout {
    pub pointer: usize,
    pub offset: usize,
}

/// Module RVAs of the game functions the in-game command executor calls.
/// `None` on a build whose functions have not been located; the executor
/// then refuses commands instead of calling an unresolved address. The
/// functions take the platform C calling convention (cdecl on x86) except
/// `send_text`, a member function on its chat context (thiscall on x86).
#[derive(Clone, Copy, Debug)]
pub struct CommandLayout {
    /// `select(count, unit pointers)`: makes the given native Units the
    /// game's current selection.
    pub select_units: Option<usize>,
    /// `enqueue(bytes, len)`: submits one command packet for the current
    /// selection.
    pub enqueue: Option<usize>,
    /// `target_order(x, y, target unit pointer, marker, order, queued)`:
    /// issues a targeted order for the current selection.
    pub target_order_builder: Option<usize>,
    /// `encode(unit selection pointer) -> wire id`: the unit id command
    /// packets carry.
    pub target_encoder: Option<usize>,
    /// `move(x, y)`: moves the local screen to map pixels.
    pub move_screen: Option<usize>,
    /// u32 largest screen x and y `move_screen` may take.
    pub screen_max_x: Option<usize>,
    pub screen_max_y: Option<usize>,
    /// `print(text, player, 0)`: shows a line in the game's message area;
    /// player 16 shows it from no one.
    pub print_text: Option<usize>,
    /// `send(chat context, recipient string, text string)`: sends a chat
    /// line as the local player.
    pub send_text: Option<usize>,
    /// The chat context `send_text` takes: `read(read(pointer) ^ xor_key +
    /// offset)`.
    pub chat_context: Option<ChatContextLayout>,
    /// `end_game(code)`: stores the end code, stops the game loop and runs the
    /// game's end of game (replay frame count included).
    pub end_game: Option<usize>,
    /// `recompute(delays)`: installs seven per-speed frame delays (ms) into
    /// `ScalarsLayout::game_speed_primary_delays_offset` and derives the turn
    /// durations from them.
    pub recompute_turn_durations: Option<usize>,
    /// Seven u32 delays the game keeps beside the ones `recompute` sets.
    pub alt_speed_modifiers: Option<usize>,
    /// i32 count of logical frames one screen update steps.
    pub step_game_frames: Option<usize>,
    /// u8 player-slot mask of the vision a replay shows, and the local
    /// vision mask the game draws from; a replay vision change sets both.
    pub replay_vision: Option<usize>,
    pub local_vision: Option<usize>,
    /// u32 a replay reveals the whole map while it is 1.
    pub replay_reveal_all: Option<usize>,
    /// The game loop's screen update cap: while the u8 is 1, an update is
    /// skipped until the u32 milliseconds have passed since the last one.
    /// The game installs both from its settings when a game loop starts.
    pub draw_limit_enabled: Option<usize>,
    pub draw_limit_interval: Option<usize>,
}

/// Where the chat context `CommandLayout::send_text` takes lives.
#[derive(Clone, Copy, Debug)]
pub struct ChatContextLayout {
    pub pointer: usize,
    pub xor_key: u64,
    pub offset: usize,
}

/// Everything that differs between the SCR x64 and x86 builds. `module_size`
/// is the mapped image size [`Layout::detect`] tells the builds apart by.
#[derive(Clone, Copy, Debug)]
pub struct Layout {
    pub pointer_bytes: usize,
    pub module_size: u32,
    pub cunit: CunitLayout,
    pub corder: CorderLayout,
    pub csprite: CspriteLayout,
    pub cimage: CimageLayout,
    pub scalars: ScalarsLayout,
    pub bullet: BulletLayout,
    pub map: MapLayout,
    pub players: PlayersLayout,
    pub selection: SelectionLayout,
    pub replay: ReplayLayout,
    pub recorder: Option<RecorderLayout>,
    pub menu_root: MenuRootLayout,
    pub commands: CommandLayout,
    pub mouse: Option<MouseLayout>,
    pub chat: Option<ChatLayout>,
    /// u32 id of the network provider of the running game: `BNET` and `BNAU`
    /// Battle.net, `UDPA` LAN, 0 single player.
    pub network_provider: Option<usize>,
    pub save: Option<SaveLayout>,
    pub fonts: Option<FontLayout>,
    pub render: Option<RenderLayout>,
    pub resource_writes: Option<ResourceWriteLayout>,
}

/// Module RVAs of the instructions that change a player's minerals (BWGame
/// root + 4 * slot) or gas (+ 0x30) for the events BWAPI counts: paying a
/// repair step, and giving back what a cancel returns. Each instruction
/// changes one array entry, so the value before and after it is the amount.
#[derive(Clone, Copy, Debug)]
pub struct ResourceWriteLayout {
    pub repairs: &'static [usize],
    pub refunds: &'static [usize],
}

/// The game's text fonts and the glyphs it renders them from. `table` is a
/// module RVA of `count` pointers to font objects, in BWAPI's Text::Size
/// order. A font object points to its glyph block at `glyphs` (a `FONT`
/// header whose byte `height_byte` is the line height in game pixels) and to
/// its glyph set at `glyph_set`; it keeps a u8 outline flag at `outline` and
/// the f32 outline width right after it. The game's text commands carry an
/// outlined font's width twice and `outline_edge` as shader constants.
///
/// A glyph set caches the glyphs it rendered in a binary tree whose root
/// pointer is at `glyph_tree`; a node links its left and right children at
/// `node_left` and `node_right` and holds the u32 character at `node_key` and
/// the glyph record at `node_glyph`. `cache_glyph(glyph set, character)`
/// adds a missing character to the tree. A glyph record holds the u32 atlas
/// page at `glyph_page`, then f32 pairs: the bitmap's top-left offset from the
/// pen in game pixels at `glyph_bearing`, its size in atlas pixels at
/// `glyph_size`, its atlas position at `glyph_atlas`, and the f32 advance in
/// game pixels at `glyph_advance`. Atlas pixels are half a game pixel.
///
/// `pages` is a module RVA of the atlas page list and `page_texture(pages,
/// page)` uploads what changed on a page and returns its texture holder,
/// which keeps the texture pointer at `holder_texture`. Pages are `atlas_size`
/// pixels square. `cache_glyph` and `page_texture` are member functions of
/// their first argument (thiscall on x86).
#[derive(Clone, Copy, Debug)]
pub struct FontLayout {
    pub table: usize,
    pub count: usize,
    pub glyphs: usize,
    pub height_byte: usize,
    pub glyph_set: usize,
    pub outline: usize,
    pub outline_edge: f32,
    pub glyph_tree: usize,
    pub node_left: usize,
    pub node_right: usize,
    pub node_key: usize,
    pub node_glyph: usize,
    pub cache_glyph: usize,
    pub glyph_page: usize,
    pub glyph_bearing: usize,
    pub glyph_size: usize,
    pub glyph_atlas: usize,
    pub glyph_advance: usize,
    pub pages: usize,
    pub page_texture: usize,
    pub holder_texture: usize,
    pub atlas_size: u32,
}

/// Where the game queues what it draws each screen update and the renderer
/// that draws the queue.
///
/// `renderer` is a module RVA of the pointer to the renderer object, whose
/// vtable has `vtable_entries` entries after the type information pointer.
/// Entry `render_entry` is `render(renderer, queue, width, height)`, which
/// sorts the queue, uploads the vertices and draws; entry
/// `create_texture_entry` is `create(renderer, format, pixels, bytes, width,
/// height, 0, 0) -> texture`, format 0 RGBA. Both are member functions of
/// the renderer (thiscall on x86).
///
/// The queue `render` takes holds `queue_capacity` commands of
/// `command_bytes`, the sort list `{data, length, capacity}` at `queue_sort`
/// whose entries of `sort_entry_bytes` are `{u16 layer, u16 order, u32,
/// command pointer}`, and the u16 command count at `queue_count`. The game
/// draws by sort layer, then order; `layer` lies above the game screen and
/// its console and below the cursor.
///
/// `vertices` is a module RVA of the vertex buffer: capacity in u32 units,
/// used bytes and the data pointer, the bytes skipped to align vertices at
/// `vertex_padding`, and the index buffer (capacity in u16 units, used bytes,
/// data pointer) at `index_buffer`. A command's vertices start at a multiple
/// of their size; its u16 indices count from its first vertex.
///
/// Draw mode `triangle_mode` draws indexed triangles of vertices `{f32 x, y,
/// u, v, u32 RGBA}` in 0..1 of the game screen, y up; `color_shader`
/// multiplies the texture by the vertex color and `text_shader` draws the
/// game's glyph atlas in the vertex color, its second texture
/// `blank_texture` (a module RVA of the texture pointer).
///
/// The game screen is `screen_height` game pixels high and as wide as the
/// u32 at `ScalarsLayout::game_screen_offset`.
#[derive(Clone, Copy, Debug)]
pub struct RenderLayout {
    pub renderer: usize,
    pub vtable_entries: usize,
    pub render_entry: usize,
    pub create_texture_entry: usize,
    pub queue_capacity: usize,
    pub command_bytes: usize,
    pub queue_sort: usize,
    pub sort_entry_bytes: usize,
    pub queue_count: usize,
    pub layer: u16,
    pub vertices: usize,
    pub vertex_padding: usize,
    pub index_buffer: usize,
    pub command: DrawCommandLayout,
    pub triangle_mode: u32,
    pub color_shader: u32,
    pub text_shader: u32,
    pub blank_texture: usize,
    pub screen_height: u32,
}

/// One draw command of the queue: `render_target` (u32, 1 is the screen),
/// `is_hd` (u32), seven texture pointers at `textures`, the u32 draw mode and
/// shader, the vertex and index offsets in bytes (pointer-wide), the u32
/// allocated and used vertex counts, the u16 `order_mark` the game writes
/// 0xffff, and twenty f32 shader constants at `constants`: the color factor
/// at 0..4, then 0, 0, 0, 1 and 1, 1, 1, 1, the reciprocal render target or
/// texture size at 14 and 15 and the glyph outline at 16..19. Everything else
/// is zero.
#[derive(Clone, Copy, Debug)]
pub struct DrawCommandLayout {
    pub render_target: usize,
    pub is_hd: usize,
    pub textures: usize,
    pub draw_mode: usize,
    pub shader: usize,
    pub vertex_offset: usize,
    pub index_offset: usize,
    pub vertex_count: usize,
    pub used_count: usize,
    pub order_mark: usize,
    pub constants: usize,
}

/// The local player's last save from the save game dialog.
#[derive(Clone, Copy, Debug)]
pub struct SaveLayout {
    /// NUL-terminated save name of `name_bytes`; the dialog offers it next.
    pub name: usize,
    pub name_bytes: usize,
    /// u32 tick count (ms) of the save; 0 before the first.
    pub tick: usize,
}

/// The game's chat box.
#[derive(Clone, Copy, Debug)]
pub struct ChatLayout {
    /// Pointer to the chat box dialog the match loads; null outside a match.
    /// `ScalarsLayout::chat_box_mode_offset` is non-zero while it is open.
    pub dialog: usize,
    /// Dialog: pointer to its first control.
    pub dialog_first_control: usize,
    /// Control: pointer to the next control, u16 id, then the text as a data
    /// pointer and a pointer-wide byte length.
    pub control_next: usize,
    pub control_id: usize,
    pub control_text: usize,
    pub control_text_length: usize,
    /// Id of the chat box's text box.
    pub text_box_id: u16,
    /// `close(dialog, event, send)`: closes the open chat box; with `send` 0
    /// it drops the typed line, as Escape does.
    pub close: usize,
    /// Dialog: offset of the event handler the game calls with
    /// `(dialog, event)` for every event the dialog takes. The handler and
    /// `close` are cdecl on x86.
    pub dialog_event: usize,
    /// Event: u16 key or character at `event_key`, u16 kind at
    /// `event_kind`; `event_kind_char` is the kind of a character input.
    pub event_key: usize,
    pub event_kind: usize,
    pub event_kind_char: u16,
}

impl Layout {
    /// The layout of an attached SCR module by its mapped image size; `None`
    /// for a build this crate has no layout for.
    pub fn detect(module_size: usize) -> Option<&'static Layout> {
        [&Self::X64, &Self::X86].into_iter().find(|layout| layout.module_size as usize == module_size)
    }

    /// SCR x64 build.
    pub const X64: Self = Self {
        pointer_bytes: 8,
        module_size: 0x145_0000,
        cunit: CunitLayout {
            record_stride: 0x1e8,
            read_window_bytes: 0x3d0,
            region_bytes: 0x1a_0000,
            array_pointer_rva: 0xe7_8060,
            capacity_pointer_rva: 0xe7_8068,
            native_unit_list_header_module_offset: 0xe7_7fc8,
            native_unit_list_header_bytes: 0xd0,
            // Links are record-relative: native Unit+0x00/+0x08 and the
            // invisible/pylon unions at Unit+0x160/+0x170.
            unit_lists: Some(NativeUnitListsLayout {
                header_list_first_offsets: [0x00, 0x20, 0xb8, 0x28],
                header_list_last_offsets: [0xb0, 0xc0, 0x08, 0xc8],
                header_list_prev_link_offset: 0x10,
                header_list_next_link_offset: 0x18,
                invisible_list_first_module_rva: 0xe7_8450,
                invisible_list_prev_link_offset: 0x170,
                invisible_list_next_link_offset: 0x178,
                pylon_list_first_module_rva: 0xe7_80c0,
                pylon_list_prev_link_offset: 0x180,
                pylon_list_next_link_offset: 0x188,
            }),
            selection_ptr_adjustment: 0x10,
            hit_points_offset: 0x20,
            sprite_ptr_offset: 0x28,
            move_target_position_x_offset: 0x30,
            move_target_position_y_offset: 0x32,
            move_target_unit_ptr_offset: 0x38,
            next_movement_waypoint_x_offset: 0x40,
            next_movement_waypoint_y_offset: 0x42,
            next_target_waypoint_x_offset: 0x44,
            next_target_waypoint_y_offset: 0x46,
            movement_flags_offset: 0x48,
            current_direction_1_offset: 0x49,
            turn_radius_offset: 0x4a,
            velocity_direction_offset: 0x4b,
            flingy_id_offset: 0x4c,
            flingy_movement_type_offset: 0x4f,
            position_x_offset: 0x50,
            position_y_offset: 0x52,
            halt_position_x_offset: 0x54,
            halt_position_y_offset: 0x58,
            flingy_top_speed_raw_offset: 0x5c,
            flingy_current_speed1_raw_offset: 0x60,
            flingy_current_speed2_raw_offset: 0x64,
            velocity_x_offset: 0x68,
            velocity_y_offset: 0x6c,
            flingy_acceleration_offset: 0x70,
            current_direction2_raw_offset: 0x72,
            velocity_direction2_raw_offset: 0x73,
            owner_offset: 0x78,
            order_offset: 0x79,
            order_timer_offset: 0x80,
            ground_weapon_cooldown_offset: 0x81,
            air_weapon_cooldown_offset: 0x82,
            spell_cooldown_offset: 0x83,
            order_target_position_x_offset: 0x88,
            order_target_position_y_offset: 0x8a,
            order_target_unit_ptr_offset: 0x90,
            shields_offset: 0x98,
            type_offset: 0x9c,
            sub_unit_ptr_offset: 0xb0,
            order_queue_head_offset: 0xb8,
            order_queue_tail_offset: 0xc0,
            connected_unit_ptr_offset: 0xd0,
            order_queue_count_offset: 0xd8,
            order_queue_timer_offset: 0xd9,
            attack_notify_timer_offset: 0xdb,
            previous_unit_type_offset: 0xdc,
            last_event_timer_offset: 0xde,
            last_event_color_offset: 0xdf,
            kill_count_offset: 0xe3,
            last_attacking_player_id_offset: 0xe4,
            rank_increase_offset: 0xe2,
            secondary_order_timer_offset: 0xe5,
            ai_action_flags_offset: 0xe6,
            user_action_flags_offset: 0xe7,
            button_set_id_offset: 0xe8,
            raw_is_cloaked_offset: 0xea,
            movement_state_offset: 0xeb,
            build_queue_type_offset: 0xec,
            energy_raw_offset: 0xf6,
            build_queue_slot_offset: 0xf8,
            uniqueness_identifier_offset: 0xf9,
            secondary_order_offset: 0xfa,
            building_overlay_state_offset: 0xfb,
            hit_points_gain_raw_offset: 0xfc,
            shield_gain_raw_offset: 0xfe,
            build_unit_remaining_time_offset: 0x100,
            previous_hit_points_raw_offset: 0x102,
            worker_resource_carry_count_offset: 0x133,
            production_remaining_time_offset: 0x122,
            research_tech_type_offset: 0x124,
            upgrade_type_offset: 0x125,
            larva_timer_offset: 0x126,
            building_landing_timer_offset: 0x127,
            gas_resource_ptr_offset: 0x128,
            building_upgrade_level_offset: 0x129,
            hangar_in_count_offset: 0x128,
            hangar_out_count_offset: 0x129,
            hangar_in_hangar_offset: 0x130,
            resource_gather_queue_count_offset: 0x13b,
            resource_amount_offset: 0x138,
            silo_nuke_ready_offset: 0x140,
            status_flags_offset: 0x150,
            worker_resource_type_offset: 0x154,
            detection_player_flags_offset: 0x158,
            secondary_order_target_position_x_offset: 0x160,
            secondary_order_target_position_y_offset: 0x162,
            build_unit_ptr_offset: 0x168,
            rally_position_x_offset: 0x180,
            rally_position_y_offset: 0x182,
            rally_unit_ptr_offset: 0x188,
            remove_timer_offset: 0x1ac,
            defense_matrix_damage_offset: 0x1ae,
            defense_matrix_timer_offset: 0x1b0,
            stim_timer_offset: 0x1b1,
            ensnare_timer_offset: 0x1b2,
            lockdown_timer_offset: 0x1b3,
            irradiate_timer_offset: 0x1b4,
            stasis_timer_offset: 0x1b5,
            plague_timer_offset: 0x1b6,
            storm_timer_offset: 0x1b7,
            // The tail after the x64 irradiatedBy pointer: 0x1b8 + 8 + k.
            irradiated_by_ptr_offset: 0x1b8,
            irradiated_by_ptr_bytes: 8,
            irradiate_player_id_offset: 0x1c0,
            parasite_flags_offset: 0x1c1,
            is_blind_offset: 0x1c3,
            maelstrom_timer_offset: 0x1c4,
            closed_status_end_offset: 0x1c5,
            acid_spore_count_offset: 0x1c6,
            pattern_end_offset: 0x1c7,
            scan_min_bytes: 0x1c7,
            worker_power_up_ptr_offset: 0x118,
            vulture_spider_mine_count_offset: 0x118,
            worker_resource_target_ptr_offset: 0x128,
            resource_gatherer_target_ptr_offset: 0x138,
            resource_next_gatherer_ptr_offset: 0x140,
            resource_gatherer_prev_ptr_offset: 0x140,
            resource_group_offset: 0x148,
            resource_gatherer_next_ptr_offset: 0x148,
            building_addon_ptr_offset: 0x118,
            building_addon_build_type_offset: 0x120,
            hangar_in_head_ptr_offset: 0x118,
            hangar_out_head_ptr_offset: 0x120,
            is_being_healed_offset: 0x1a3,
        },
        corder: CorderLayout {
            read_bytes: 0x28,
            prev_offset: 0x00,
            next_offset: 0x08,
            order_offset: 0x10,
            unit_type_offset: 0x12,
            target_x_offset: 0x18,
            target_y_offset: 0x1a,
            target_unit_ptr_offset: 0x20,
        },
        csprite: CspriteLayout {
            visibility_flags_offset: 0x14,
            main_image_ptr_offset: 0x30,
        },
        cimage: CimageLayout {
            animation_offset: 0x1e,
            frame_set_offset: 0x20,
            parent_sprite_ptr_offset: 0x50,
        },
        scalars: ScalarsLayout {
            session_is_in_game_offset: 0xDAF178,
            session_is_replay_offset: 0xDAB1D8,
            session_paused_offset: 0xE7FA7C,
            session_network_in_match_offset: 0x10908DC,
            session_latency_raw_offset: 0x10924A8,
            session_builtin_turn_latency_offset: 0x10908D8,
            session_sync_active_offset: 0x10924A4,
            session_turn_rate_offset: 0x1091FCC,
            turn_send_count_offset: 0xDDF340,
            next_engine_step_tick_offset: 0x1067D08,
            game_speed_pointer_offset: 0x10C53A8,
            game_speed_primary_delays_offset: 0xDCF050,
            initial_random_seed_offset: 0x1095878,
            replay_frame_count_offset: 0x1095871,
            local_minerals_offset: 0x10A4388,
            local_gas_offset: 0x10A438C,
            supply_table_offset: 0xECE180,
            completed_unit_count_table_offset: 0xECE360,
            player_identity_table_offset: 0x1095911,
            current_player_id_cluster_offset: 0xDD5B50,
            viewport_map_x_offset: 0xED3BB8,
            viewport_map_y_offset: 0xED3BBC,
            chat_box_mode_offset: 0x10A4AB0,
            screen_position_offset: 0xED3BC0,
            game_screen_offset: 0xDADDC0,
            order_pool_offset: 0xE76920,
            cheat_flags_offset: 0x1053E30,
            campaign_state_offset: 0x10B0D08,
            local_game_result_offset: 0x106A7A8,
            pylon_state_offset: 0xE780D8,
        },
        bullet: BulletLayout {
            root_block_module_offset: 0xe5f660,
            root_block_bytes: 0x50,
            record_bytes: 0xa8,
            pointer_bytes: 8,
            capacity: 400,
            root_free_head_offset: 0x00,
            root_free_tail_offset: 0x08,
            root_pool_base_offset: 0x28,
            root_active_head_offset: 0x40,
            root_active_tail_offset: 0x48,
            prev_offset: 0x00,
            next_offset: 0x08,
            active_marker_offset: 0x10,
            sprite_ptr_offset: 0x18,
            type_offset: 0x3c,
            position_x_offset: 0x40,
            position_y_offset: 0x42,
            velocity_x_offset: 0x58,
            velocity_y_offset: 0x5c,
            velocity_direction_offset: 0x62,
            source_player_offset: 0x68,
            target_position_x_offset: 0x78,
            target_position_y_offset: 0x7a,
            target_unit_ptr_offset: 0x80,
            remove_timer_offset: 0x89,
            source_unit_ptr_offset: 0x90,
        },
        map: MapLayout {
            pointer_bytes: 8,
            active_tile_pointer_module_offset: 0xe7e460,
            sai_pathing_pointer_module_offset: 0x1097780,
            sai_pathing_allocation_bytes: 0xa1670,
            tile_grid: 0x18,
            split_table: 0x20018,
            region_table: 0x44a08,
            region_stride: 0x48,
            region_neighbors_offset: 0x10,
            region_center_offset: 0x18,
            region_bounds_offset: 0x20,
            region_properties_offset: 0x28,
        },
        players: PlayersLayout {
            storm_players: 0x10931b0,
            storm_player_bytes: 0x68,
            net_player_to_unique: 0x1090878,
            net_player_to_game: 0x10908a8,
            net_player_flags: 0x10908e8,
            original_chk_player_types: 0x10907d8,
            lobby_state: 0x1090690,
            in_lobby_or_game: 0x10920c0,
            storm_races: 0x1094300,
            participation_table: 0x10b0d40,
            participation_mode: 0x105423c,
        },
        selection: SelectionLayout {
            command_table: 0x10907f0,
            ui_table: 0x10a1e20,
            player_selections: 0x10a1b20,
        },
        replay: ReplayLayout {
            header: 0x1095870,
            header_bytes: 0x279,
        },
        recorder: Some(RecorderLayout {
            object: 0x1093ab0,
            enabled: 0x00,
            buffer: 0x08,
            used: 0x10,
            open_frame: 0x18,
            last_frame: 0x20,
        }),
        menu_root: MenuRootLayout {
            pointer: 0xdb00d8,
            xor_key: 0x3703_db00_831b_dfe2,
        },
        commands: CommandLayout {
            select_units: Some(0x37e930),
            enqueue: Some(0x3921a0),
            target_order_builder: Some(0x37a850),
            target_encoder: Some(0x1c6410),
            move_screen: Some(0x2ad310),
            screen_max_x: Some(0xed3b74),
            screen_max_y: Some(0xed3b78),
            print_text: Some(0x35d5c0),
            send_text: Some(0x4b4160),
            chat_context: Some(ChatContextLayout { pointer: 0xde0088, xor_key: 0x4564d796a638067d, offset: 0xb0 }),
            end_game: Some(0x3072f0),
            recompute_turn_durations: Some(0x31c6b0),
            alt_speed_modifiers: Some(0xdcf098),
            step_game_frames: Some(0xdcf048),
            replay_vision: Some(0x109585c),
            local_vision: Some(0x10908e0),
            replay_reveal_all: Some(0x1095860),
            draw_limit_enabled: Some(0x1067d20),
            draw_limit_interval: Some(0x1067d28),
        },
        mouse: Some(MouseLayout { pointer: 0xE7E4A8, offset: 0x140 }),
        chat: Some(ChatLayout {
            dialog: 0x10a4ab8,
            dialog_first_control: 0x90,
            control_next: 0x00,
            control_id: 0x52,
            control_text: 0x20,
            control_text_length: 0x28,
            text_box_id: 7,
            close: 0x40c3d0,
            dialog_event: 0x60,
            event_key: 0x10,
            event_kind: 0x18,
            event_kind_char: 0x0f,
        }),
        network_provider: Some(0x10920c4),
        save: Some(SaveLayout { name: 0x10a1898, name_bytes: 0x1c, tick: 0x10a18b4 }),
        fonts: Some(FontLayout {
            table: 0x10c4518,
            count: 4,
            glyphs: 0,
            height_byte: 11,
            glyph_set: 0x20,
            outline: 0x28,
            outline_edge: 0.75,
            glyph_tree: 0x3e8,
            node_left: 0x08,
            node_right: 0x10,
            node_key: 0x20,
            node_glyph: 0x24,
            cache_glyph: 0x621dc0,
            glyph_page: 0x04,
            glyph_bearing: 0x08,
            glyph_size: 0x10,
            glyph_atlas: 0x18,
            glyph_advance: 0x20,
            pages: 0x10c6040,
            page_texture: 0x6287c0,
            holder_texture: 0x08,
            atlas_size: 1024,
        }),
        render: Some(RenderLayout {
            renderer: 0x12cbaf0,
            vtable_entries: 28,
            render_entry: 7,
            create_texture_entry: 12,
            queue_capacity: 0x2000,
            command_bytes: 0xd8,
            queue_sort: 0x1d0008,
            sort_entry_bytes: 16,
            queue_count: 0x1d0050,
            layer: 28,
            vertices: 0xdef8d0,
            vertex_padding: 0x48,
            index_buffer: 0x60,
            command: DrawCommandLayout {
                render_target: 0x00,
                is_hd: 0x04,
                textures: 0x08,
                draw_mode: 0x40,
                shader: 0x44,
                vertex_offset: 0x48,
                index_offset: 0x50,
                vertex_count: 0x58,
                used_count: 0x5c,
                order_mark: 0x60,
                constants: 0x88,
            },
            triangle_mode: 1,
            color_shader: 4,
            text_shader: 6,
            blank_texture: 0x10fb948,
            screen_height: 480,
        }),
        resource_writes: Some(ResourceWriteLayout {
            // A repair step's payment.
            repairs: &[0x1a6d2d, 0x1a6d3e],
            refunds: &[
                // A cancelled unit, queue slot, egg or cocoon: its full cost.
                0x2a7a13, 0x2a7a2c,
                // A cancelled building or Zerg building morph: 3/4.
                0x2a7a6f, 0x2a7a94,
                // A cancelled research and a cancelled upgrade.
                0x1e7ea9, 0x1e7ec2, 0x1e7fbe, 0x1e7fec,
            ],
        }),
    };

    /// SCR x86 (WOW64) build. Its CUnit records have no selection-table header
    /// in front of them, so record offsets equal CUnit-relative offsets
    /// (`selection_ptr_adjustment` is 0).
    pub const X86: Self = Self {
        pointer_bytes: 4,
        // SizeOfImage of the mapped (decrypted) module image.
        module_size: 0x10CE000,
        cunit: CunitLayout {
            record_stride: 0x150,
            read_window_bytes: 0x2a0,
            // 3400 records of `record_stride` bytes.
            region_bytes: 0x116e80,
            array_pointer_rva: 0xba36f0,
            capacity_pointer_rva: 0xba36f4,
            native_unit_list_header_module_offset: 0xba36a4,
            native_unit_list_header_bytes: 0x68,
            // Links are record-relative: the record's own prev/next pair at
            // +0x00/+0x04 and the invisible/pylon unions at +0xf0/+0xf8.
            unit_lists: Some(NativeUnitListsLayout {
                header_list_first_offsets: [0x00, 0x10, 0x5c, 0x14],
                header_list_last_offsets: [0x58, 0x60, 0x04, 0x64],
                header_list_prev_link_offset: 0x00,
                header_list_next_link_offset: 0x04,
                invisible_list_first_module_rva: 0xba_3918,
                invisible_list_prev_link_offset: 0xf0,
                invisible_list_next_link_offset: 0xf4,
                pylon_list_first_module_rva: 0xba_3724,
                pylon_list_prev_link_offset: 0xf8,
                pylon_list_next_link_offset: 0xfc,
            }),
            selection_ptr_adjustment: 0x0,
            hit_points_offset: 0x8,
            sprite_ptr_offset: 0xc,
            move_target_position_x_offset: 0x10,
            move_target_position_y_offset: 0x12,
            move_target_unit_ptr_offset: 0x14,
            next_movement_waypoint_x_offset: 0x18,
            next_movement_waypoint_y_offset: 0x1a,
            next_target_waypoint_x_offset: 0x1c,
            next_target_waypoint_y_offset: 0x1e,
            movement_flags_offset: 0x20,
            current_direction_1_offset: 0x21,
            turn_radius_offset: 0x22,
            velocity_direction_offset: 0x23,
            flingy_id_offset: 0x24,
            flingy_movement_type_offset: 0x27,
            position_x_offset: 0x28,
            position_y_offset: 0x2a,
            halt_position_x_offset: 0x2c,
            halt_position_y_offset: 0x30,
            flingy_top_speed_raw_offset: 0x34,
            flingy_current_speed1_raw_offset: 0x38,
            flingy_current_speed2_raw_offset: 0x3c,
            velocity_x_offset: 0x40,
            velocity_y_offset: 0x44,
            flingy_acceleration_offset: 0x48,
            current_direction2_raw_offset: 0x4a,
            velocity_direction2_raw_offset: 0x4b,
            owner_offset: 0x4c,
            order_offset: 0x4d,
            order_timer_offset: 0x54,
            ground_weapon_cooldown_offset: 0x55,
            air_weapon_cooldown_offset: 0x56,
            spell_cooldown_offset: 0x57,
            order_target_position_x_offset: 0x58,
            order_target_position_y_offset: 0x5a,
            order_target_unit_ptr_offset: 0x5c,
            shields_offset: 0x60,
            type_offset: 0x64,
            sub_unit_ptr_offset: 0x70,
            order_queue_head_offset: 0x74,
            order_queue_tail_offset: 0x78,
            connected_unit_ptr_offset: 0x80,
            order_queue_count_offset: 0x84,
            order_queue_timer_offset: 0x85,
            attack_notify_timer_offset: 0x87,
            previous_unit_type_offset: 0x88,
            last_event_timer_offset: 0x8a,
            last_event_color_offset: 0x8b,
            kill_count_offset: 0x8f,
            last_attacking_player_id_offset: 0x90,
            rank_increase_offset: 0x8e,
            secondary_order_timer_offset: 0x91,
            ai_action_flags_offset: 0x92,
            user_action_flags_offset: 0x93,
            button_set_id_offset: 0x94,
            raw_is_cloaked_offset: 0x96,
            movement_state_offset: 0x97,
            build_queue_type_offset: 0x98,
            energy_raw_offset: 0xa2,
            build_queue_slot_offset: 0xa4,
            uniqueness_identifier_offset: 0xa5,
            secondary_order_offset: 0xa6,
            building_overlay_state_offset: 0xa7,
            hit_points_gain_raw_offset: 0xa8,
            shield_gain_raw_offset: 0xaa,
            build_unit_remaining_time_offset: 0xac,
            previous_hit_points_raw_offset: 0xae,
            worker_resource_carry_count_offset: 0xaf,
            production_remaining_time_offset: 0xc6,
            research_tech_type_offset: 0xc8,
            upgrade_type_offset: 0xc9,
            larva_timer_offset: 0xca,
            building_landing_timer_offset: 0xcb,
            gas_resource_ptr_offset: 0xcc,
            building_upgrade_level_offset: 0xcd,
            hangar_in_count_offset: 0xcc,
            hangar_out_count_offset: 0xcd,
            hangar_in_hangar_offset: 0xce,
            resource_gather_queue_count_offset: 0xd3,
            resource_amount_offset: 0xd0,
            silo_nuke_ready_offset: 0xd4,
            status_flags_offset: 0xdc,
            worker_resource_type_offset: 0xe0,
            detection_player_flags_offset: 0xe4,
            secondary_order_target_position_x_offset: 0xe8,
            secondary_order_target_position_y_offset: 0xea,
            build_unit_ptr_offset: 0xec,
            // The rally point shares the pylon list links, as on x64.
            rally_position_x_offset: 0xf8,
            rally_position_y_offset: 0xfa,
            rally_unit_ptr_offset: 0xfc,
            remove_timer_offset: 0x110,
            defense_matrix_damage_offset: 0x112,
            defense_matrix_timer_offset: 0x114,
            stim_timer_offset: 0x115,
            ensnare_timer_offset: 0x116,
            lockdown_timer_offset: 0x117,
            irradiate_timer_offset: 0x118,
            stasis_timer_offset: 0x119,
            plague_timer_offset: 0x11a,
            storm_timer_offset: 0x11b,
            // The tail after the 4-byte x86 irradiatedBy pointer: 0x11c + 4 + k.
            irradiated_by_ptr_offset: 0x11c,
            irradiated_by_ptr_bytes: 4,
            irradiate_player_id_offset: 0x120,
            parasite_flags_offset: 0x121,
            is_blind_offset: 0x123,
            maelstrom_timer_offset: 0x124,
            closed_status_end_offset: 0x125,
            acid_spore_count_offset: 0x126,
            pattern_end_offset: 0x127,
            scan_min_bytes: 0x127,
            worker_power_up_ptr_offset: 0xc0,
            vulture_spider_mine_count_offset: 0xc0,
            worker_resource_target_ptr_offset: 0xcc,
            resource_gatherer_target_ptr_offset: 0xd0,
            resource_next_gatherer_ptr_offset: 0xd4,
            resource_gatherer_prev_ptr_offset: 0xd4,
            resource_group_offset: 0xd8,
            resource_gatherer_next_ptr_offset: 0xd8,
            building_addon_ptr_offset: 0xc0,
            building_addon_build_type_offset: 0xc4,
            hangar_in_head_ptr_offset: 0xc0,
            hangar_out_head_ptr_offset: 0xc4,
            is_being_healed_offset: 0x107,
        },
        corder: CorderLayout {
            read_bytes: 0x18,
            prev_offset: 0x0,
            next_offset: 0x4,
            order_offset: 0x8,
            unit_type_offset: 0xa,
            target_x_offset: 0x10,
            target_y_offset: 0x12,
            target_unit_ptr_offset: 0x14,
        },
        csprite: CspriteLayout {
            visibility_flags_offset: 0xc,
            main_image_ptr_offset: 0x1c,
        },
        cimage: CimageLayout {
            animation_offset: 0x16,
            frame_set_offset: 0x18,
            parent_sprite_ptr_offset: 0x3c,
        },
        scalars: ScalarsLayout {
            session_is_in_game_offset: 0xB05860,
            session_is_replay_offset: 0xDA4580,
            session_paused_offset: 0xBAA1CC,
            session_network_in_match_offset: 0xD9F7BC,
            session_latency_raw_offset: 0xDA1288,
            session_builtin_turn_latency_offset: 0xD9F7B8,
            session_sync_active_offset: 0xDA1284,
            session_turn_rate_offset: 0xDA0E84,
            turn_send_count_offset: 0xB30960,
            next_engine_step_tick_offset: 0xD77588,
            game_speed_pointer_offset: 0xDCC3E8,
            game_speed_primary_delays_offset: 0xB24D7C,
            // Replay-header cluster at 0xDA4588: seed at +8, frame count at +1.
            initial_random_seed_offset: 0xDA4590,
            replay_frame_count_offset: 0xDA4589,
            local_minerals_offset: 0xDB1ED4,
            local_gas_offset: 0xDB1ED8,
            supply_table_offset: 0xBE0170,
            completed_unit_count_table_offset: 0xBE0350,
            player_identity_table_offset: 0xDA4629,
            current_player_id_cluster_offset: 0xB2ACE8,
            viewport_map_x_offset: 0xBE5B80,
            viewport_map_y_offset: 0xBE5B84,
            chat_box_mode_offset: 0xDB24C0,
            screen_position_offset: 0xBE5B88,
            game_screen_offset: 0xB04AC0,
            order_pool_offset: 0xBA28C8,
            cheat_flags_offset: 0xD65D00,
            campaign_state_offset: 0xDB8670,
            local_game_result_offset: 0xD79C54,
            pylon_state_offset: 0xBA3734,
        },
        bullet: BulletLayout {
            root_block_module_offset: 0xb94a4c,
            root_block_bytes: 0x34,
            record_bytes: 0x70,
            pointer_bytes: 4,
            capacity: 400,
            root_free_head_offset: 0x00,
            root_free_tail_offset: 0x04,
            root_pool_base_offset: 0x20,
            root_active_head_offset: 0x2c,
            root_active_tail_offset: 0x30,
            prev_offset: 0x00,
            next_offset: 0x04,
            active_marker_offset: 0x08,
            sprite_ptr_offset: 0x0c,
            type_offset: 0x24,
            position_x_offset: 0x28,
            position_y_offset: 0x2a,
            velocity_x_offset: 0x40,
            velocity_y_offset: 0x44,
            velocity_direction_offset: 0x4a,
            source_player_offset: 0x4c,
            target_position_x_offset: 0x58,
            target_position_y_offset: 0x5a,
            target_unit_ptr_offset: 0x5c,
            remove_timer_offset: 0x61,
            source_unit_ptr_offset: 0x64,
        },
        map: MapLayout {
            pointer_bytes: 4,
            active_tile_pointer_module_offset: 0xba9478,
            sai_pathing_pointer_module_offset: 0xda6478,
            sai_pathing_allocation_bytes: 0x97a20,
            tile_grid: 0xc,
            split_table: 0x2000c,
            region_table: 0x449fc,
            region_stride: 0x40,
            region_neighbors_offset: 0xc,
            region_center_offset: 0x10,
            region_bounds_offset: 0x18,
            region_properties_offset: 0x20,
        },
        players: PlayersLayout {
            storm_players: 0xda1f48,
            storm_player_bytes: 0x68,
            net_player_to_unique: 0xd9f750,
            net_player_to_game: 0xd9f780,
            net_player_flags: 0xd9f7c8,
            original_chk_player_types: 0xd9f704,
            lobby_state: 0xd9f658,
            in_lobby_or_game: 0xda0f78,
            storm_races: 0xda3090,
            participation_table: 0xdb8698,
            // The pointer comes first and the flag at +4.
            participation_mode: 0xd65f54,
        },
        selection: SelectionLayout {
            command_table: 0xd9f710,
            ui_table: 0xdafc08,
            player_selections: 0xdafa88,
        },
        replay: ReplayLayout {
            header: 0xda4588,
            header_bytes: 0x279,
        },
        recorder: Some(RecorderLayout {
            object: 0xda2840,
            enabled: 0x00,
            buffer: 0x08,
            used: 0x0c,
            open_frame: 0x14,
            last_frame: 0x18,
        }),
        menu_root: MenuRootLayout {
            pointer: 0xb063cc,
            xor_key: 0x10fa_e560,
        },
        commands: CommandLayout {
            select_units: Some(0x32ca30),
            enqueue: Some(0x33a250),
            target_order_builder: Some(0x32b9f0),
            target_encoder: Some(0x1b4e00),
            move_screen: Some(0x281ad0),
            screen_max_x: Some(0xbe5b4c),
            screen_max_y: Some(0xbe5b50),
            print_text: Some(0x313200),
            send_text: Some(0x42efc0),
            chat_context: Some(ChatContextLayout { pointer: 0xb31320, xor_key: 0xbf65_2b26, offset: 0x58 }),
            end_game: Some(0x2cdfb0),
            recompute_turn_durations: Some(0x2df8c0),
            alt_speed_modifiers: Some(0xb24dac),
            step_game_frames: Some(0xb24d78),
            replay_vision: Some(0xda4578),
            local_vision: Some(0xd9f7c0),
            replay_reveal_all: Some(0xda457c),
            draw_limit_enabled: Some(0xd7759c),
            draw_limit_interval: Some(0xd775a4),
        },
        mouse: Some(MouseLayout { pointer: 0xba94ac, offset: 0x130 }),
        chat: Some(ChatLayout {
            dialog: 0xdb24c4,
            dialog_first_control: 0x5c,
            control_next: 0x00,
            control_id: 0x3a,
            control_text: 0x14,
            control_text_length: 0x18,
            text_box_id: 7,
            close: 0x39dde0,
            dialog_event: 0x44,
            event_key: 0x08,
            event_kind: 0x10,
            event_kind_char: 0x0f,
        }),
        network_provider: Some(0xda0f7c),
        save: Some(SaveLayout { name: 0xdaf8d8, name_bytes: 0x1c, tick: 0xdaf8f8 }),
        fonts: Some(FontLayout {
            table: 0xdcb5d4,
            count: 4,
            glyphs: 0,
            height_byte: 11,
            glyph_set: 0x14,
            outline: 0x18,
            outline_edge: 0.75,
            glyph_tree: 0x320,
            node_left: 0x04,
            node_right: 0x08,
            node_key: 0x10,
            node_glyph: 0x14,
            cache_glyph: 0x55c680,
            glyph_page: 0x04,
            glyph_bearing: 0x08,
            glyph_size: 0x10,
            glyph_atlas: 0x18,
            glyph_advance: 0x20,
            pages: 0xdcd038,
            page_texture: 0x562fc0,
            holder_texture: 0x08,
            atlas_size: 1024,
        }),
        render: Some(RenderLayout {
            renderer: 0xf5e348,
            vtable_entries: 28,
            render_entry: 7,
            create_texture_entry: 12,
            queue_capacity: 0x2000,
            command_bytes: 0xa0,
            queue_sort: 0x160004,
            sort_entry_bytes: 8,
            queue_count: 0x160028,
            layer: 28,
            vertices: 0xb3cb18,
            vertex_padding: 0x24,
            index_buffer: 0x30,
            command: DrawCommandLayout {
                render_target: 0x00,
                is_hd: 0x04,
                textures: 0x08,
                draw_mode: 0x24,
                shader: 0x28,
                vertex_offset: 0x2c,
                index_offset: 0x30,
                vertex_count: 0x34,
                used_count: 0x38,
                order_mark: 0x3c,
                constants: 0x50,
            },
            triangle_mode: 1,
            color_shader: 4,
            text_shader: 6,
            blank_texture: 0xdfe240,
            screen_height: 480,
        }),
        resource_writes: Some(ResourceWriteLayout {
            // A repair step's payment.
            repairs: &[0x18dac6, 0x18dad7],
            refunds: &[
                // A cancelled unit, queue slot, egg or cocoon: its full cost.
                0x27ce4e, 0x27ce64,
                // A cancelled building or Zerg building morph: 3/4.
                0x27ce9d, 0x27cec1,
                // A cancelled research and a cancelled upgrade.
                0x1d9fa7, 0x1d9fbc, 0x1da076, 0x1da0a1,
                // The 3/4 refunds the x86 build inlines where x64 calls the
                // building helper above.
                0x1da324, 0x1da347, 0x1da3f3, 0x1da42c,
            ],
        }),
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_selects_the_layout_by_module_image_size() {
        fn key(layout: &Layout) -> (u32, usize, usize, usize) {
            (layout.module_size, layout.pointer_bytes, layout.cunit.record_stride, layout.cunit.array_pointer_rva)
        }
        assert_eq!(Layout::detect(Layout::X64.module_size as usize).map(key), Some(key(&Layout::X64)));
        assert_eq!(Layout::detect(Layout::X86.module_size as usize).map(key), Some(key(&Layout::X86)));
        assert!(Layout::detect(0x2000).is_none());
    }

    #[test]
    fn x64_pointer_decode_keeps_eight_byte_little_endian_semantics() {
        let bytes = 0x0000_55aa_bbcc_ddee_usize.to_le_bytes();
        assert_eq!(
            read_pointer(&bytes, 0, Layout::X64.pointer_bytes),
            Some(0x0000_55aa_bbcc_ddee)
        );
        assert_eq!(read_pointer(&bytes[..7], 0, Layout::X64.pointer_bytes), None);
    }

    #[test]
    fn read_pointer_decodes_four_byte_width_for_x86() {
        let bytes = 0xaabb_ccdd_u32.to_le_bytes();
        assert_eq!(read_pointer(&bytes, 0, 4), Some(0xaabb_ccdd));
        assert_eq!(read_pointer(&bytes, 0, 2), None);
    }

    #[test]
    fn x64_cunit_offsets_are_consistent() {
        assert_eq!(Layout::X64.module_size, 0x145_0000);
        assert_eq!(Layout::X64.cunit.irradiate_player_id_offset, 0x1c0);
        assert_eq!(
            Layout::X64.cunit.irradiate_player_id_offset,
            Layout::X64.cunit.irradiated_by_ptr_offset + Layout::X64.cunit.irradiated_by_ptr_bytes
        );
        assert_eq!(
            Layout::X64.cunit.read_window_bytes,
            2 * Layout::X64.cunit.record_stride
        );
    }

    #[test]
    fn x86_cunit_pool_geometry_is_consistent() {
        assert_eq!(Layout::X86.cunit.record_stride, 0x150);
        assert_eq!(
            Layout::X86.cunit.region_bytes,
            3400 * Layout::X86.cunit.record_stride
        );
        assert_eq!(Layout::X86.pointer_bytes, 4);
        assert_eq!(Layout::X86.cunit.irradiated_by_ptr_bytes, 4);
        // The x86 pool has no +0x10 selection-table header.
        assert_eq!(Layout::X86.cunit.selection_ptr_adjustment, 0);
        assert_eq!(
            Layout::X86.cunit.read_window_bytes,
            2 * Layout::X86.cunit.record_stride
        );
        assert_eq!(
            Layout::X86.cunit.irradiate_player_id_offset,
            Layout::X86.cunit.irradiated_by_ptr_offset + Layout::X86.cunit.irradiated_by_ptr_bytes
        );
        assert_eq!(Layout::X86.module_size, 0x10CE000);
        assert_eq!(Layout::X86.cunit.remove_timer_offset, 0x110);
        assert_eq!(Layout::X86.cunit.lockdown_timer_offset, 0x117);
        assert_eq!(Layout::X86.cunit.maelstrom_timer_offset, 0x124);
        assert_eq!(Layout::X86.cunit.acid_spore_count_offset, 0x126);
    }

    #[test]
    fn command_functions_are_pinned_on_x64() {
        let commands = Layout::X64.commands;
        assert_eq!(commands.select_units, Some(0x37e930));
        assert_eq!(commands.enqueue, Some(0x3921a0));
        assert_eq!(commands.target_order_builder, Some(0x37a850));
    }

    #[test]
    fn unit_lists_fit_their_blocks_on_both_builds() {
        for layout in [&Layout::X64, &Layout::X86] {
            let c = &layout.cunit;
            let width = layout.pointer_bytes;
            let lists = c.unit_lists.expect("unit lists");
            // Every head pointer stays inside the header block the layout reads.
            for offset in lists.header_list_first_offsets.iter().chain(lists.header_list_last_offsets.iter()) {
                assert!(offset + width <= c.native_unit_list_header_bytes);
            }
            // Links of the header lists are the native Unit prev/next pair.
            assert_eq!(lists.header_list_prev_link_offset, c.selection_ptr_adjustment);
            assert_eq!(lists.header_list_next_link_offset, c.selection_ptr_adjustment + width);
            for offset in [
                lists.header_list_prev_link_offset,
                lists.header_list_next_link_offset,
                lists.invisible_list_prev_link_offset,
                lists.invisible_list_next_link_offset,
                lists.pylon_list_prev_link_offset,
                lists.pylon_list_next_link_offset,
            ] {
                assert!(offset + width <= c.record_stride);
            }
            // The rally point is the union over the pylon links, clear of the
            // invisible links.
            assert_eq!(c.rally_position_x_offset, lists.pylon_list_prev_link_offset);
            assert_eq!(c.rally_unit_ptr_offset, lists.pylon_list_next_link_offset);
            assert!(c.rally_position_x_offset >= lists.invisible_list_next_link_offset + width);
        }
        let lists = Layout::X64.cunit.unit_lists.expect("x64 unit lists");
        assert_eq!(lists.pylon_list_first_module_rva, 0xe7_80c0);
        assert_eq!(lists.invisible_list_first_module_rva, 0xe7_8450);
    }

    #[test]
    fn chat_dialog_event_slot_is_pinned_on_x64() {
        let chat = Layout::X64.chat.expect("x64 chat layout");
        // The match's dialog object holds its event handler at +0x60; an
        // event carries a u16 key at +0x10 and a u16 kind at +0x18.
        assert_eq!(chat.dialog_event, 0x60);
        assert_eq!(chat.event_key, 0x10);
        assert_eq!(chat.event_kind, 0x18);
        assert_eq!(chat.event_kind_char, 0x0f);
    }

    #[test]
    fn recorder_is_pinned_on_x64() {
        let recorder = Layout::X64.recorder.expect("x64 recorder layout");
        // The global points at the recorder object, which holds the enabled
        // byte, the buffer pointer, the bytes written, the open frame's first
        // command byte, and the last frame number.
        assert_eq!(recorder.object, 0x1093ab0);
        assert_eq!(recorder.enabled, 0x00);
        assert_eq!(recorder.buffer, 0x08);
        assert_eq!(recorder.used, 0x10);
        assert_eq!(recorder.open_frame, 0x18);
        assert_eq!(recorder.last_frame, 0x20);
    }

    #[test]
    fn recorded_command_lengths_cover_their_payload() {
        use super::recorder_command_length as length;
        // A chat line is the id, the player byte, and 80 bytes of text.
        assert_eq!(length(0x5c, 0), Some(82));
        // The variable-length kinds count their entries in the second byte.
        assert_eq!(length(0x09, 3), Some(8));
        assert_eq!(length(0x63, 1), Some(6));
        assert_eq!(length(0x18, 0), Some(1));
        assert_eq!(length(0x5a, 0), Some(1));
        assert_eq!(length(0x0f, 0), Some(2));
        assert_eq!(length(0x1f, 0), Some(3));
        assert_eq!(length(0x0c, 0), Some(8));
        assert_eq!(length(0x14, 0), Some(10));
        assert_eq!(length(0x60, 0), Some(12));
        // Ids the recorder never writes have no length.
        assert_eq!(length(0x05, 0), None);
        assert_eq!(length(0x66, 0), None);
    }
}
