//! Readers of the player, game, force, start location and availability tables.

use std::sync::Arc;

use bwrapi_layout::Layout;

use crate::columns::{Cell, Table};
use crate::decode::bwgame::*;
use crate::decode::map::BWGAME_MAP_WIDTH_OFFSET;
use crate::decode::scalars::PLAYER_IDENTITY_ENTRY_STRIDE;
use crate::fake::{in_game, put_unit, FakeMemory, Shared, POOL, ROOT};
use crate::tables::source;
use crate::Engine;

const SPEED_OBJECT: usize = 0x5_0000_0000;

fn engine(memory: &Arc<FakeMemory>, sources: u64) -> Engine {
    let mut engine = Engine::with_memory(Box::new(Shared(Arc::clone(memory)))).unwrap();
    engine.subscribe(sources);
    engine
}

fn root_put(memory: &FakeMemory, offset: usize, bytes: &[u8]) {
    memory.patch(ROOT + offset, bytes);
}

fn root_i32(memory: &FakeMemory, offset: usize, value: i32) {
    root_put(memory, offset, &value.to_le_bytes());
}

fn tile_center(tile_x: i16, tile_y: i16) -> [u8; 4] {
    let [x0, x1] = (tile_x * 32 + 64).to_le_bytes();
    let [y0, y1] = (tile_y * 32 + 48).to_le_bytes();
    [x0, x1, y0, y1]
}

/// Slot 0 is player 0 (Terran human, force 1, "Alpha"); slot 1 is player 3
/// (Zerg computer, force 2, "Beta"), so slot and id differ.
fn two_players() -> Arc<FakeMemory> {
    let memory = in_game(1);
    let mut identities = vec![0u8; 8 * PLAYER_IDENTITY_ENTRY_STRIDE];
    for (slot, (id, kind, race, team, name)) in [(0u32, 2u8, 1u8, 1u8, "Alpha"), (3, 1, 0, 2, "Beta")].into_iter().enumerate() {
        let entry = &mut identities[slot * PLAYER_IDENTITY_ENTRY_STRIDE..][..PLAYER_IDENTITY_ENTRY_STRIDE];
        entry[..4].copy_from_slice(&id.to_le_bytes());
        entry[8..11].copy_from_slice(&[kind, race, team]);
        entry[0x0b..0x0b + name.len()].copy_from_slice(name.as_bytes());
    }
    memory.put_module(Layout::X64.scalars.player_identity_table_offset, &identities);

    // Resources are indexed by slot.
    root_i32(&memory, BWGAME_MINERALS_OFFSET, 50);
    root_i32(&memory, BWGAME_MINERALS_OFFSET + 4, 75);
    root_i32(&memory, BWGAME_GAS_OFFSET, 10);
    root_i32(&memory, BWGAME_GAS_OFFSET + 4, 20);
    root_i32(&memory, BWGAME_CUMULATIVE_MINERALS_OFFSET + 4, 900);
    root_i32(&memory, BWGAME_CUMULATIVE_GAS_OFFSET + 4, 300);
    // Supply: [race][available, used, max][slot]; slot 1 Zerg and Terran.
    for (race, available, used, max) in [(0usize, 18, 9, 400), (1, 30, 12, 200)] {
        let base = BWGAME_SUPPLY_TABLE_OFFSET + race * 144 + 4;
        root_i32(&memory, base, available);
        root_i32(&memory, base + 48, used);
        root_i32(&memory, base + 96, max);
    }
    root_put(&memory, BWGAME_PLAYER_COLOR_INDEX_OFFSET, &[6, 7]);
    root_put(&memory, BWGAME_PLAYER_VICTORY_STATUS_OFFSET + 1, &[3]);
    root_put(&memory, BWGAME_PLAYER_LEFT_OFFSET + 1, &[1]);
    root_i32(&memory, BWGAME_PLAYER_SCORE_UNIT_OFFSET + 4, 11);
    root_i32(&memory, BWGAME_PLAYER_SCORE_KILL_OFFSET + 4, 12);
    root_i32(&memory, BWGAME_PLAYER_SCORE_BUILDING_OFFSET + 4, 13);
    root_i32(&memory, BWGAME_PLAYER_SCORE_RAZING_OFFSET + 4, 14);
    root_i32(&memory, BWGAME_PLAYER_SCORE_CUSTOM_OFFSET + 4, 15);
    // Slot 0 allies slot 1 (player 3); slot 1 allies itself and slot 0.
    root_put(&memory, BWGAME_PLAYER_ALLIANCES_OFFSET + 1, &[2]);
    root_put(&memory, BWGAME_PLAYER_ALLIANCES_OFFSET + BWGAME_ALLIANCE_PLAYER_SLOTS, &[1, 1]);
    // 64x64 map; slot 1 starts at tile (10, 11), slot 0 at (5, 6).
    root_put(&memory, BWGAME_MAP_WIDTH_OFFSET, &[64, 0, 64, 0]);
    root_put(&memory, BWGAME_START_POSITIONS_OFFSET, &tile_center(5, 6));
    root_put(&memory, BWGAME_START_POSITIONS_OFFSET + 4, &tile_center(10, 11));
    memory
}

