// UnitData: the units table of one frame plus the BWAPI rules built on it.
#include "legacy44_projection.h"
#include "legacy44_bwapi_rules.h"

#include <BWAPI/Order.h>
#include <BWAPI/Position.h>
#include <BWAPI/UnitType.h>
#include <BWAPI/UpgradeType.h>

#include <algorithm>
#include <array>
#include <limits>

namespace BWRAPI::Legacy44 {

namespace {

// Unit box dimensions, the BWAPI 4.4 UnitType.cpp unitDimensions table.
#include "../../compatibility/src/UnitTypeStaticData.inc"

// BWAPI 4.4 UnitTypes::maxUnitWidth/Height are the maxima over
// UnitTypes::allUnitTypes(): 256 (Special_Floor_Hatch) and 160
// (Spell_Dark_Swarm).
constexpr int kMaxUnitWidth = 256;
constexpr int kMaxUnitHeight = 160;

// Iscript animations of BW::Anims that mean the unit is attacking.
constexpr std::uint8_t kGroundAttackInit = 0x02;
constexpr std::uint8_t kAirAttackInit = 0x03;
constexpr std::uint8_t kGroundAttackRepeat = 0x05;
constexpr std::uint8_t kAirAttackRepeat = 0x06;

// BWtoBWAPI.cpp AttackAnimationRestFrame: the frame set a unit's attack
// animation rests on; 10000 for every type the table does not name.
int attack_animation_rest_frame(int type) {
  namespace UT = BWAPI::UnitTypes::Enum;
  switch (type) {
    case UT::Terran_Marine:
    case UT::Terran_Ghost:
      return 34;
    case UT::Terran_SCV:
    case UT::Zerg_Broodling:
      return 17;
    case UT::Terran_Firebat:
    case UT::Protoss_Corsair:
    case UT::Protoss_Zealot:
    case UT::Protoss_Arbiter:
      return 0;
    case UT::Protoss_Dark_Templar:
    case UT::Zerg_Ultralisk:
      return 153;
    case UT::Protoss_Dragoon:
      return 357;
    case UT::Zerg_Zergling:
      return 85;
    case UT::Zerg_Hydralisk:
      return 51;
    case UT::Zerg_Sunken_Colony:
      return 33;
    default:
      return 10000;
  }
}

// Training queue of a unit: the leading known entries (the table marks the end
// of the queue with an unknown entry).
int training_queue_count(const BwrUnits& units, std::size_t row, std::array<int, 5>& queue) {
  const std::uint16_t entries[5] = {units.train_queue_0[row], units.train_queue_1[row],
                                    units.train_queue_2[row], units.train_queue_3[row],
                                    units.train_queue_4[row]};
  int count = 0;
  while (count < 5 && known(entries[count])) {
    queue[count] = entries[count];
    ++count;
  }
  return count;
}

bool unit_finder_matches_rectangle(int type, int unit_x, int unit_y, int left, int top,
                                   int right, int bottom) {
  const auto* data = findUnitTypeStaticData(type);
  if (data == nullptr) return false;
  const bool width_extended = right - left + 1 < kMaxUnitWidth;
  const bool height_extended = top - bottom + 1 < kMaxUnitHeight;
  const int search_right = right + (width_extended ? kMaxUnitWidth : 0);
  const int search_bottom = bottom + (height_extended ? kMaxUnitHeight : 0);
  if (unit_x < left || unit_x > search_right + 1 ||
      unit_y < top || unit_y > search_bottom + 1)
    return false;
  if (width_extended && unit_x - data->dimensionLeft > right) return false;
  if (height_extended && unit_y - data->dimensionUp > bottom) return false;
  return true;
}

// UnitUpdate.cpp updateData without canAccessDetected: the values an
// undetected unit shows. Resources, acid spores, isGathering and
// isInvincible are not reset there; the caller keeps their last values.
void clear_undetected(BWAPI::UnitData& unit) {
  unit.lastHitPoints = unit.hitPoints = unit.shields = 0;
  unit.resourceGroup = unit.killCount = 0;
  unit.defenseMatrixPoints = unit.defenseMatrixTimer = 0;
  unit.ensnareTimer = unit.irradiateTimer = unit.lockdownTimer = unit.maelstromTimer = 0;
  unit.orderTimer = unit.plagueTimer = unit.removeTimer = unit.stasisTimer = unit.stimTimer = 0;
  unit.order = unit.secondaryOrder = BWAPI::Orders::Enum::Unknown;
  unit.buildUnit = -1;
  unit.isTraining = unit.isMorphing = unit.isConstructing = unit.isIdle = false;
  unit.target = unit.orderTarget = -1;
  unit.targetPositionX = unit.orderTargetPositionX = BWAPI::Positions::Unknown.x;
  unit.targetPositionY = unit.orderTargetPositionY = BWAPI::Positions::Unknown.y;
  unit.addon = unit.nydusExit = unit.powerUp = -1;
  unit.isAccelerating = unit.isBeingGathered = unit.isBlind = unit.isBraking = false;
  unit.carryResourceType = 0;
  unit.isLifted = unit.isParasited = unit.isSelected = unit.isUnderStorm = false;
  unit.isPowered = true;
  unit.isStuck = unit.isInterruptible = false;
  unit.buttonset = BWAPI::UnitTypes::Enum::None;
  unit.lastAttackerPlayer = -1;
  unit.recentlyAttacked = false;
  unit.scarabCount = unit.interceptorCount = unit.spiderMineCount = 0;
  unit.carrier = unit.hatchery = -1;
}

// UnitUpdate.cpp updateData without canAccessInside: the production and
// private values of a unit the BWAPI player does not own.
void clear_inside(BWAPI::UnitData& unit) {
  unit.energy = 0;
  unit.buildType = BWAPI::UnitTypes::Enum::None;
  unit.trainingQueueCount = 0;
  std::fill(std::begin(unit.trainingQueue), std::end(unit.trainingQueue), 0);
  unit.tech = BWAPI::TechTypes::Enum::None;
  unit.upgrade = BWAPI::UpgradeTypes::Enum::None;
  unit.remainingBuildTime = unit.remainingTrainTime = 0;
  unit.remainingResearchTime = unit.remainingUpgradeTime = 0;
  unit.rallyPositionX = BWAPI::Positions::None.x;
  unit.rallyPositionY = BWAPI::Positions::None.y;
  unit.rallyUnit = unit.transport = -1;
  unit.hasNuke = unit.isHallucination = false;
}

}  // namespace

// updateInternalData clears every isVisible cell but the sprite-backed ones,
// which are all false for a unit nobody may access; updateData then writes
// the canAccess, canAccessDetected and canAccessInside defaults. Besides the
// fields clear_undetected keeps, id, clearanceLevel and isAttackFrame keep
// their last values.
void reset_inaccessible_unit(BWAPI::UnitData& unit, int neutral) {
  std::fill(std::begin(unit.isVisible), std::end(unit.isVisible), false);
  unit.isDetected = false;
  unit.replayID = 0;
  unit.isUnderDarkSwarm = unit.isUnderDWeb = false;
  unit.positionX = BWAPI::Positions::Unknown.x;
  unit.positionY = BWAPI::Positions::Unknown.y;
  unit.angle = unit.velocityX = unit.velocityY = 0;
  unit.groundWeaponCooldown = unit.airWeaponCooldown = unit.spellCooldown = 0;
  unit.isAttacking = unit.isBurrowed = unit.isCloaked = unit.isCompleted = false;
  unit.isMoving = unit.isStartingAttack = false;
  clear_undetected(unit);
  unit.exists = false;
  unit.player = neutral;
  unit.type = BWAPI::UnitTypes::Enum::Unknown;
  clear_inside(unit);
}

bool GameDataProjection::project_units(const BwrFrame& frame, const FrameSample& sample,
                                       BWAPI::GameData& data, std::string& error) {
  const BwrUnits* units_table = frame.units;
  if (units_table == nullptr) return true;
  const BwrUnits& t = *units_table;
  const int local_player = sample.local_player;
  // isVisible is indexed by player ID (UnitUpdate.cpp selfPlayerID).
  const int self_id = player_ids_.id(local_player);
  const bool complete_map = data.flags[BWAPI::Flag::CompleteMapInformation];

  const auto set = [&](auto& cell, auto value) {
    cell = static_cast<std::decay_t<decltype(cell)>>(value);
  };
  const auto set_flag = [&](bool& cell, std::uint8_t value) {
    if (value <= 1) cell = value == 1;
  };
  // A related unit: 0 is none, an id of a live unit resolves, anything
  // else stays unknown. The id resolves even when the target is not
  // published; the client then reads its exists=false row.
  const auto relation = [&](int& cell, std::uint64_t id) {
    cell = -1;
    if (id == 0 || !known(id)) return;
    const auto slot = engine_slot(id);
    if (slot < slot_life_.size() && slot_life_[slot] == id) cell = slot_unit_id_[slot];
  };

  x_search_.clear();
  y_search_.clear();

  for (const auto& source : sample.units) {
    // Only accessible units are published; the rest keep their last callback
    // data with exists=false (UnitUpdate.cpp updateData/canAccess).
    if (!source.accessible) continue;
    const std::size_t row = source.row;
    auto& unit = data.units[source.id];
    // The row still holds the unit's last published values.
    const int last_resources = unit.resources;
    const int last_acid_spores = unit.acidSporeCount;
    const bool last_gathering = unit.isGathering;
    const bool last_invincible = unit.isInvincible;
    zero_game_data(data, reinterpret_cast<const unsigned char*>(&unit) -
                             reinterpret_cast<const unsigned char*>(&data),
                   sizeof(unit));
    const int type = source.type;
    unit.id = source.id;
    unit.player = player_ids_.lookup(source.owner);
    unit.type = type;
    unit.positionX = source.x;
    unit.positionY = source.y;
    unit.exists = true;

    // Motion.
    if (known(t.facing[row])) set(unit.angle, direction_to_angle(t.facing[row]));
    if (known(t.velocity_x[row])) set(unit.velocityX, t.velocity_x[row] / 256.0);
    if (known(t.velocity_y[row])) set(unit.velocityY, t.velocity_y[row] / 256.0);

    // Vitals; raw hit points, shields and energy are 1/256 points.
    if (known(t.hit_points[row])) set(unit.hitPoints, points_from_raw(t.hit_points[row]));
    if (known(t.previous_hit_points[row]))
      set(unit.lastHitPoints, points_from_raw(t.previous_hit_points[row]));
    if (known(t.shields[row])) set(unit.shields, points_from_raw(t.shields[row]));
    // UnitUpdate.cpp: energy is 0 for every type that is not a spellcaster.
    if (is_bwapi_spellcaster(type) && known(t.energy[row]))
      set(unit.energy, points_from_raw(t.energy[row]));
    if (known(t.resources[row])) set(unit.resources, t.resources[row]);
    if (known(t.resource_group[row])) set(unit.resourceGroup, t.resource_group[row]);
    if (known(t.kill_count[row])) set(unit.killCount, t.kill_count[row]);
    if (known(t.acid_spores[row])) set(unit.acidSporeCount, t.acid_spores[row]);
    if (known(t.scarabs[row])) set(unit.scarabCount, t.scarabs[row]);
    if (known(t.interceptors[row])) set(unit.interceptorCount, t.interceptors[row]);
    if (known(t.spider_mines[row])) set(unit.spiderMineCount, t.spider_mines[row]);

    // UnitUpdate.cpp publishes getGroundWeaponCooldown()/getAirWeaponCooldown():
    // a Reaver/Warbringer reads its order timer, every other unit its damage
    // dealer (the turret when there is one). An unknown turret relation or a
    // missing turret row is unknown, never the parent's own turret-less value.
    std::uint8_t ground = t.ground_cooldown[row];
    std::uint8_t air = t.air_cooldown[row];
    const std::uint64_t sub_unit = t.sub_unit[row];
    if (!known(sub_unit)) {
      ground = air = std::numeric_limits<std::uint8_t>::max();
    } else if (sub_unit != 0) {
      std::size_t turret_row = 0;
      const bool has_row = unit_row_of(engine_slot(sub_unit), turret_row);
      ground = !has_row ? std::numeric_limits<std::uint8_t>::max()
                        : t.ground_cooldown[turret_row];
      air = !has_row ? std::numeric_limits<std::uint8_t>::max() : t.air_cooldown[turret_row];
    }
    if (uses_order_timer_as_ground_cooldown(type)) ground = t.order_timer[row];
    if (known(ground)) set(unit.groundWeaponCooldown, ground);
    if (known(air)) set(unit.airWeaponCooldown, air);
    if (known(t.spell_cooldown[row])) set(unit.spellCooldown, t.spell_cooldown[row]);

    // UnitUpdate.cpp attack state, from the damage dealer's main image:
    // attacking while it plays an attack init/repeat animation and the unit
    // has an order target; starting an attack when a weapon cooldown rose
    // since the last published frame; an attack frame while the attack
    // animation is off its rest frame set. A missing or unknown image is
    // the official no-image case.
    {
      auto& track = unit_track(source.id);
      std::uint8_t animation = 0;
      std::uint16_t frame_set = 0;
      const std::size_t dealer = sub_unit != 0 ? engine_slot(sub_unit) : engine_slot(source.engine_id);
      const bool image = known(sub_unit) && sprite_image(dealer, animation, frame_set) &&
                         known(animation) && known(frame_set);
      const bool attacking =
          image && t.order_target[row] != 0 &&
          (animation == kGroundAttackInit || animation == kAirAttackInit ||
           animation == kGroundAttackRepeat || animation == kAirAttackRepeat);
      bool starting = false;
      if (known(ground) && known(air)) {
        starting = (air > track.last_air_cooldown || ground > track.last_ground_cooldown) && attacking;
        track.last_air_cooldown = air;
        track.last_ground_cooldown = ground;
      }
      unit.isAttacking = attacking;
      unit.isStartingAttack = starting;
      if (image) {
        const int rest = attack_animation_rest_frame(type);
        unit.isAttackFrame = starting || (attacking && (frame_set != rest || track.last_frame_set != rest));
        track.last_frame_set = frame_set;
      }
    }

    // Timers.
    if (known(t.defense_matrix_damage[row]))
      set(unit.defenseMatrixPoints, points_from_raw(t.defense_matrix_damage[row]));
    if (known(t.defense_matrix_timer[row])) set(unit.defenseMatrixTimer, t.defense_matrix_timer[row]);
    if (known(t.ensnare_timer[row])) set(unit.ensnareTimer, t.ensnare_timer[row]);
    if (known(t.irradiate_timer[row])) set(unit.irradiateTimer, t.irradiate_timer[row]);
    if (known(t.lockdown_timer[row])) set(unit.lockdownTimer, t.lockdown_timer[row]);
    if (known(t.maelstrom_timer[row])) set(unit.maelstromTimer, t.maelstrom_timer[row]);
    if (known(t.plague_timer[row])) set(unit.plagueTimer, t.plague_timer[row]);
    if (known(t.remove_timer[row])) set(unit.removeTimer, t.remove_timer[row]);
    if (known(t.stasis_timer[row])) set(unit.stasisTimer, t.stasis_timer[row]);
    if (known(t.stim_timer[row])) set(unit.stimTimer, t.stim_timer[row]);
    if (known(t.order_timer[row])) set(unit.orderTimer, t.order_timer[row]);
    if (known(t.storm_timer[row])) set(unit.isUnderStorm, t.storm_timer[row] > 0);

    // Orders.
    unit.order = source.order;
    if (known(t.secondary_order[row])) set(unit.secondaryOrder, t.secondary_order[row]);
    relation(unit.orderTarget, t.order_target[row]);
    if (known(t.order_target_x[row]) && known(t.order_target_y[row])) {
      set(unit.orderTargetPositionX, t.order_target_x[row]);
      set(unit.orderTargetPositionY, t.order_target_y[row]);
    } else {
      unit.orderTargetPositionX = unit.orderTargetPositionY = -1;
    }
    relation(unit.target, t.move_target[row]);
    unit.targetPositionX = unit.targetPositionY = BWAPI::Positions::Unknown.x;
    if (known(t.move_target_x[row]) && known(t.move_target_y[row])) {
      set(unit.targetPositionX, t.move_target_x[row]);
      set(unit.targetPositionY, t.move_target_y[row]);
    }
    // Rally: a position only for a unit that rallies.
    unit.rallyPositionX = unit.rallyPositionY = -1;
    if (known(t.rally_x[row]) && known(t.rally_y[row])) {
      set(unit.rallyPositionX, t.rally_x[row]);
      set(unit.rallyPositionY, t.rally_y[row]);
    }
    relation(unit.rallyUnit, t.rally_unit[row]);

    // Production. Research and upgrade are none unless the flag says so.
    if (known(t.build_type[row])) set(unit.buildType, t.build_type[row]);
    if (t.researching[row] == 0) {
      set(unit.tech, BWAPI::TechTypes::Enum::None);
      set(unit.remainingResearchTime, 0);
    } else if (t.researching[row] == 1) {
      if (known(t.tech[row])) set(unit.tech, t.tech[row]);
      if (known(t.remaining_research_frames[row]))
        set(unit.remainingResearchTime, t.remaining_research_frames[row]);
    }
    if (t.upgrading[row] == 0) {
      set(unit.upgrade, BWAPI::UpgradeTypes::Enum::None);
      set(unit.remainingUpgradeTime, 0);
    } else if (t.upgrading[row] == 1) {
      if (known(t.upgrade[row])) set(unit.upgrade, t.upgrade[row]);
      if (known(t.remaining_upgrade_frames[row]))
        set(unit.remainingUpgradeTime, t.remaining_upgrade_frames[row]);
    }
    if (known(t.remaining_build_frames[row]))
      set(unit.remainingBuildTime, t.remaining_build_frames[row]);
    // UnitUpdate.cpp: a Hatchery, Lair or Hive publishes its larva countdown
    // (0 once three larvae are connected, GameUnits.cpp augmentUnitData) in
    // every state; every other unit the build unit's remaining time.
    if (type == BWAPI::UnitTypes::Enum::Zerg_Hatchery || type == BWAPI::UnitTypes::Enum::Zerg_Lair ||
        type == BWAPI::UnitTypes::Enum::Zerg_Hive) {
      if (known(t.larva_frames[row])) set(unit.remainingTrainTime, t.larva_frames[row]);
    } else if (known(t.remaining_train_frames[row])) {
      set(unit.remainingTrainTime, t.remaining_train_frames[row]);
    }
    std::array<int, 5> queue{};
    const int queue_count = training_queue_count(t, row, queue);
    unit.trainingQueueCount = queue_count;
    std::copy(queue.begin(), queue.begin() + queue_count, std::begin(unit.trainingQueue));
    relation(unit.buildUnit, t.build_unit[row]);

    // Relations.
    relation(unit.addon, t.addon[row]);
    relation(unit.nydusExit, t.nydus_exit[row]);
    relation(unit.powerUp, t.power_up[row]);
    relation(unit.transport, t.transport[row]);
    relation(unit.carrier, t.carrier[row]);
    relation(unit.hatchery, t.hatchery[row]);

    // Booleans.
    set_flag(unit.hasNuke, t.has_nuke[row]);
    set_flag(unit.isAccelerating, t.accelerating[row]);
    set_flag(unit.isBlind, t.blind[row]);
    set_flag(unit.isBraking, t.braking[row]);
    set_flag(unit.isBurrowed, t.burrowed[row]);
    set_flag(unit.isCloaked, t.cloaked[row]);
    set_flag(unit.isGathering, t.gathering[row]);
    set_flag(unit.isBeingGathered, t.being_gathered[row]);
    set_flag(unit.isHallucination, t.hallucination[row]);
    set_flag(unit.isInterruptible, t.interruptible[row]);
    set_flag(unit.isMoving, t.moving[row]);
    set_flag(unit.isParasited, t.parasited[row]);
    set_flag(unit.isStuck, t.stuck[row]);
    set_flag(unit.recentlyAttacked, t.recently_attacked[row]);
    set_flag(unit.isInvincible, t.invincible[row]);
    if (known(t.carry[row])) set(unit.carryResourceType, t.carry[row]);
    // UnitUpdate.cpp copies CUnit::lastAttackingPlayer as it is: a slot, not a
    // player ID.
    if (known(t.last_attacker[row])) set(unit.lastAttackerPlayer, t.last_attacker[row]);
    else unit.lastAttackerPlayer = -1;
    if (known(t.button_set[row])) set(unit.buttonset, t.button_set[row]);
    unit.isCompleted = source.completed;
    // UnitUpdate.cpp: a Terran building is lifted when it is in the air.
    if (t.in_air[row] <= 1) {
      set(unit.isLifted, t.in_air[row] == 1 && is_liftable_terran_building(type));
    }
    unit.isSelected = source.selected;
    // BWAPI defaults every unit to powered and only clears Protoss buildings
    // with a positively observed unpowered status bit.
    unit.isPowered = !is_protoss_building(type) || t.powered[row] != 0;

    unit.isMorphing = is_morphing_order(source.order);
    unit.isTraining = queue_count > 0;
    unit.isConstructing = is_constructing_order(source.order) ||
                          is_morphing_order(source.order) ||
                          (known(t.secondary_order[row]) &&
                           t.secondary_order[row] == BWAPI::Orders::Enum::BuildAddon) ||
                          (!unit.isCompleted && unit.buildUnit >= 0);
    unit.isIdle = !unit.isTraining && !unit.isConstructing && !unit.isMorphing &&
                  is_bwapi_idle_order(source.order);

    // Visibility and detection. The BWAPI player's cell and isDetected come
    // from the sample's UnitUpdate.cpp rule. Every other cell is false
    // outside a replay or CompleteMapInformation; there it is the owner, the
    // neutral player's "any sprite bit" or the player's own sprite bit, and
    // false without a sprite.
    std::uint16_t sprite_bits = 0;
    const bool sprite_known = sprite_visibility(engine_slot(source.engine_id), sprite_bits) &&
                              known(sprite_bits);
    const bool self_known = local_player >= 0 && self_id >= 0 && self_id < 9 && !sample.is_replay;
    if (self_known) {
      unit.isVisible[self_id] = source.visible;
      unit.isDetected = source.detected;
    }
    if ((sample.is_replay || complete_map) && sprite_known) {
      // Cell i is the player with ID i; an ID without a player stays false.
      for (int player = 0; player < 9; ++player) {
        if (player == self_id && self_known) continue;
        const int slot = player_ids_.slot(player);
        if (slot < 0) continue;
        if (source.owner == slot) {
          unit.isVisible[player] = true;
        } else {
          unit.isVisible[player] = slot == static_cast<int>(kNeutralPlayerId)
                                       ? sprite_bits != 0
                                       : slot < 8 && (sprite_bits & (1 << slot)) != 0;
        }
      }
    }
    // Without a BWAPI player, detected means visible to any player.
    if (!self_known)
      unit.isDetected = std::any_of(std::begin(unit.isVisible), std::end(unit.isVisible),
                                    [](bool cell) { return cell; });
    // UnitImpl::canAccessDetected and canAccessInside: a unit the BWAPI
    // player does not own shows its private and production values only
    // under CompleteMapInformation (always on in a replay), and its detected
    // values only while detected (neutral types on the match's first frame,
    // as canAccess). Without a BWAPI player outside a replay there is no
    // perspective to hide anything from.
    const bool owned = !sample.is_replay && local_player >= 0 && source.owner == local_player;
    const bool full_access = complete_map || (!sample.is_replay && local_player < 0);
    const bool detected_access = full_access || owned || unit.isDetected ||
                                 (sample.match_started && BWAPI::UnitType(type).isNeutral());
    if (!detected_access) {
      clear_undetected(unit);
      unit.resources = last_resources;
      unit.acidSporeCount = last_acid_spores;
      unit.isGathering = last_gathering;
      unit.isInvincible = last_invincible;
    }
    if (!detected_access || !(full_access || owned)) clear_inside(unit);
    // BWAPI 4.4 UnitUpdate: replayID = UnitTarget(unit) under CMI, so
    // indexToUnit((replayID & 0x7ff) - 1) is this unit.
    unit.replayID = complete_map && source.unit_index >= 0 ? source.replay_id : 0;

    // The shared allocator maps the engine's record pool onto the 1700-entry
    // official array; units beyond capacity have no index. UnitData and the
    // search tables still hold every published unit.
    if (source.unit_index >= 0) {
      if (source.unit_index >= static_cast<int>(kMaxUnitArray) ||
          data.unitArray[source.unit_index] != -1) {
        error = "projected units share a unitArray slot";
        return false;
      }
      data.unitArray[source.unit_index] = source.id;
    }
    // The game's unit finder, which Server::updateSharedMemory copies, holds
    // each unit's box edges twice per axis (BWAPI getLeft..getRight and
    // getTop..getBottom) and leaves out the units it hides: those loaded in a
    // transport or bunker, and a worker inside a gas building, which holds
    // HarvestGas only while there. The client's search relies on both edges.
    if (unit.transport < 0 && unit.order != BWAPI::Orders::Enum::HarvestGas &&
        x_search_.size() + 2 <= kMaxUnitSearch) {
      const BWAPI::UnitType type(unit.type);
      x_search_.push_back(BWAPI::unitFinder{source.id, source.x - type.dimensionLeft()});
      x_search_.push_back(BWAPI::unitFinder{source.id, source.x + type.dimensionRight()});
      y_search_.push_back(BWAPI::unitFinder{source.id, source.y - type.dimensionUp()});
      y_search_.push_back(BWAPI::unitFinder{source.id, source.y + type.dimensionDown()});
    }
  }

  // GameUnits.cpp augmentUnitData: a builder and the unit it constructs (or
  // an incomplete addon / trained unit) reference each other.
  const auto live = [&](int id, int self) -> BWAPI::UnitData* {
    return id >= 0 && id != self && id < static_cast<int>(kMaxUnits) && data.units[id].exists
               ? &data.units[id] : nullptr;
  };
  // Sets one side: unit constructs (or is constructed with) `other` of type `type`.
  const auto construct = [&](BWAPI::UnitData& unit, int other, int type) {
    unit.buildUnit = other;
    unit.isConstructing = true;
    unit.isIdle = false;
    unit.buildType = type;
  };
  for (const auto& source : sample.units) {
    if (!source.accessible) continue;
    auto& unit = data.units[source.id];
    auto* built = live(unit.orderTarget, source.id);
    bool both = true;
    if (!built || unit.order != BWAPI::Orders::Enum::ConstructingBuilding) {
      built = live(unit.addon, source.id);
      if (built && built->isCompleted) built = nullptr;
    }
    if (!built && unit.isTraining) {
      built = live(unit.buildUnit, source.id);
      both = false;
    }
    if (!built) continue;
    if (both) construct(unit, built->id, built->type);
    construct(*built, unit.id, built->type);
  }

  // GameUnits.cpp computeSecondaryUnitSets: every accessible unit counts for
  // its owner in allUnitCount, the completed ones in completedUnitCount and
  // the ones Unit::isVisible() reports (the BWAPI player's cell, any cell
  // without one) in visibleUnitCount, under its type and under AllUnits and
  // Buildings (and Factories) or Men.
  {
    namespace UT = BWAPI::UnitTypes::Enum;
    const bool self_cell = self_id >= 0 && self_id < 9 && !sample.is_replay;
    for (const auto& source : sample.units) {
      if (!source.accessible) continue;
      const int owner = player_ids_.listed(source.owner);
      if (owner < 0) continue;
      const auto& unit = data.units[source.id];
      const auto& cells = unit.isVisible;
      const bool visible = self_cell ? cells[self_id]
                                     : std::any_of(std::begin(cells), std::end(cells),
                                                   [](bool cell) { return cell; });
      auto& player = data.players[owner];
      const auto count = [&](int counted) {
        ++player.allUnitCount[counted];
        if (visible) ++player.visibleUnitCount[counted];
        if (unit.isCompleted) ++player.completedUnitCount[counted];
      };
      count(source.type);
      count(UT::AllUnits);
      if (is_bwapi_building_type(source.type)) {
        count(UT::Buildings);
        if (is_bwapi_factory_type(source.type)) count(UT::Factories);
      } else {
        count(UT::Men);
      }
    }
    // The next frame's PlayerImpl::updateData reads these counts.
    for (std::size_t id = 0; id < static_cast<std::size_t>(player_ids_.count()); ++id) {
      const auto& completed = data.players[id].completedUnitCount;
      for (std::size_t upgrade = 0; upgrade < kUpgradeTypeCount; ++upgrade) {
        bool used = false;
        for (const BWAPI::UnitType user : BWAPI::UpgradeType(static_cast<int>(upgrade)).whatUses())
          if (completed[user] > 0) {
            used = true;
            break;
          }
        upgrade_users_[id][upgrade] = used;
      }
    }
  }

  const auto finder_less = [](const BWAPI::unitFinder& left, const BWAPI::unitFinder& right) {
    if (left.searchValue != right.searchValue) return left.searchValue < right.searchValue;
    return left.unitIndex < right.unitIndex;
  };
  std::sort(x_search_.begin(), x_search_.end(), finder_less);
  std::sort(y_search_.begin(), y_search_.end(), finder_less);
  for (std::size_t index = 0; index < x_search_.size(); ++index) {
    data.xUnitSearch[index] = x_search_[index];
    data.yUnitSearch[index] = y_search_[index];
  }
  data.unitSearchSize = static_cast<int>(x_search_.size());

  // Dark Swarm and Disruption Web cover the ground units under them.
  for (const auto& spell : sample.units) {
    if (!spell.accessible) continue;
    if (spell.owner != static_cast<int>(kNeutralPlayerId) ||
        (spell.type != BWAPI::UnitTypes::Enum::Spell_Dark_Swarm &&
         spell.type != BWAPI::UnitTypes::Enum::Spell_Disruption_Web))
      continue;
    const auto* spell_type = findUnitTypeStaticData(spell.type);
    if (spell_type == nullptr) continue;
    const int web = spell.type == BWAPI::UnitTypes::Enum::Spell_Disruption_Web ? 1 : 0;
    const int right = spell.x + spell_type->dimensionRight - web;
    const int bottom = spell.y + spell_type->dimensionDown - web;
    const int left = spell.x - spell_type->dimensionLeft;
    const int top = spell.y - spell_type->dimensionUp;
    for (const auto& candidate : sample.units) {
      if (!candidate.accessible) continue;
      const bool lifted = data.units[candidate.id].isLifted;
      if (!is_spell_affected_unit(candidate.type, lifted) ||
          !unit_finder_matches_rectangle(candidate.type, candidate.x, candidate.y, left, top,
                                         right, bottom))
        continue;
      if (spell.type == BWAPI::UnitTypes::Enum::Spell_Dark_Swarm)
        data.units[candidate.id].isUnderDarkSwarm = true;
      else
        data.units[candidate.id].isUnderDWeb = true;
    }
  }
  return true;
}

}  // namespace BWRAPI::Legacy44
