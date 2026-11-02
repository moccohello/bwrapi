// PlayerData and the game scalars built from the engine tables.
#include "legacy44_test_support.h"

#include <BWAPI/Latency.h>
#include <BWAPI/PlayerType.h>
#include <BWAPI/Position.h>
#include <BWAPI/TechType.h>
#include <BWAPI/UnitType.h>
#include <BWAPI/UpgradeType.h>

#include <filesystem>
#include <fstream>
#include <string>

using namespace BWRAPI::Legacy44;
using namespace BWRAPI::Legacy44::Test;
namespace UT = BWAPI::UnitTypes::Enum;

namespace {

void player_values() {
  Env env;
  env.fx.local_player = 1;
  env.fx.game_frame = 0;
  auto self = player_row(1, 2);
  self.participating = 1;
  set_text(self.name, "Artanis");
  self.minerals = 150;
  self.gas = 25;
  self.player_type = BWAPI::PlayerTypes::Enum::Player;
  self.force = 2;
  self.color = 4;
  self.supply_used_protoss = 12;
  self.supply_total_protoss = 18;
  self.start_x = 10;
  self.start_y = 99;
  self.victory = 3;
  self.left_game = 0;
  self.gathered_minerals = 900;
  self.unit_score = 77;
  self.allies = 0b0110;  // allied with players 1 and 2
  auto enemy = player_row(0, 0);
  enemy.player_type = BWAPI::PlayerTypes::Enum::Computer;
  enemy.victory = 2;
  enemy.left_game = 1;
  enemy.participating = 1;
  env.fx.players.rows = {enemy, self};
  CHECK(env.project());
  // The server's player vector holds the playable slots plus the neutral.
  CHECK(env.data->playerCount == 3);
  CHECK(env.sample.players.id(0) == 0 && env.sample.players.id(1) == 1 &&
        env.sample.players.id(11) == 2);
  const auto& p = env.data->players[1];
  CHECK(std::string(p.name) == "Artanis");
  CHECK(p.race == 2 && p.minerals == 150 && p.gas == 25);
  CHECK(p.type == BWAPI::PlayerTypes::Enum::Player && p.force == 2 && p.color == 4);
  CHECK(p.supplyUsed[2] == 12 && p.supplyTotal[2] == 18);
  CHECK(p.startLocationX == 10 && p.startLocationY == 99);
  CHECK(p.isVictorious && !p.isDefeated && !p.leftGame);
  CHECK(p.gatheredMinerals == 900);
  CHECK(p.totalUnitScore == 77);
  // Alliance holds only between participating players: ID 2 is neutral.
  CHECK(!p.isAlly[0] && p.isAlly[1] && !p.isAlly[2]);
  CHECK(p.isEnemy[0] && !p.isEnemy[1] && !p.isEnemy[2] && !p.isEnemy[11]);
  const auto& e = env.data->players[0];
  CHECK(e.isDefeated && !e.isVictorious && e.leftGame);
  // A slot that left reads as left, as the original game's slot type does.
  CHECK(e.type == BWAPI::PlayerTypes::Enum::ComputerLeft);
  // The enemy's alliance matrix is unknown, not empty.
  CHECK(!e.isAlly[1] && !e.isEnemy[1]);
  // self is the local player; the enemy is its unallied opponent slot.
  CHECK(env.data->self == 1);
  CHECK(env.data->enemy == 0);
  CHECK(p.isParticipating && e.isParticipating);
  // The neutral player follows the playable players.
  const auto& neutral = env.data->players[2];
  CHECK(neutral.isNeutral && std::string(neutral.name) == "Neutral" &&
        neutral.type == BWAPI::PlayerTypes::Enum::Neutral && env.data->neutral == 2);
}

void only_the_playable_slots_have_a_victory_state() {
  Env env;
  auto high = player_row(9, 0);
  high.victory = 3;
  high.left_game = 1;
  high.player_type = BWAPI::PlayerTypes::Enum::Player;
  env.fx.players.rows = {high};
  CHECK(env.project());
  // Slot 9 is outside the server's player set: it has no ID and no row, and
  // only the neutral player counts.
  CHECK(env.sample.players.id(9) == -1);
  CHECK(env.data->playerCount == 1 && env.data->neutral == 0 && env.data->players[0].isNeutral);
  CHECK(!env.data->players[0].isVictorious && !env.data->players[0].leftGame);
}

// GameEvents.cpp onGameStart: the playable slots whose type is set and below
// Closed get IDs in slot order, then the neutral player. A slot that left
// keeps its place, and so does one of unknown type. The IDs hold for the match.
void player_ids_follow_the_start_set() {
  Env env;
  namespace PT = BWAPI::PlayerTypes::Enum;
  auto row = [](int slot, int type) {
    auto player = player_row(slot, 0);
    player.player_type = static_cast<std::uint8_t>(type);
    return player;
  };
  env.fx.players.rows = {row(0, PT::None),       row(1, PT::Computer), row(2, PT::Closed),
                         row(3, PT::Observer),   row(4, PT::PlayerLeft), player_row(5, 0),
                         row(6, PT::RescuePassive), row(7, PT::None),  row(11, PT::Neutral)};
  CHECK(env.project());
  const auto& ids = env.sample.players;
  CHECK(ids.id(0) == -1 && ids.id(1) == 0 && ids.id(2) == -1 && ids.id(3) == -1);
  CHECK(ids.id(4) == 1 && ids.id(5) == 2 && ids.id(6) == 3 && ids.id(7) == -1);
  CHECK(ids.id(11) == 4 && env.data->neutral == 4 && env.data->playerCount == 5);
  CHECK(ids.slot(3) == 6 && ids.slot(5) == -1);
  // A later type change does not renumber the match.
  env.fx.players.rows[0].player_type = PT::Player;
  env.fx.players.rows[1].player_type = PT::ComputerLeft;
  CHECK(env.project());
  CHECK(env.sample.players.id(0) == -1 && env.sample.players.id(1) == 0 && env.data->playerCount == 5);
  CHECK(env.data->players[0].type == PT::ComputerLeft);
  // A new match numbers again.
  env.fx.match_id = 2;
  CHECK(env.project());
  CHECK(env.sample.players.id(0) == 0 && env.sample.players.id(1) == 1 && env.data->playerCount == 6);
}

// A 1v1 on an eight-player map: slots 2..7 are empty, so getPlayers() is the
// two players and the neutral player, and every player-indexed cell names them
// by ID.
void one_versus_one_skips_empty_slots() {
  Env env;
  namespace PT = BWAPI::PlayerTypes::Enum;
  env.fx.local_player = 0;
  env.fx.game_frame = 0;
  auto human = player_row(0, 1);
  human.player_type = PT::Player;
  human.participating = 1;
  human.allies = 0b1;
  human.minerals = 50;
  auto computer = player_row(1, 2);
  computer.player_type = PT::Computer;
  computer.participating = 1;
  computer.allies = 0b10;
  computer.minerals = 75;
  env.fx.players.rows = {human, computer};
  for (int slot = 2; slot < 8; ++slot) {
    auto empty = player_row(slot, 0);
    empty.player_type = PT::None;
    empty.participating = 0;
    env.fx.players.rows.push_back(empty);
  }
  auto neutral = player_row(11, 4);
  neutral.player_type = PT::Neutral;
  neutral.participating = 0;
  neutral.minerals = 9;
  env.fx.players.rows.push_back(neutral);
  env.fx.units.rows = {unit_row(1, UT::Terran_SCV, 0, 10, 10),
                       unit_row(2, UT::Protoss_Probe, 1, 20, 20),
                       unit_row(3, UT::Resource_Mineral_Field, 11, 30, 30)};
  env.fx.render.rows = {{unit_id(1), 10, 10, 0b11}, {unit_id(2), 20, 20, 0b11}, {unit_id(3), 30, 30, 0b11}};
  env.fx.units.rows[0].last_attacker = 1;
  Fixture::UnitCountsRows::Row minerals;
  minerals.player = 11;
  minerals.unit_type = UT::Resource_Mineral_Field;
  minerals.all = 1;
  env.fx.unit_counts.rows = {minerals};
  CHECK(env.project());
  const auto& d = *env.data;
  CHECK(d.playerCount == 3 && d.self == 0 && d.enemy == 1 && d.neutral == 2);
  // The enemy's and the neutral player's resources are hidden.
  CHECK(d.players[0].minerals == 50 && d.players[1].minerals == 0 && d.players[2].minerals == 0);
  CHECK(d.players[2].isNeutral && std::string(d.players[2].name) == "Neutral");
  CHECK(d.players[2].allUnitCount[UT::Resource_Mineral_Field] == 1);
  // No empty slot reaches a row.
  for (int id = 3; id < 12; ++id) CHECK(d.players[id].type == PT::None && !d.players[id].isParticipating);
  CHECK(d.players[0].isAlly[0] && d.players[0].isEnemy[1] && !d.players[0].isEnemy[2]);
  CHECK(d.players[1].isEnemy[0] && !d.players[1].isAlly[2] && !d.players[1].isEnemy[2]);
  CHECK(!d.players[0].isEnemy[11] && !d.players[2].isEnemy[0]);
  // Unit owners are IDs; lastAttackerPlayer stays the engine slot as upstream.
  CHECK(env.unit(env.sample.units[0].id).player == 0);
  CHECK(env.unit(env.sample.units[1].id).player == 1);
  CHECK(env.unit(env.sample.units[2].id).player == 2);
  CHECK(env.unit(env.sample.units[0].id).lastAttackerPlayer == 1);
  // Commands name players by ID.
  CHECK(env.projection.player_slot(2) == 11 && env.projection.player_slot(1) == 1 &&
        env.projection.player_slot(3) == -1);
}

// GameEvents.cpp "find the opponent": the last unallied playing slot wins, and
// the pick stays for the rest of the match.
void enemy_is_the_last_unallied_opponent() {
  Env env;
  env.fx.local_player = 0;
  env.fx.game_frame = 0;
  auto self = player_row(0, 1);
  self.player_type = BWAPI::PlayerTypes::Enum::Player;
  self.allies = 0b1001;
  auto foe_a = player_row(2, 0);
  foe_a.player_type = BWAPI::PlayerTypes::Enum::Computer;
  auto ally = player_row(3, 2);
  ally.player_type = BWAPI::PlayerTypes::Enum::Player;
  auto foe_b = player_row(5, 0);
  foe_b.player_type = BWAPI::PlayerTypes::Enum::EitherPreferComputer;
  // An observer of a playing type is never the enemy.
  auto observer = player_row(6, 0);
  observer.player_type = BWAPI::PlayerTypes::Enum::Player;
  for (auto* row : {&self, &foe_a, &ally, &foe_b}) row->participating = 1;
  observer.participating = 0;
  env.fx.players.rows = {self, foe_a, ally, foe_b, observer};
  CHECK(env.project());
  // Slots 0, 2, 3, 5 and 6 are IDs 0..4.
  CHECK(env.data->enemy == 3);
  CHECK(env.data->players[0].isAlly[2] && !env.data->players[0].isEnemy[2]);
  CHECK(env.data->players[0].isEnemy[1] && env.data->players[0].isEnemy[3]);
  CHECK(!env.data->players[0].isEnemy[4] && !env.data->players[4].isParticipating);
  // An alliance formed later does not change the match-start enemy.
  self.allies = 0b100001;
  CHECK(env.project());
  CHECK(env.data->enemy == 3);
}

// Every playable slot allied with self: no enemy.
void enemy_none_when_all_allied() {
  Env env;
  env.fx.local_player = 4;
  env.fx.game_frame = 0;
  auto self = player_row(4, 1);
  self.player_type = BWAPI::PlayerTypes::Enum::Player;
  self.allies = 0b11111111;
  self.participating = 1;
  auto other = player_row(0, 0);
  other.player_type = BWAPI::PlayerTypes::Enum::Computer;
  other.participating = 1;
  env.fx.players.rows = {self, other};
  CHECK(env.project());
  CHECK(env.data->enemy == -1);
}

// GameEvents.cpp participation is decided once per match from the frame-0
// column; a later column value does not change it.
void participation_is_decided_on_frame_zero() {
  Env env;
  env.fx.local_player = 0;
  env.fx.game_frame = 0;
  auto self = player_row(0, 1);
  self.player_type = BWAPI::PlayerTypes::Enum::Player;
  self.allies = 0b0001;
  self.participating = 1;
  auto observer = player_row(1, 0);
  observer.player_type = BWAPI::PlayerTypes::Enum::Player;
  observer.allies = 0b0010;
  observer.participating = 0;
  env.fx.players.rows = {self, observer};
  CHECK(env.project());
  const auto& d = *env.data;
  CHECK(d.players[0].isParticipating && !d.players[1].isParticipating);
  CHECK(!d.players[0].isEnemy[1] && !d.players[1].isEnemy[0] && d.enemy == -1);
  env.fx.game_frame = 1;
  env.fx.players.rows[1].participating = UINT8_MAX;
  CHECK(env.project());
  CHECK(!env.data->players[1].isParticipating);
}

// Without frame 0 a playable slot participates once it is seen owning or
// having lost a unit, and keeps participating for the match.
void participation_without_frame_zero() {
  Env env;
  env.fx.local_player = 0;
  auto self = player_row(0, 1);
  self.player_type = BWAPI::PlayerTypes::Enum::Player;
  self.allies = 0b0001;
  auto foe = player_row(1, 0);
  foe.player_type = BWAPI::PlayerTypes::Enum::Computer;
  foe.allies = 0b0010;
  auto observer = player_row(2, 2);
  observer.player_type = BWAPI::PlayerTypes::Enum::Player;
  env.fx.players.rows = {self, foe, observer};
  Fixture::UnitCountsRows::Row own;
  own.player = 0;
  own.unit_type = UT::Terran_SCV;
  own.all = 4;
  Fixture::UnitCountsRows::Row lost;
  lost.player = 1;
  lost.unit_type = UT::Zerg_Drone;
  lost.all = 0;
  lost.dead = 4;
  env.fx.unit_counts.rows = {own, lost};
  CHECK(env.project());
  const auto& d = *env.data;
  CHECK(d.players[0].isParticipating && d.players[1].isParticipating && !d.players[2].isParticipating);
  CHECK(d.players[0].isEnemy[1] && d.players[1].isEnemy[0] && !d.players[0].isEnemy[2]);
  CHECK(d.enemy == 1);
  env.fx.unit_counts.rows.clear();
  CHECK(env.project());
  CHECK(env.data->players[1].isParticipating && env.data->players[0].isEnemy[1]);
}

void unknown_self_in_replay() {
  Env env;
  env.fx.flags = BWR_FRAME_REPLAY;
  env.fx.players.rows = {player_row(0, 0), player_row(1, 1)};
  CHECK(env.project());
  CHECK(env.data->self == -1);
  CHECK(env.data->isReplay && env.data->enemy == -1);
}

// GameUnits.cpp computeSecondaryUnitSets: all and completed counts are the
// published units; killed and dead counts are the game's counters.
void unit_counts() {
  Env env;
  env.fx.players.rows = {player_row(0, 0), player_row(11, 4)};
  auto incomplete = unit_row(3, UT::Zerg_Zergling, 0, 30, 30);
  incomplete.completed = 0;
  env.fx.units.rows = {unit_row(1, UT::Zerg_Zergling, 0, 10, 10), unit_row(2, UT::Zerg_Zergling, 0, 20, 20),
                       incomplete, unit_row(4, UT::Zerg_Hatchery, 0, 40, 40),
                       unit_row(5, UT::Resource_Mineral_Field_Type_2, 11, 50, 50)};
  Fixture::UnitCountsRows::Row zerglings;
  zerglings.player = 0;
  zerglings.unit_type = UT::Zerg_Zergling;
  zerglings.all = 8;
  zerglings.completed = 6;
  zerglings.killed = 3;
  zerglings.dead = UINT32_MAX;
  env.fx.unit_counts.rows = {zerglings};
  CHECK(env.project());
  const auto& p = env.data->players[0];
  CHECK(p.allUnitCount[UT::Zerg_Zergling] == 3 && p.completedUnitCount[UT::Zerg_Zergling] == 2);
  CHECK(p.allUnitCount[UT::Men] == 3 && p.allUnitCount[UT::Buildings] == 1 &&
        p.allUnitCount[UT::Factories] == 1 && p.allUnitCount[UT::AllUnits] == 4);
  CHECK(p.completedUnitCount[UT::AllUnits] == 3);
  CHECK(p.killedUnitCount[UT::Zerg_Zergling] == 3);
  // A type without a unit has every count zero.
  CHECK(p.allUnitCount[UT::Zerg_Hydralisk] == 0);
  // Every mineral field variant counts as Resource_Mineral_Field.
  const auto& neutral = env.data->players[1];
  CHECK(neutral.allUnitCount[UT::Resource_Mineral_Field] == 1 &&
        neutral.allUnitCount[UT::Resource_Mineral_Field_Type_2] == 0);
}

// PlayerImpl::updateData: an enemy of the BWAPI player shows no resources,
// techs, availability, supply, scores or dead and killed counts outside
// CompleteMapInformation, and only the upgrade levels a completed unit of
// the last frame's counts uses. Its start location stays Unknown.
void enemy_data_is_hidden() {
  Env env;
  namespace UP = BWAPI::UpgradeTypes::Enum;
  env.fx.local_player = 0;
  env.fx.game_frame = 0;
  auto self = player_row(0, 1);
  self.player_type = BWAPI::PlayerTypes::Enum::Player;
  self.participating = 1;
  self.allies = 0b1;
  auto foe = player_row(1, 1);
  foe.player_type = BWAPI::PlayerTypes::Enum::Computer;
  foe.participating = 1;
  foe.allies = 0b10;
  foe.minerals = 75;
  foe.gas = 20;
  foe.gathered_minerals = 300;
  foe.supply_used_terran = 10;
  foe.supply_total_terran = 20;
  foe.unit_score = 50;
  foe.start_x = 7;
  foe.start_y = 8;
  env.fx.players.rows = {self, foe};
  // A completed enemy Marine the BWAPI player sees.
  env.fx.units.rows = {unit_row(1, UT::Terran_Marine, 1, 10, 10)};
  env.fx.render.rows = {{unit_id(1), 10, 10, 0b1}};
  Fixture::UpgradesRows::Row weapons;
  weapons.player = 1;
  weapons.upgrade_type = UP::Terran_Infantry_Weapons;
  weapons.level = 2;
  weapons.max_level = 3;
  weapons.in_progress = 1;
  Fixture::UpgradesRows::Row plating;
  plating.player = 1;
  plating.upgrade_type = UP::Terran_Vehicle_Plating;
  plating.level = 1;
  env.fx.upgrades.rows = {weapons, plating};
  Fixture::TechsRows::Row stim;
  stim.player = 1;
  stim.tech_type = BWAPI::TechTypes::Enum::Stim_Packs;
  stim.researched = 1;
  stim.available = 1;
  env.fx.techs.rows = {stim};
  Fixture::UnitAvailabilityRows::Row marine;
  marine.player = 1;
  marine.unit_type = UT::Terran_Marine;
  marine.available = 1;
  env.fx.unit_availability.rows = {marine};
  Fixture::UnitCountsRows::Row lost;
  lost.player = 1;
  lost.unit_type = UT::Terran_Marine;
  lost.dead = 2;
  env.fx.unit_counts.rows = {lost};
  CHECK(env.project());
  const auto& e = env.data->players[1];
  CHECK(e.minerals == 0 && e.gas == 0 && e.gatheredMinerals == 0);
  CHECK(e.supplyUsed[1] == 0 && e.supplyTotal[1] == 0 && e.totalUnitScore == 0);
  CHECK(e.deadUnitCount[UT::Terran_Marine] == 0);
  CHECK(!e.hasResearched[BWAPI::TechTypes::Enum::Stim_Packs] &&
        !e.hasResearched[BWAPI::TechTypes::Enum::Scanner_Sweep] &&
        !e.isResearchAvailable[BWAPI::TechTypes::Enum::Stim_Packs]);
  CHECK(!e.isUnitAvailable[UT::Terran_Marine] && e.maxUpgradeLevel[UP::Terran_Infantry_Weapons] == 0 &&
        !e.isUpgrading[UP::Terran_Infantry_Weapons]);
  CHECK(e.startLocationX == BWAPI::TilePositions::Unknown.x &&
        e.startLocationY == BWAPI::TilePositions::Unknown.y);
  // The unit counts the upgrade check reads are the previous frame's.
  CHECK(e.upgradeLevel[UP::Terran_Infantry_Weapons] == 0);
  CHECK(e.allUnitCount[UT::Terran_Marine] == 1 && e.completedUnitCount[UT::Terran_Marine] == 1);
  env.fx.game_frame = 1;
  CHECK(env.project());
  CHECK(env.data->players[1].upgradeLevel[UP::Terran_Infantry_Weapons] == 2);
  CHECK(env.data->players[1].upgradeLevel[UP::Terran_Vehicle_Plating] == 0);
  // CompleteMapInformation shows everything but the match-start location.
  env.data->flags[BWAPI::Flag::CompleteMapInformation] = true;
  env.fx.game_frame = 2;
  CHECK(env.project());
  const auto& full = env.data->players[1];
  CHECK(full.minerals == 75 && full.gas == 20 && full.supplyUsed[1] == 10 && full.totalUnitScore == 50);
  CHECK(full.hasResearched[BWAPI::TechTypes::Enum::Stim_Packs] && full.isUnitAvailable[UT::Terran_Marine]);
  CHECK(full.upgradeLevel[UP::Terran_Vehicle_Plating] == 1 && full.deadUnitCount[UT::Terran_Marine] == 2);
  CHECK(full.startLocationX == BWAPI::TilePositions::Unknown.x);
}

void upgrades_techs_availability() {
  Env env;
  env.fx.players.rows = {player_row(0, 1)};
  Fixture::UpgradesRows::Row upgrade;
  upgrade.player = 0;
  upgrade.upgrade_type = BWAPI::UpgradeTypes::Enum::Terran_Infantry_Armor;
  upgrade.level = 2;
  upgrade.max_level = 3;
  upgrade.in_progress = 1;
  env.fx.upgrades.rows = {upgrade};
  Fixture::TechsRows::Row stim;
  stim.player = 0;
  stim.tech_type = BWAPI::TechTypes::Enum::Stim_Packs;
  stim.researched = 0;
  stim.available = 1;
  stim.in_progress = 1;
  Fixture::TechsRows::Row sweep;
  sweep.player = 0;
  sweep.tech_type = BWAPI::TechTypes::Enum::Scanner_Sweep;
  sweep.researched = 0;
  env.fx.techs.rows = {stim, sweep};
  Fixture::UnitAvailabilityRows::Row missile;
  missile.player = 0;
  missile.unit_type = UT::Terran_Nuclear_Missile;
  missile.available = 1;
  env.fx.unit_availability.rows = {missile};
  CHECK(env.project());
  const auto& p = env.data->players[0];
  CHECK(p.upgradeLevel[BWAPI::UpgradeTypes::Enum::Terran_Infantry_Armor] == 2);
  CHECK(p.maxUpgradeLevel[BWAPI::UpgradeTypes::Enum::Terran_Infantry_Armor] == 3);
  CHECK(p.isUpgrading[BWAPI::UpgradeTypes::Enum::Terran_Infantry_Armor]);
  CHECK(!p.hasResearched[BWAPI::TechTypes::Enum::Stim_Packs] && p.isResearching[BWAPI::TechTypes::Enum::Stim_Packs]);
  CHECK(p.isResearchAvailable[BWAPI::TechTypes::Enum::Stim_Packs]);
  // Scanner Sweep needs no research.
  CHECK(p.hasResearched[BWAPI::TechTypes::Enum::Scanner_Sweep]);
  CHECK(p.isUnitAvailable[UT::Terran_Nuclear_Missile]);
  CHECK(p.hasResearched[BWAPI::TechTypes::Enum::Nuclear_Strike]);
}

void game_scalars() {
  Env env;
  env.fx.game_frame = 4242;
  env.fx.flags = BWR_FRAME_REPLAY;
  Fixture::GameRows::Row game;
  game.game_type = 15;
  game.multiplayer = 0;
  game.paused = 1;
  game.latency_frames = 2;  // turn delay; BWAPI adds the send frame
  game.frame_ms = 42;
  game.elapsed_seconds = 180;
  game.random_seed = 99;
  game.replay_frames = 5000;
  game.countdown_seconds = 7;
  set_text(game.map_file, "Python.scx");
  set_text(game.map_path, "maps\\Python.scx");
  env.fx.game.rows = {game};
  Fixture::ViewportRows::Row viewport;
  viewport.x = 320;
  viewport.y = 640;
  env.fx.viewport.rows = {viewport};
  Fixture::StartLocationsRows::Row start;
  start.x = 10;
  start.y = 20;
  env.fx.start_locations.rows = {start, start};
  Fixture::ForcesRows::Row force;
  force.id = 2;
  set_text(force.name, "Team Two");
  env.fx.forces.rows = {force};
  CHECK(env.project());
  const auto& d = *env.data;
  CHECK(d.isInGame && d.frameCount == 4242 && d.isReplay && d.isPaused);
  CHECK(d.gameType == 15 && !d.isMultiplayer);
  // The table carries no Battle.net state.
  CHECK(!d.isBattleNet);
  CHECK(d.latency == BWAPI::Latency::SinglePlayer && d.latencyFrames == 3 && d.latencyTime == 126);
  CHECK(d.elapsedTime == 180 && d.randomSeed == 99 && d.replayFrameCount == 5000);
  CHECK(d.countdownTimer == 7);
  CHECK(std::string(d.mapFileName) == "Python.scx" && std::string(d.mapPathName) == "maps\\Python.scx");
  // No such map file: the hash stays empty.
  CHECK(d.mapHash[0] == '\0');
  CHECK(d.screenX == 320 && d.screenY == 640);
  CHECK(d.startLocationCount == 2 && d.startLocations[1].y == 20);
  CHECK(d.forceCount == 5 && std::string(d.forces[2].name) == "Team Two" && d.forces[1].name[0] == 0);
  // Values this fixture does not feed stay at their defaults.
  CHECK(d.initialUnitCount == 0);
  CHECK(d.botAPM_selects == 0);
  CHECK(d.remainingLatencyFrames == 0);
  CHECK(d.nukeDotCount == 0);
  // Without user input the cursor reads as GameImpl's Positions::Unknown.
  CHECK(d.mouseX == BWAPI::Positions::Unknown.x && d.mouseY == BWAPI::Positions::Unknown.y);
}

// Map::calculateMapHash: the SHA-1 hex of the map file, once per match.
void map_hash_of_the_map_file() {
  const auto path = std::filesystem::temp_directory_path() / "bwrapi-map-hash-test.scx";
  {
    std::ofstream file(path, std::ios::binary);
    file << "abc";
  }
  Env env;
  Fixture::GameRows::Row game;
  const std::string utf8 = path.u8string();
  set_text(game.map_path, utf8.c_str());
  env.fx.game.rows = {game};
  CHECK(env.project());
  CHECK(std::string(env.data->mapHash) == "a9993e364706816aba3e25717850c26c9cd0d89d");
  // The hash is kept for the match even when the file changes.
  {
    std::ofstream file(path, std::ios::binary);
    file << "abcd";
  }
  CHECK(env.project());
  CHECK(std::string(env.data->mapHash) == "a9993e364706816aba3e25717850c26c9cd0d89d");
  // A new match reads the file again.
  env.fx.match_id = 2;
  CHECK(env.project());
  CHECK(std::string(env.data->mapHash) == "81fe8bfe87576c3ecb22426f8e57847382917acf");
  std::filesystem::remove(path);
}

// GameImpl::getRemainingLatencyFrames/Time: latency less the frames and the
// milliseconds since the last sent turn, where a turn sent before a frame
// (or while it stands) carries that frame's count and its send carries the
// watcher's QPC stamp.
void remaining_latency_follows_sent_turns() {
  Env env;
  LARGE_INTEGER now;
  QueryPerformanceCounter(&now);
  Fixture::GameRows::Row game;
  game.latency_frames = 2;  // turn delay; BWAPI adds the send frame
  game.frame_ms = 42;
  game.turns_sent = 10;
  game.last_turn_qpc = static_cast<std::uint64_t>(now.QuadPart);
  env.fx.game_frame = 100;
  env.fx.game.rows = {game};
  CHECK(env.project());
  CHECK(env.data->remainingLatencyFrames == 3);
  CHECK(env.data->remainingLatencyTime <= 126 && env.data->remainingLatencyTime > 126 - 1000);
  // No turn for two frames.
  env.fx.game_frame = 102;
  CHECK(env.project());
  CHECK(env.data->remainingLatencyFrames == 1);
  env.fx.game_frame = 103;
  env.fx.game.rows[0].turns_sent = 11;
  CHECK(env.project());
  CHECK(env.data->remainingLatencyFrames == 3);
  // Paused: the frame stands and turns still go out on it.
  env.fx.game.rows[0].turns_sent = 12;
  env.projection.project_latency(env.fx.build(), *env.data);
  CHECK(env.data->remainingLatencyFrames == 3);
}

void unknown_game_values() {
  Env env;
  Fixture::GameRows::Row game;  // every column unknown
  env.fx.game.rows = {game};
  CHECK(env.project());
}

}  // namespace

int main() {
  player_values();
  only_the_playable_slots_have_a_victory_state();
  player_ids_follow_the_start_set();
  one_versus_one_skips_empty_slots();
  enemy_is_the_last_unallied_opponent();
  enemy_none_when_all_allied();
  participation_is_decided_on_frame_zero();
  participation_without_frame_zero();
  unknown_self_in_replay();
  unit_counts();
  enemy_data_is_hidden();
  upgrades_techs_availability();
  game_scalars();
  remaining_latency_follows_sent_turns();
  map_hash_of_the_map_file();
  unknown_game_values();
  return finish();
}