#[test]
fn players_are_decoded_from_slot_indexed_tables_and_published_by_id() {
    let memory = two_players();
    let frame = engine(&memory, source::PLAYERS).acquire().unwrap();
    assert_eq!(frame.players.id, [0, 3]);
    let alpha = frame.players.row(0).unwrap();
    let beta = frame.players.row(1).unwrap();

    assert_eq!((alpha.race, alpha.player_type, alpha.force, alpha.name.as_str()), (1, 2, 1, "Alpha"));
    assert_eq!((beta.race, beta.player_type, beta.force, beta.name.as_str()), (0, 1, 2, "Beta"));
    assert_eq!((alpha.minerals, alpha.gas), (50, 10));
    assert_eq!((beta.minerals, beta.gas, beta.gathered_minerals, beta.gathered_gas), (75, 20, 900, 300));
    assert_eq!((alpha.color, beta.color), (6, 7));
    assert_eq!((beta.supply_used_zerg, beta.supply_used_terran, beta.supply_used_protoss), (9, 12, 0));
    // The published total is min(available, max).
    assert_eq!((beta.supply_total_zerg, beta.supply_total_terran, beta.supply_total_protoss), (18, 30, 0));
    assert_eq!((alpha.supply_used_zerg, alpha.supply_total_zerg), (0, 0));
    assert_eq!((alpha.start_x, alpha.start_y, beta.start_x, beta.start_y), (5, 6, 10, 11));
    assert_eq!((alpha.victory, beta.victory, alpha.left_game, beta.left_game), (0, 3, 0, 1));
    assert_eq!(
        (beta.unit_score, beta.kill_score, beta.building_score, beta.razing_score, beta.custom_score),
        (11, 12, 13, 14, 15)
    );
    // Slot 0 allies slot 1, which is player 3; slot 1 allies slots 0 and 1.
    assert_eq!((alpha.allies, beta.allies), (1 << 3, 1 << 0 | 1 << 3));
    // Without the in-game DLL the repair and refund totals stay unknown.
    assert_eq!((alpha.refunded_minerals, beta.repaired_gas), (i32::UNKNOWN, i32::UNKNOWN));
}

#[test]
fn players_carry_the_resource_totals_of_their_bwgame_slot() {
    let memory = two_players();
    let mut engine = engine(&memory, source::PLAYERS);
    let mut totals = [crate::ResourceTotals::default(); crate::RESOURCE_SLOTS];
    // Identity slot 1 is player 3.
    totals[1] = crate::ResourceTotals { repaired_minerals: 1, repaired_gas: 2, refunded_minerals: 75, refunded_gas: 25 };
    engine.set_resource_totals(Some(totals));
    let frame = engine.acquire().unwrap();
    assert_eq!(frame.players.id, [0, 3]);
    assert_eq!(frame.players.refunded_minerals, [0, 75]);
    assert_eq!((frame.players.repaired_minerals[1], frame.players.repaired_gas[1], frame.players.refunded_gas[1]), (1, 2, 25));
}

