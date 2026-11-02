#include "legacy44_latcom.h"

#include <BWAPI/Order.h>
#include <BWAPI/Race.h>
#include <BWAPI/TechType.h>
#include <BWAPI/UnitCommandType.h>
#include <BWAPI/UnitType.h>
#include <BWAPI/UpgradeType.h>

#include <algorithm>
#include <iterator>

namespace BWRAPI::Legacy44 {

namespace {

namespace C = BWAPI::UnitCommandTypes::Enum;
namespace O = BWAPI::Orders::Enum;
namespace U = BWAPI::UnitTypes::Enum;

// BWAPI::UnitCommand::isQueued: the commands whose `extra` is the shift-queue
// flag.
bool is_queued(const BWAPIC::UnitCommand& command) {
  switch (command.type.getID()) {
    case C::Attack_Move:
    case C::Attack_Unit:
    case C::Move:
    case C::Patrol:
    case C::Hold_Position:
    case C::Stop:
    case C::Follow:
    case C::Gather:
    case C::Return_Cargo:
    case C::Repair:
    case C::Load:
    case C::Unload_All:
    case C::Unload_All_Position:
    case C::Right_Click_Position:
    case C::Right_Click_Unit:
      return command.extra != 0;
    default:
      return false;
  }
}

// Game::getUnit: null for an ID outside the unit table.
BWAPI::UnitData* unit_at(BWAPI::GameData& data, int id) {
  return id >= 0 && id < static_cast<int>(std::size(data.units)) ? &data.units[id] : nullptr;
}

BWAPI::PlayerData* player_at(BWAPI::GameData& data, int id) {
  return id >= 0 && id < static_cast<int>(std::size(data.players)) ? &data.players[id] : nullptr;
}

// Supply is kept for the three playable races only.
void add_supply(BWAPI::PlayerData& player, BWAPI::Race race, int amount) {
  const int index = race.getID();
  if (index >= 0 && index < static_cast<int>(std::size(player.supplyUsed))) player.supplyUsed[index] += amount;
}

// Player::getUpgradeLevel of the unit's owner.
int upgrade_level(BWAPI::GameData& data, const BWAPI::UnitData& unit, BWAPI::UpgradeType upgrade) {
  const BWAPI::PlayerData* owner = player_at(data, unit.player);
  return owner != nullptr && upgrade.isValid() ? owner->upgradeLevel[upgrade.getID()] : 0;
}

void set_target_position(BWAPI::UnitData& unit, int x, int y) {
  unit.targetPositionX = x;
  unit.targetPositionY = y;
  unit.orderTargetPositionX = x;
  unit.orderTargetPositionY = y;
}

}  // namespace

void LatencyCompensation::reset() {
  for (auto& frame : slots_) frame.clear();
  head_ = 0;
  pending_ = 0;
  have_frame_ = false;
  frame_ = 0;
}

void LatencyCompensation::grow(std::size_t size) {
  std::rotate(slots_.begin(), slots_.begin() + static_cast<std::ptrdiff_t>(head_), slots_.end());
  head_ = 0;
  slots_.resize(size);
}

void LatencyCompensation::insert(const Entry& entry, int frames) {
  if (frames <= 0) return;
  if (slots_.size() < static_cast<std::size_t>(frames)) grow(static_cast<std::size_t>(frames));
  slot(static_cast<std::size_t>(frames - 1)).push_back(entry);
  ++pending_;
}

// GameImpl::addToCommandBuffer: each command applies until the game acts on
// it, the later steps of the multi-step ones on the frames CommandTemp.h names
// (RLF is the remaining latency frames). The effect on the command's own frame
// is already the client's, so it is not repeated here.
void LatencyCompensation::record(const std::vector<BWAPIC::UnitCommand>& commands,
                                 const BWAPI::GameData& data) {
  const int rlf = data.remainingLatencyFrames;
  for (const auto& command : commands) {
    // A queued command never applies.
    if (is_queued(command)) continue;
    if (command.unitIndex < 0 || command.unitIndex >= static_cast<int>(std::size(data.units))) continue;
    const BWAPI::UnitData& unit = data.units[command.unitIndex];
    Entry entry{command, Step::Resource, unit.player};
    const auto step = [&](Step step) {
      Entry other = entry;
      other.step = step;
      return other;
    };
    switch (command.type.getID()) {
      case C::Cancel_Construction:
        insert(entry, rlf);
        insert(step(Step::Order), rlf + 1);
        insert(step(Step::Finish), rlf + 2);
        break;
      case C::Build_Addon:
      case C::Cancel_Addon:
      case C::Cancel_Research:
      case C::Cancel_Upgrade:
      case C::Morph:
        insert(entry, rlf);
        insert(step(Step::Order), rlf + 1);
        break;
      case C::Cancel_Morph:
        insert(entry, rlf);
        // A building returns to its old type through an order step; a unit
        // leaves its egg or cocoon later.
        if (BWAPI::UnitType(unit.type).isBuilding()) {
          insert(step(Step::Order), rlf + 1);
          insert(step(Step::Finish), rlf + 15);
        } else {
          insert(step(Step::Finish), rlf + 13);
        }
        break;
      case C::Cancel_Train:
      case C::Cancel_Train_Slot:
        insert(entry, rlf);
        insert(step(Step::Order), rlf + 1);
        insert(step(Step::Finish), rlf + 3);
        break;
      case C::Halt_Construction:
        insert(step(Step::Order), rlf);
        insert(step(Step::Finish), rlf + 1);
        break;
      default:
        insert(entry, rlf);
        break;
    }
  }
}

// GameImpl::applyLatencyCompensation, once per projected frame.
void LatencyCompensation::apply(BWAPI::GameData& data, int frame) {
  // Frames an asynchronous client let pass without a projection still count:
  // the game acted on their commands without this frame's GameData seeing it.
  int steps = have_frame_ ? frame - frame_ : 1;
  if (steps < 1) steps = 1;
  have_frame_ = true;
  frame_ = frame;
  if (pending_ != 0) {
    const std::size_t pops = std::min(static_cast<std::size_t>(steps), slots_.size());
    for (std::size_t pop = 0; pop < pops; ++pop) {
      auto& front = slot(0);
      // The frame the game acts on: only the price and supply still held
      // apply to it.
      if (pop + 1 == static_cast<std::size_t>(steps))
        for (auto& entry : front) execute(entry, true, data);
      pending_ -= front.size();
      front.clear();
      head_ = (head_ + 1) % slots_.size();
    }
  }
  const int rlf = data.remainingLatencyFrames;
  if (pending_ != 0) {
    for (std::size_t offset = 0; static_cast<int>(offset) < rlf && offset < slots_.size(); ++offset) {
      for (auto& entry : slot(offset)) {
        // Game::getRemainingLatencyFrames is above zero here, so this is
        // CommandTemp::execute() on a frame before the game acts; it turns
        // every Halt_Construction step into its order step.
        if (entry.command.type.getID() == C::Halt_Construction) entry.step = Step::Order;
        execute(entry, false, data);
      }
    }
  }
  // Room for the latest step any new command adds.
  if (rlf + 15 > 0 && slots_.size() < static_cast<std::size_t>(rlf + 15))
    grow(static_cast<std::size_t>(rlf + 15));
}

// CommandTemp::execute(isCurrentFrame) on the projected GameData.
void LatencyCompensation::execute(Entry& entry, bool current_frame, BWAPI::GameData& data) const {
  if (!data.hasLatCom) return;
  const BWAPIC::UnitCommand& command = entry.command;
  const int type = command.type.getID();
  // Train, Morph and Build_Addon hold the price or supply that the game only
  // takes on a later frame, so a bot does not spend it twice.
  if (current_frame &&
      (entry.step != Step::Resource || (type != C::Train && type != C::Morph && type != C::Build_Addon)))
    return;
  BWAPI::UnitData* unit = unit_at(data, command.unitIndex);
  BWAPI::PlayerData* player = player_at(data, entry.player);
  if (unit == nullptr || player == nullptr || !unit->exists) return;
  BWAPI::UnitData* target = unit_at(data, command.targetIndex);
  const int target_id = target != nullptr ? command.targetIndex : -1;
  // The unit's type as the steps before have left it.
  const auto unit_type = [unit] { return BWAPI::UnitType(unit->type); };

  switch (type) {
    case C::Follow:
    case C::Hold_Position:
    case C::Move:
    case C::Patrol:
    case C::Right_Click_Position:
    case C::Attack_Move:
      if (!unit_type().canMove()) return;
      break;
    default:
      break;
  }

  switch (type) {
    case C::Attack_Move:
      unit->order = O::AttackMove;
      set_target_position(*unit, command.x, command.y);
      break;

    case C::Attack_Unit:
      if (target == nullptr || !target->exists || !unit_type().canAttack()) return;
      unit->order = O::AttackUnit;
      unit->target = target_id;
      break;

    case C::Build:
      unit->order = O::PlaceBuilding;
      unit->isConstructing = true;
      unit->isIdle = false;
      unit->buildType = command.extra;
      break;

    // The game takes an addon's price one frame after the command; it is held
    // from the command on, as for a building morph.
    case C::Build_Addon: {
      const BWAPI::UnitType addon(command.extra);
      if (entry.step == Step::Resource) {
        player->minerals -= addon.mineralPrice();
        player->gas -= addon.gasPrice();
        if (!current_frame) {
          unit->isIdle = false;
          unit->order = O::PlaceAddon;
        }
      } else if (entry.step == Step::Order) {
        unit->isConstructing = true;
        unit->order = O::Nothing;
        unit->secondaryOrder = O::BuildAddon;
        unit->buildType = command.extra;
      }
      break;
    }

    case C::Burrow:
      unit->order = O::Burrowing;
      break;

    case C::Cancel_Addon:
      if (entry.step == Step::Resource) {
        const BWAPI::UnitType addon(unit->buildType);
        player->minerals += static_cast<int>(addon.mineralPrice() * 0.75);
        player->gas += static_cast<int>(addon.gasPrice() * 0.75);
        unit->buildType = U::None;
      } else if (entry.step == Step::Order) {
        unit->remainingBuildTime = 0;
        unit->isConstructing = false;
        unit->order = O::Nothing;
        unit->isIdle = true;
        unit->buildUnit = -1;
      }
      break;

    case C::Cancel_Construction: {
      if (unit_type().getRace() == BWAPI::Races::Terran) {
        BWAPI::UnitData* builder = unit_at(data, unit->buildUnit);
        if (builder != nullptr && builder->exists) {
          switch (entry.step) {
            case Step::Resource:
              builder->buildType = U::None;
              break;
            case Step::Order:
              builder->isConstructing = false;
              builder->order = O::ResetCollision;
              break;
            case Step::Finish:
              builder->order = O::PlayerGuard;
              break;
          }
        }
      }
      if (entry.step == Step::Resource) {
        unit->buildUnit = -1;
        player->minerals += static_cast<int>(unit_type().mineralPrice() * 0.75);
        player->gas += static_cast<int>(unit_type().gasPrice() * 0.75);
        unit->remainingBuildTime = 0;
      }
      if (unit_type().getRace() == BWAPI::Races::Zerg) {
        if (entry.step == Step::Resource) {
          // The drone comes back and takes its supply again.
          unit->type = unit_type().whatBuilds().first.getID();
          unit->buildType = U::None;
          unit->isMorphing = false;
          unit->order = O::ResetCollision;
          unit->isConstructing = false;
          add_supply(*player, unit_type().getRace(), unit_type().supplyRequired());
        } else if (entry.step == Step::Order) {
          unit->order = O::PlayerGuard;
          unit->isIdle = true;
        }
      }
      break;
    }

    case C::Cancel_Morph:
      switch (entry.step) {
        case Step::Resource: {
          const BWAPI::UnitType built(unit->buildType);
          const BWAPI::UnitType previous = built.whatBuilds().first;
          // A building morph refunds three quarters, a unit morph all of it.
          if (previous.isBuilding()) {
            player->minerals += static_cast<int>(built.mineralPrice() * 0.75);
            player->gas += static_cast<int>(built.gasPrice() * 0.75);
          } else {
            player->minerals += built.mineralPrice();
            player->gas += built.gasPrice();
          }
          if (previous.isBuilding() && previous.producesCreep()) unit->order = O::InitCreepGrowth;
          // An egg keeps its type (BWAPI issue 781).
          if (unit->type != U::Zerg_Egg) unit->type = previous.getID();
          unit->buildType = U::None;
          unit->isConstructing = false;
          unit->isMorphing = false;
          unit->isCompleted = true;
          unit->remainingBuildTime = 0;
          break;
        }
        case Step::Order:
          if (unit_type().isBuilding()) {
            unit->isIdle = true;
            unit->order = O::Nothing;
            if (unit->type == U::Zerg_Hatchery || unit->type == U::Zerg_Lair)
              unit->secondaryOrder = O::SpreadCreep;
          } else {
            const BWAPI::UnitType morph = unit_type();
            const auto source = morph.whatBuilds();
            add_supply(*player, morph.getRace(),
                       source.first.supplyRequired() * source.second -
                           morph.supplyRequired() * (1 + static_cast<int>(morph.isTwoUnitsInOneEgg())));
          }
          break;
        case Step::Finish:
          if (unit->type == U::Zerg_Hatchery || unit->type == U::Zerg_Lair) {
            unit->secondaryOrder = O::SpawningLarva;
          } else if (!unit_type().isBuilding()) {
            unit->order = O::PlayerGuard;
            unit->isCompleted = true;
            unit->isConstructing = false;
            unit->isIdle = true;
            unit->isMorphing = false;
          }
          break;
      }
      break;

    case C::Cancel_Research:
      if (entry.step == Step::Resource) {
        const BWAPI::TechType tech(unit->tech);
        player->minerals += tech.mineralPrice();
        player->gas += tech.gasPrice();
        unit->remainingResearchTime = 0;
        unit->tech = BWAPI::TechTypes::Enum::None;
      } else if (entry.step == Step::Order) {
        unit->order = O::Nothing;
        unit->isIdle = true;
      }
      break;

    case C::Cancel_Train_Slot:
      // A later slot only leaves the queue; slot 0 is Cancel_Train.
      if (command.extra != 0) {
        const int slot = command.extra;
        if (entry.step == Step::Resource && slot > 0 &&
            slot < static_cast<int>(std::size(unit->trainingQueue)) && unit->trainingQueueCount > 0) {
          const BWAPI::UnitType trained(unit->trainingQueue[slot]);
          player->minerals += trained.mineralPrice();
          player->gas += trained.gasPrice();
          for (int i = slot; i + 1 < static_cast<int>(std::size(unit->trainingQueue)); ++i)
            unit->trainingQueue[i] = unit->trainingQueue[i + 1];
          --unit->trainingQueueCount;
        }
        break;
      }
      [[fallthrough]];

    case C::Cancel_Train: {
      const int count = unit->trainingQueueCount;
      // The resource and order steps take the last queued unit.
      if (entry.step != Step::Finish && (count < 1 || count > static_cast<int>(std::size(unit->trainingQueue))))
        break;
      switch (entry.step) {
        case Step::Resource: {
          const BWAPI::UnitType trained(unit->trainingQueue[count - 1]);
          player->minerals += trained.mineralPrice();
          player->gas += trained.gasPrice();
          unit->buildUnit = -1;
          if (count == 1) {
            unit->isIdle = false;
            unit->isTraining = false;
          }
          break;
        }
        case Step::Order: {
          const BWAPI::UnitType trained(unit->trainingQueue[count - 1]);
          unit->trainingQueueCount = count - 1;
          add_supply(*player, trained.getRace(), -trained.supplyRequired());
          if (count == 1) {
            unit->buildType = U::None;
          } else {
            // The game counts the next unit's time down already; its full
            // time stands in until the latency is over.
            const BWAPI::UnitType next(unit->trainingQueue[count - 2]);
            unit->remainingTrainTime = next.buildTime();
            unit->buildType = next.getID();
          }
          break;
        }
        case Step::Finish:
          if (unit->buildType == U::None) unit->order = O::Nothing;
          break;
      }
      break;
    }

    case C::Cancel_Upgrade:
      if (entry.step == Step::Resource) {
        const BWAPI::UpgradeType upgrade(unit->upgrade);
        const int next_level = upgrade_level(data, *unit, upgrade) + 1;
        player->minerals += upgrade.mineralPrice(next_level);
        player->gas += upgrade.gasPrice(next_level);
        unit->upgrade = BWAPI::UpgradeTypes::Enum::None;
        unit->remainingUpgradeTime = 0;
      } else if (entry.step == Step::Order) {
        unit->order = O::Nothing;
        unit->isIdle = true;
      }
      break;

    case C::Cloak:
      unit->order = O::Cloak;
      unit->energy -= unit_type().cloakingTech().energyCost();
      break;

    case C::Decloak:
      unit->order = O::Decloak;
      break;

    case C::Follow:
      unit->order = O::Follow;
      unit->target = target_id;
      unit->isIdle = false;
      unit->isMoving = true;
      break;

    case C::Gather:
      if (target == nullptr) return;
      unit->target = target_id;
      unit->isIdle = false;
      unit->isMoving = true;
      unit->isGathering = true;
      if (BWAPI::UnitType(target->type).isMineralField())
        unit->order = O::MoveToMinerals;
      else if (BWAPI::UnitType(target->type).isRefinery())
        unit->order = O::MoveToGas;
      break;

    case C::Halt_Construction:
      if (entry.step == Step::Order) {
        if (BWAPI::UnitData* building = unit_at(data, unit->buildUnit); building != nullptr)
          building->buildUnit = -1;
        unit->buildUnit = -1;
        unit->order = O::ResetCollision;
        unit->isConstructing = false;
        unit->buildType = U::None;
      } else if (entry.step == Step::Finish) {
        unit->order = O::PlayerGuard;
        unit->isIdle = true;
      }
      break;

    case C::Hold_Position:
      unit->isMoving = false;
      unit->isIdle = false;
      unit->order = O::HoldPosition;
      break;

    case C::Land:
      unit->order = O::BuildingLand;
      unit->isIdle = false;
      break;

    case C::Lift:
      unit->order = O::BuildingLiftOff;
      unit->isIdle = false;
      break;

    case C::Load:
      if (unit_type() == BWAPI::UnitTypes::Terran_Bunker) {
        unit->order = O::PickupBunker;
        unit->target = target_id;
      } else if (unit_type().spaceProvided() != 0) {
        unit->order = O::PickupTransport;
        unit->target = target_id;
      } else if (target != nullptr && BWAPI::UnitType(target->type).spaceProvided() != 0) {
        unit->order = O::EnterTransport;
        unit->target = target_id;
      }
      unit->isIdle = false;
      break;

    // The game takes a building morph's price one frame after the command;
    // it is held from the command on, as for an addon.
    case C::Morph: {
      const BWAPI::UnitType morph(command.extra);
      if (entry.step == Step::Resource) {
        if (!current_frame) {
          unit->isCompleted = false;
          unit->isIdle = false;
          unit->isConstructing = true;
          unit->isMorphing = true;
          unit->buildType = morph.getID();
        }
        if (unit_type().isBuilding()) {
          if (!current_frame) {
            unit->order = O::ZergBuildingMorph;
            unit->type = morph.getID();
          }
          player->minerals -= morph.mineralPrice();
          player->gas -= morph.gasPrice();
        } else {
          add_supply(*player, morph.getRace(),
                     morph.supplyRequired() * (1 + static_cast<int>(morph.isTwoUnitsInOneEgg())) -
                         unit_type().supplyRequired());
          if (!current_frame) {
            unit->order = O::ZergUnitMorph;
            player->minerals -= morph.mineralPrice();
            player->gas -= morph.gasPrice();
            switch (morph.getID()) {
              case U::Zerg_Lurker_Egg:
                unit->type = U::Zerg_Lurker_Egg;
                break;
              case U::Zerg_Devourer:
              case U::Zerg_Guardian:
                unit->type = U::Zerg_Cocoon;
                break;
              default:
                unit->type = U::Zerg_Egg;
                break;
            }
            if (unit->trainingQueueCount >= 0 &&
                unit->trainingQueueCount < static_cast<int>(std::size(unit->trainingQueue)))
              unit->trainingQueue[unit->trainingQueueCount++] = morph.getID();
          }
        }
      } else if (entry.step == Step::Order && unit_type().isBuilding()) {
        unit->order = O::IncompleteBuilding;
      }
      break;
    }

    case C::Move:
    case C::Right_Click_Position:
      unit->order = O::Move;
      set_target_position(*unit, command.x, command.y);
      unit->isMoving = true;
      unit->isIdle = false;
      break;

    case C::Patrol:
      unit->order = O::Patrol;
      unit->isIdle = false;
      unit->isMoving = true;
      set_target_position(*unit, command.x, command.y);
      break;

    case C::Repair:
      if (unit_type() != BWAPI::UnitTypes::Terran_SCV) return;
      unit->order = O::Repair;
      unit->target = target_id;
      unit->isIdle = false;
      break;

    case C::Research: {
      const BWAPI::TechType tech(command.extra);
      unit->order = O::ResearchTech;
      unit->tech = tech.getID();
      unit->isIdle = false;
      unit->remainingResearchTime = tech.researchTime();
      player->minerals -= tech.mineralPrice();
      player->gas -= tech.gasPrice();
      if (tech.getID() >= 0 && tech.getID() < static_cast<int>(std::size(player->isResearching)))
        player->isResearching[tech.getID()] = true;
      break;
    }

    case C::Return_Cargo:
      if (unit->carryResourceType == 0) return;
      // carryResourceType 1 is gas (Unit::isCarryingGas).
      unit->order = unit->carryResourceType == 1 ? O::ReturnGas : O::ReturnMinerals;
      unit->isGathering = true;
      unit->isIdle = false;
      break;

    case C::Right_Click_Unit: {
      if (target == nullptr) return;
      unit->target = target_id;
      unit->isIdle = false;
      unit->isMoving = true;
      const BWAPI::UnitType self = unit_type();
      const BWAPI::UnitType other(target->type);
      if (self.isWorker() && other.isMineralField()) {
        unit->isGathering = true;
        unit->order = O::MoveToMinerals;
      } else if (self.isWorker() && other.isRefinery()) {
        unit->isGathering = true;
        unit->order = O::MoveToGas;
      } else if (self.isWorker() && other.getRace() == BWAPI::Races::Terran &&
                 other.whatBuilds().first == self && !target->isCompleted) {
        // An SCV resumes an unfinished Terran building.
        unit->order = O::ConstructingBuilding;
        unit->buildUnit = target_id;
        target->buildUnit = command.unitIndex;
        unit->isConstructing = true;
        target->isConstructing = true;
      } else if (self.canAttack() && target->player != unit->player && !other.isNeutral()) {
        unit->order = O::AttackUnit;
      } else if (self.canMove()) {
        unit->order = O::Follow;
      }
      break;
    }

    case C::Set_Rally_Position:
      if (!unit_type().canProduce()) return;
      unit->order = O::RallyPointTile;
      unit->rallyPositionX = command.x;
      unit->rallyPositionY = command.y;
      unit->rallyUnit = -1;
      break;

    case C::Set_Rally_Unit:
      if (!unit_type().canProduce()) return;
      if (target == nullptr || !target->exists) return;
      unit->order = O::RallyPointUnit;
      unit->rallyUnit = target_id;
      break;

    case C::Siege:
      unit->order = O::Sieging;
      break;

    case C::Stop:
      unit->order = O::Stop;
      unit->isIdle = true;
      break;

    // The game takes the price on the command's frame and the supply one
    // frame later; both are shown from the command on.
    case C::Train: {
      const BWAPI::UnitType trained(command.extra);
      if (!current_frame) {
        player->minerals -= trained.mineralPrice();
        player->gas -= trained.gasPrice();
      }
      if (unit->trainingQueueCount >= 0 &&
          unit->trainingQueueCount < static_cast<int>(std::size(unit->trainingQueue)))
        unit->trainingQueue[unit->trainingQueueCount++] = trained.getID();
      add_supply(*player, trained.getRace(), trained.supplyRequired());
      unit->isTraining = true;
      unit->isIdle = false;
      unit->remainingTrainTime = trained.buildTime();
      if (trained == BWAPI::UnitTypes::Terran_Nuclear_Missile) unit->secondaryOrder = O::Train;
      break;
    }

    case C::Unburrow:
      unit->order = O::Unburrowing;
      break;

    case C::Unload:
      unit->order = O::Unload;
      unit->target = target_id;
      break;

    case C::Unload_All:
      if (unit_type() == BWAPI::UnitTypes::Terran_Bunker) {
        unit->order = O::Unload;
      } else {
        unit->order = O::MoveUnload;
        set_target_position(*unit, command.x, command.y);
      }
      break;

    case C::Unload_All_Position:
      unit->order = O::MoveUnload;
      set_target_position(*unit, command.x, command.y);
      break;

    case C::Unsiege:
      unit->order = O::Unsieging;
      break;

    case C::Upgrade: {
      const BWAPI::UpgradeType upgrade(command.extra);
      unit->order = O::Upgrade;
      unit->upgrade = upgrade.getID();
      unit->isIdle = false;
      const int level = upgrade_level(data, *unit, upgrade);
      unit->remainingUpgradeTime = upgrade.upgradeTime(level + 1);
      player->minerals -= upgrade.mineralPrice(level + 1);
      player->gas -= upgrade.gasPrice(level + 1);
      if (upgrade.getID() >= 0 && upgrade.getID() < static_cast<int>(std::size(player->isUpgrading)))
        player->isUpgrading[upgrade.getID()] = true;
      break;
    }

    case C::Use_Tech:
      if (command.extra == BWAPI::TechTypes::Enum::Stim_Packs && unit->hitPoints > 10) {
        unit->hitPoints -= 10;
        unit->stimTimer = 17;
      }
      break;

    case C::Use_Tech_Position: {
      const BWAPI::TechType tech(command.extra);
      if (!tech.targetsPosition()) return;
      unit->order = tech.getOrder().getID();
      set_target_position(*unit, command.x, command.y);
      break;
    }

    case C::Use_Tech_Unit: {
      const BWAPI::TechType tech(command.extra);
      if (!tech.targetsUnit() || target == nullptr) return;
      unit->order = tech.getOrder().getID();
      unit->orderTarget = target_id;
      set_target_position(*unit, target->positionX, target->positionY);
      break;
    }

    default:
      break;
  }
}

}  // namespace BWRAPI::Legacy44
