// Latency compensation: accepted commands show on the projected frames until
// the game acts on them, then stop.
#include "legacy44_latcom.h"

#include <BWAPI/Order.h>
#include <BWAPI/Race.h>
#include <BWAPI/UnitCommandType.h>
#include <BWAPI/UnitType.h>

#include <iostream>
#include <memory>
#include <vector>

using namespace BWRAPI::Legacy44;

namespace {

int failures = 0;

#define CHECK(condition)                                                \
  do {                                                                  \
    if (!(condition)) {                                                 \
      std::cerr << __FILE__ << ':' << __LINE__ << ": check failed: "    \
                << #condition << '\n';                                  \
      ++failures;                                                       \
    }                                                                   \
  } while (false)

namespace O = BWAPI::Orders::Enum;
namespace U = BWAPI::UnitTypes::Enum;

constexpr int kUnit = 5;
constexpr int kTarget = 7;
constexpr int kTerran = BWAPI::Races::Enum::Terran;

// What the projection writes each frame: unit 5 of player 0 standing idle,
// unit 7 beside it, and player 0's stock.
void project(BWAPI::GameData& data, int unit_type, int rlf, int minerals = 500, int gas = 200,
             int supply = 10) {
  data.hasLatCom = true;
  data.remainingLatencyFrames = rlf;
  auto& unit = data.units[kUnit];
  unit = {};
  unit.id = kUnit;
  unit.exists = true;
  unit.player = 0;
  unit.type = unit_type;
  unit.order = O::PlayerGuard;
  unit.secondaryOrder = O::Nothing;
  unit.target = unit.orderTarget = unit.buildUnit = unit.rallyUnit = -1;
  unit.buildType = U::None;
  unit.isIdle = true;
  unit.isCompleted = true;
  unit.hitPoints = 40;
  auto& mineral = data.units[kTarget];
  mineral = {};
  mineral.id = kTarget;
  mineral.exists = true;
  mineral.player = 11;
  mineral.type = U::Resource_Mineral_Field;
  mineral.isCompleted = true;
  auto& player = data.players[0];
  player.minerals = minerals;
  player.gas = gas;
  player.supplyUsed[kTerran] = supply;
}

BWAPIC::UnitCommand command(BWAPI::UnitCommandType type, int target = -1, int extra = 0) {
  return BWAPIC::UnitCommand{type, kUnit, target, 0, 0, extra};
}

// The command is accepted on frame 10 with two latency frames left. The game
// acts on it on frame 12; frame 11 shows it from the buffer, frame 12 from
// the game itself.
void gather_shows_until_game_acts() {
  auto data = std::make_unique<BWAPI::GameData>();
  LatencyCompensation latcom;
  project(*data, U::Zerg_Drone, 2);
  latcom.apply(*data, 10);
  latcom.record({command(BWAPI::UnitCommandTypes::Gather, kTarget)}, *data);

  project(*data, U::Zerg_Drone, 1);
  latcom.apply(*data, 11);
  const auto& drone = data->units[kUnit];
  CHECK(drone.target == kTarget && drone.order == O::MoveToMinerals);
  CHECK(drone.isGathering && drone.isMoving && !drone.isIdle);

  project(*data, U::Zerg_Drone, 0);
  latcom.apply(*data, 12);
  CHECK(drone.target == -1 && drone.order == O::PlayerGuard && drone.isIdle);

  project(*data, U::Zerg_Drone, 2);
  latcom.apply(*data, 13);
  CHECK(drone.target == -1 && drone.order == O::PlayerGuard);
}

// Frames an asynchronous client lets pass move the buffer on all the same.
void skipped_frames_count() {
  auto data = std::make_unique<BWAPI::GameData>();
  LatencyCompensation latcom;
  project(*data, U::Zerg_Drone, 3);
  latcom.apply(*data, 10);
  latcom.record({command(BWAPI::UnitCommandTypes::Move)}, *data);
  project(*data, U::Zerg_Drone, 1);
  latcom.apply(*data, 12);
  CHECK(data->units[kUnit].order == O::Move);
  project(*data, U::Zerg_Drone, 0);
  latcom.apply(*data, 13);
  CHECK(data->units[kUnit].order == O::PlayerGuard);
}

// Train shows its price and supply from the command on; on the frame the game
// takes the price only the supply, which the game takes a frame later, is
// still added.
void train_reserves_until_game_takes_it() {
  auto data = std::make_unique<BWAPI::GameData>();
  LatencyCompensation latcom;
  const BWAPI::UnitType marine = BWAPI::UnitTypes::Terran_Marine;
  project(*data, U::Terran_Barracks, 2);
  latcom.apply(*data, 10);
  latcom.record({command(BWAPI::UnitCommandTypes::Train, -1, U::Terran_Marine)}, *data);

  project(*data, U::Terran_Barracks, 1);
  latcom.apply(*data, 11);
  const auto& barracks = data->units[kUnit];
  const auto& player = data->players[0];
  CHECK(player.minerals == 500 - marine.mineralPrice());
  CHECK(player.supplyUsed[kTerran] == 10 + marine.supplyRequired());
  CHECK(barracks.trainingQueueCount == 1 && barracks.trainingQueue[0] == U::Terran_Marine);
  CHECK(barracks.isTraining && !barracks.isIdle && barracks.remainingTrainTime == marine.buildTime());

  // The game took the price; the projection shows it, not the supply.
  project(*data, U::Terran_Barracks, 0, 500 - marine.mineralPrice());
  latcom.apply(*data, 12);
  CHECK(player.minerals == 500 - marine.mineralPrice());
  CHECK(player.supplyUsed[kTerran] == 10 + marine.supplyRequired());
  CHECK(barracks.trainingQueueCount == 1);

  project(*data, U::Terran_Barracks, 2, 500 - marine.mineralPrice(), 200, 10 + marine.supplyRequired());
  latcom.apply(*data, 13);
  CHECK(player.supplyUsed[kTerran] == 10 + marine.supplyRequired());
}

// A building morph holds its price from the command on, and its order step
// shows while a new turn leaves latency frames before the game acts.
void morph_steps_follow_the_buffer() {
  auto data = std::make_unique<BWAPI::GameData>();
  LatencyCompensation latcom;
  const BWAPI::UnitType lair = BWAPI::UnitTypes::Zerg_Lair;
  project(*data, U::Zerg_Hatchery, 1);
  latcom.apply(*data, 10);
  latcom.record({command(BWAPI::UnitCommandTypes::Morph, -1, U::Zerg_Lair)}, *data);

  project(*data, U::Zerg_Hatchery, 2);
  latcom.apply(*data, 11);
  const auto& hatchery = data->units[kUnit];
  CHECK(data->players[0].minerals == 500 - lair.mineralPrice());
  CHECK(data->players[0].gas == 200 - lair.gasPrice());
  CHECK(hatchery.type == U::Zerg_Hatchery && hatchery.order == O::IncompleteBuilding);

  project(*data, U::Zerg_Hatchery, 1);
  latcom.apply(*data, 12);
  CHECK(data->players[0].minerals == 500 && hatchery.order == O::PlayerGuard);
}

// Nothing applies while the client has latency compensation off, to a queued
// command, or after a reset.
void off_queued_and_reset_do_nothing() {
  auto data = std::make_unique<BWAPI::GameData>();
  LatencyCompensation latcom;
  project(*data, U::Zerg_Drone, 2);
  latcom.apply(*data, 10);
  latcom.record({command(BWAPI::UnitCommandTypes::Gather, kTarget)}, *data);
  project(*data, U::Zerg_Drone, 1);
  data->hasLatCom = false;
  latcom.apply(*data, 11);
  CHECK(data->units[kUnit].target == -1 && data->units[kUnit].order == O::PlayerGuard);

  project(*data, U::Zerg_Drone, 2);
  latcom.apply(*data, 12);
  latcom.record({command(BWAPI::UnitCommandTypes::Gather, kTarget, 1)}, *data);
  project(*data, U::Zerg_Drone, 1);
  latcom.apply(*data, 13);
  CHECK(data->units[kUnit].target == -1);

  project(*data, U::Zerg_Drone, 2);
  latcom.apply(*data, 14);
  latcom.record({command(BWAPI::UnitCommandTypes::Gather, kTarget)}, *data);
  latcom.reset();
  project(*data, U::Zerg_Drone, 1);
  latcom.apply(*data, 15);
  CHECK(data->units[kUnit].target == -1);
}

}  // namespace

int main() {
  gather_shows_until_game_acts();
  skipped_frames_count();
  train_reserves_until_game_takes_it();
  morph_steps_follow_the_buffer();
  off_queued_and_reset_do_nothing();
  if (failures != 0) std::cerr << failures << " check(s) failed\n";
  return failures == 0 ? 0 : 1;
}