#[test]
fn upgrades_and_techs_carry_the_player_id_of_their_slot() {
    let memory = two_players();
    // Identity slot 1 is player 3: its tables are at slot 1, its buildings are owned by 3.
    memory.patch(ROOT + BWGAME_MAX_UPGRADE_SC_OFFSET + BWGAME_SC_UPGRADE_COUNT + 5, &[3]);
    memory.patch(ROOT + BWGAME_UPGRADE_IN_PROGRESS_OFFSET + BWGAME_UPGRADE_IN_PROGRESS_STRIDE, &[1 << 5]);
    memory.patch(ROOT + BWGAME_TECH_RESEARCHED_SC_OFFSET + BWGAME_SC_TECH_COUNT + 4, &[1]);
    let c = &Layout::X64.cunit;
    put_unit(&memory, 0, 122, 3, 50, 50, 1);
    memory.patch(POOL + c.order_offset, &[76]);
    memory.patch(POOL + c.upgrade_type_offset, &[5]);
    memory.patch(POOL + c.production_remaining_time_offset, &300u16.to_le_bytes());
    let frame = engine(&memory, source::UPGRADES | source::TECHS).acquire().unwrap();
    assert_eq!(frame.upgrades.player, [3]);
    assert_eq!(frame.upgrades.remaining_frames, [300]);
    let researched = (0..frame.techs.rows()).map(|row| frame.techs.row(row).unwrap()).find(|row| row.researched == 1).unwrap();
    assert_eq!((researched.player, researched.tech_type), (3, 4));
    assert!(frame.techs.player.iter().all(|player| [0, 3].contains(player)));
}

#[test]
fn participation_is_read_on_frame_zero_from_start_units_and_triggers() {
    use crate::decode::participation::TRIGGER_BYTES;
    const NODE: usize = 0x6_0000_0000;
    let memory = two_players();
    let players = &Layout::X64.players;
    memory.put_module(players.participation_mode, &[0; 12]);
    // Every executor's list is empty: it closes on its own head.
    let table_address = memory.module.base_address + players.participation_table;
    let mut table = vec![0u8; 8 * 24];
    for executor in 0..8 {
        let sentinel = (table_address + executor * 24) as u64;
        table[executor * 24..][..8].copy_from_slice(&sentinel.to_le_bytes());
        table[executor * 24 + 8..][..8].copy_from_slice(&sentinel.to_le_bytes());
    }
    memory.put_module(players.participation_table, &table);
    // Slot 0 (player 0) owns a unit at the start; player 3 owns none.
    root_i32(&memory, BWGAME_UNIT_COUNT_ALL_OFFSET + (7 * BWGAME_UNIT_COUNT_PLAYER_SLOTS) * 4, 1);
    let participation = |memory: &Arc<FakeMemory>| {
        let frame = engine(memory, source::PLAYERS).acquire().unwrap();
        (frame.players.participating[0], frame.players.participating[1])
    };
    assert_eq!(participation(&memory), (1, 0));

    // A Create Unit action of executor 0 for player 3 makes it participate.
    let sentinel = table_address as u64;
    let mut node = vec![0u8; 16 + TRIGGER_BYTES];
    node[..8].copy_from_slice(&sentinel.to_le_bytes());
    node[8..16].copy_from_slice(&sentinel.to_le_bytes());
    let action = 16 + 320;
    node[action + 16..action + 20].copy_from_slice(&3u32.to_le_bytes());
    node[action + 26] = 44;
    memory.put(NODE, &node);
    let mut head = table[..24].to_vec();
    head[..8].copy_from_slice(&(NODE as u64).to_le_bytes());
    head[8..16].copy_from_slice(&(NODE as u64).to_le_bytes());
    head[16..24].copy_from_slice(&1u64.to_le_bytes());
    memory.patch(table_address, &head);
    assert_eq!(participation(&memory), (1, 1));

    // A list that does not close on its head is not trusted.
    memory.patch(NODE + 8, &0u64.to_le_bytes());
    assert_eq!(participation(&memory), (u8::UNKNOWN, u8::UNKNOWN));
    memory.patch(NODE + 8, &sentinel.to_le_bytes());

    // Past the first frame the trigger lists are no longer the start state.
    root_put(&memory, BWGAME_FRAME_COUNT_OFFSET, &1u32.to_le_bytes());
    assert_eq!(participation(&memory), (u8::UNKNOWN, u8::UNKNOWN));
}

#[test]
fn a_network_player_whose_flags_clear_has_left() {
    let memory = two_players();
    let players = &Layout::X64.players;
    memory.put_module(Layout::X64.scalars.session_network_in_match_offset, &[1]);
    // Network players 0 and 1 hold identity slots 0 and 1; the others none.
    let mut to_game = Vec::new();
    for slot in [0u32, 1, 8, 8, 8, 8, 8, 8] {
        to_game.extend_from_slice(&slot.to_le_bytes());
    }
    memory.put_module(players.net_player_to_game, &to_game);
    let mut flags = vec![0u8; 32];
    flags[..4].copy_from_slice(&0x70000u32.to_le_bytes());
    memory.put_module(players.net_player_flags, &flags);
    root_put(&memory, BWGAME_PLAYER_LEFT_OFFSET + 1, &[0]);
    let frame = engine(&memory, source::PLAYERS).acquire().unwrap();
    assert_eq!((frame.players.row(0).unwrap().left_game, frame.players.row(1).unwrap().left_game), (0, 1));
    // Outside a network match nobody left through those tables.
    memory.put_module(Layout::X64.scalars.session_network_in_match_offset, &[0]);
    let frame = engine(&memory, source::PLAYERS).acquire().unwrap();
    assert_eq!(frame.players.row(1).unwrap().left_game, 0);
}

#[test]
fn player_values_the_game_rejects_stay_unknown() {
    let memory = two_players();
    root_i32(&memory, BWGAME_MINERALS_OFFSET, -1);
    root_i32(&memory, BWGAME_PLAYER_SCORE_KILL_OFFSET, -5);
    root_put(&memory, BWGAME_PLAYER_LEFT_OFFSET, &[9]);
    root_put(&memory, BWGAME_MAP_WIDTH_OFFSET, &[0, 0, 0, 0]);
    let frame = engine(&memory, source::PLAYERS).acquire().unwrap();
    let alpha = frame.players.row(0).unwrap();
    assert_eq!((alpha.minerals, alpha.gas), (i32::UNKNOWN, i32::UNKNOWN));
    assert_eq!((alpha.kill_score, alpha.unit_score), (i32::UNKNOWN, 0));
    assert_eq!(alpha.left_game, u8::UNKNOWN);
    assert_eq!((alpha.start_x, alpha.start_y), (i16::UNKNOWN, i16::UNKNOWN));
}

#[test]
fn unit_counts_read_all_four_tables_by_slot_and_skip_all_zero_rows() {
    let memory = two_players();
    let at = |table: usize, unit_type: usize, slot: usize| table + (unit_type * BWGAME_UNIT_COUNT_PLAYER_SLOTS + slot) * 4;
    root_i32(&memory, at(BWGAME_UNIT_COUNT_ALL_OFFSET, 37, 1), 3);
    root_i32(&memory, at(BWGAME_UNIT_COUNT_COMPLETED_OFFSET, 37, 1), 2);
    root_i32(&memory, at(BWGAME_UNIT_COUNT_KILLED_OFFSET, 37, 1), 5);
    root_i32(&memory, at(BWGAME_UNIT_COUNT_DEAD_OFFSET, 37, 1), 7);
    // Only killed: still a row.
    root_i32(&memory, at(BWGAME_UNIT_COUNT_KILLED_OFFSET, 40, 0), 1);
    // A slot without an identity is not published.
    root_i32(&memory, at(BWGAME_UNIT_COUNT_ALL_OFFSET, 41, 2), 4);
    let frame = engine(&memory, source::UNIT_COUNTS).acquire().unwrap();
    assert_eq!(frame.unit_counts.rows(), 2);
    let first = frame.unit_counts.row(0).unwrap();
    let second = frame.unit_counts.row(1).unwrap();
    assert_eq!((first.player, first.unit_type, first.all, first.completed, first.killed, first.dead), (0, 40, 0, 0, 1, 0));
    assert_eq!((second.player, second.unit_type, second.all, second.completed, second.killed, second.dead), (3, 37, 3, 2, 5, 7));
}

#[test]
fn game_row_decodes_session_timing_seed_and_map_names() {
    let memory = in_game(1);
    let layout = &Layout::X64;
    let scalars = &layout.scalars;
    memory.put_module(layout.replay.header + 0x3c, &10u16.to_le_bytes());
    memory.put_module(scalars.session_paused_offset, &[1]);
    memory.put_module(scalars.session_network_in_match_offset, &[1]);
    memory.put_module(scalars.session_latency_raw_offset, &1u32.to_le_bytes());
    memory.put_module(scalars.session_builtin_turn_latency_offset, &4u32.to_le_bytes());
    memory.put_module(scalars.session_sync_active_offset, &1u32.to_le_bytes());
    memory.put_module(layout.network_provider.unwrap(), b"TENB");
    memory.put_module(scalars.game_speed_pointer_offset, &(SPEED_OBJECT as u64).to_le_bytes());
    memory.put(SPEED_OBJECT + 8, &2u32.to_le_bytes());
    let delays: Vec<u8> = [167u32, 111, 83, 67, 56, 48, 42].iter().flat_map(|value| value.to_le_bytes()).collect();
    memory.put_module(scalars.game_speed_primary_delays_offset, &delays);
    memory.put_module(scalars.initial_random_seed_offset, &0xdead_beefu32.to_le_bytes());
    memory.put_module(scalars.replay_frame_count_offset, &4321u32.to_le_bytes());
    root_put(&memory, BWGAME_MAP_FILE_NAME_OFFSET, b"C:\\maps\\Fighting Spirit.scx\0");
    root_put(&memory, BWGAME_MAP_TITLE_OFFSET, b"  Fighting Spirit \0");
    root_put(&memory, BWGAME_COUNTDOWN_SECONDS_OFFSET, &7u32.to_le_bytes());
    root_put(&memory, BWGAME_ELAPSED_TIME_OFFSET, &123u32.to_le_bytes());

    let frame = engine(&memory, source::GAME).acquire().unwrap();
    assert_eq!(frame.game.rows(), 1);
    let game = frame.game.row(0).unwrap();
    assert_eq!((game.game_type, game.multiplayer, game.paused), (10, 1, 1));
    assert_eq!((game.latency, game.latency_frames), (1, 5));
    assert_eq!(game.battle_net, 1);
    assert_eq!((game.frame_ms, game.elapsed_seconds, game.countdown_seconds), (83, 123, 7));
    assert_eq!((game.random_seed, game.replay_frames), (0xdead_beef, 4321));
    assert_eq!(game.map_path.as_str(), "C:\\maps\\Fighting Spirit.scx");
    assert_eq!(game.map_file.as_str(), "Fighting Spirit.scx");
    assert_eq!(game.map_title.as_str(), "Fighting Spirit");
}

#[test]
fn game_row_keeps_unreadable_values_unknown() {
    let memory = in_game(1);
    // Nothing of the session scalars is mapped; the BWGame fields are zero.
    let frame = engine(&memory, source::GAME).acquire().unwrap();
    let game = frame.game.row(0).unwrap();
    assert_eq!(
        (game.game_type, game.multiplayer, game.paused, game.latency, game.latency_frames, game.frame_ms),
        (u8::UNKNOWN, u8::UNKNOWN, u8::UNKNOWN, u8::UNKNOWN, u32::UNKNOWN, u32::UNKNOWN)
    );
    assert_eq!((game.random_seed, game.replay_frames), (u32::UNKNOWN, u32::UNKNOWN));
    assert_eq!((game.elapsed_seconds, game.countdown_seconds), (0, 0));
    assert_eq!((game.map_path.as_str(), game.map_file.as_str(), game.map_title.as_str()), ("", "", ""));
}

#[test]
fn forces_start_locations_and_availability_are_decoded() {
    let memory = two_players();
    for (index, name) in ["Team A", "Team B", "", "Team D"].iter().enumerate() {
        root_put(&memory, BWGAME_FORCE_NAMES_OFFSET + index * BWGAME_FORCE_NAME_BYTES, name.as_bytes());
    }
    // A repeated, an unused and an off-grid slot are dropped.
    root_put(&memory, BWGAME_START_POSITIONS_OFFSET + 8, &tile_center(5, 6));
    root_put(&memory, BWGAME_START_POSITIONS_OFFSET + 12, &[0, 0, 0, 0]);
    root_put(&memory, BWGAME_START_POSITIONS_OFFSET + 16, &[1, 0, 1, 0]);
    root_put(&memory, BWGAME_START_POSITIONS_OFFSET + 20, &tile_center(20, 21));
    // Slot 1 (player 3) may build unit type 7, slot 0 (player 0) type 8.
    root_put(&memory, BWGAME_PLAYER_UNIT_AVAILABILITY_OFFSET + 8, &[1]);
    root_put(&memory, BWGAME_PLAYER_UNIT_AVAILABILITY_OFFSET + BWGAME_UNIT_AVAILABILITY_TYPE_COUNT + 7, &[1]);

    let sources = source::FORCES | source::START_LOCATIONS | source::UNIT_AVAILABILITY;
    let frame = engine(&memory, sources).acquire().unwrap();

    assert_eq!(frame.forces.id, [1, 2, 3, 4]);
    let names: Vec<&str> = (0..4).map(|index| frame.forces.name[index].as_str()).collect();
    assert_eq!(names, ["Team A", "Team B", "", "Team D"]);

    assert_eq!(frame.start_locations.x, [5, 10, 20]);
    assert_eq!(frame.start_locations.y, [6, 11, 21]);

    assert_eq!(frame.unit_availability.rows(), 2 * BWGAME_UNIT_AVAILABILITY_TYPE_COUNT);
    let available = |player: u8, unit_type: u16| {
        let index = (0..frame.unit_availability.rows())
            .find(|index| frame.unit_availability.player[*index] == player && frame.unit_availability.unit_type[*index] == unit_type)
            .unwrap();
        frame.unit_availability.available[index]
    };
    assert_eq!((available(0, 8), available(0, 7), available(3, 7), available(3, 8)), (1, 0, 1, 0));
    assert!((0..frame.unit_availability.rows()).all(|index| matches!(frame.unit_availability.player[index], 0 | 3)));
}

#[test]
fn a_source_that_is_not_subscribed_reads_none_of_its_memory() {
    let memory = two_players();
    let identities = Layout::X64.scalars.player_identity_table_offset + memory.module.base_address;

    // Nothing from the BWGame root tables or the identity table for units only.
    let mut units = engine(&memory, source::UNITS);
    units.acquire().unwrap();
    assert_eq!(memory.reads_in(ROOT, 1), 0);
    assert_eq!(memory.reads_in(ROOT + 0x150, BWGAME_METADATA_READ_BYTES - 0x150), 0);
    assert_eq!(memory.reads_in(identities, 8 * PLAYER_IDENTITY_ENTRY_STRIDE), 0);

    // Unit counts do not read the players block or the game span.
    memory.reads.lock().unwrap().clear();
    engine(&memory, source::UNIT_COUNTS).acquire().unwrap();
    assert_eq!(memory.reads_in(ROOT, 1), 0);
    assert_eq!(memory.reads_in(ROOT + BWGAME_MAP_FILE_NAME_OFFSET, BWGAME_UNIT_COUNT_ALL_OFFSET - BWGAME_MAP_FILE_NAME_OFFSET), 0);
    assert_eq!(memory.reads_in(ROOT + BWGAME_PLAYER_UNIT_AVAILABILITY_OFFSET, 1), 0);

    // Players, forces, start locations and availability do not read the counts.
    memory.reads.lock().unwrap().clear();
    let others = source::PLAYERS | source::FORCES | source::START_LOCATIONS | source::UNIT_AVAILABILITY | source::GAME;
    engine(&memory, others).acquire().unwrap();
    assert_eq!(memory.reads_in(ROOT + BWGAME_UNIT_COUNT_ALL_OFFSET, 4 * BWGAME_UNIT_COUNT_TABLE_BYTES), 0);

    // The game row reads neither the identity table nor the root's player tables.
    memory.reads.lock().unwrap().clear();
    engine(&memory, source::GAME).acquire().unwrap();
    assert_eq!(memory.reads_in(identities, 8 * PLAYER_IDENTITY_ENTRY_STRIDE), 0);
    assert_eq!(memory.reads_in(ROOT, 1), 0);
    assert_eq!(memory.reads_in(ROOT + BWGAME_UNIT_COUNT_ALL_OFFSET, 4 * BWGAME_UNIT_COUNT_TABLE_BYTES), 0);
}
