// UnitData built from the engine units table: values, units, relations and rules.
#include "legacy44_test_support.h"

#include <BWAPI/Order.h>
#include <BWAPI/Position.h>
#include <BWAPI/TechType.h>
#include <BWAPI/UnitType.h>
#include <BWAPI/UpgradeType.h>

using namespace BWRAPI::Legacy44;
using namespace BWRAPI::Legacy44::Test;
namespace UT = BWAPI::UnitTypes::Enum;
namespace OR = BWAPI::Orders::Enum;

namespace {

// Raw hit points, shields, energy and the defense matrix are 1/256 points and
// round up.
void values_and_unknown_cells() {
  Env env;
  env.fx.local_player = 0;
  env.fx.players.rows.push_back(player_row(0, 1));
  auto marine = unit_row(5, UT::Terran_Marine, 0, 100, 200, 3);
  marine.hit_points = 40 * 256 + 1;
  marine.shields = 1;
  marine.kill_count = 7;
  marine.velocity_x = 512;
  marine.facing = 64;
  marine.ground_cooldown = 9;
  marine.sub_unit = 0;
  env.fx.units.rows.push_back(marine);
  CHECK(env.project());
  CHECK(env.sample.units.size() == 1);
  const int id = env.sample.units[0].id;
  const auto& unit = env.unit(id);
  CHECK(unit.id == id && unit.player == 0 && unit.type == UT::Terran_Marine);
  CHECK(unit.positionX == 100 && unit.positionY == 200 && unit.exists);
  CHECK(unit.hitPoints == 41);
  CHECK(unit.shields == 1);
  CHECK(unit.killCount == 7);
  CHECK(unit.velocityX == 2.0);
  CHECK(unit.angle == 0.0);
  CHECK(unit.groundWeaponCooldown == 9);
  // A column the engine leaves unknown stays zero and unmarked.
  CHECK(unit.acidSporeCount == 0);
  CHECK(unit.airWeaponCooldown == 0);
  CHECK(unit.isAttacking == false);
  // A Marine is not a spellcaster: energy is a known 0.
  CHECK(unit.energy == 0);
  // The unit sits in the official unitArray slot of its record.
  CHECK(env.data->unitArray[5] == id);
  // The finder holds the Marine's box edges (8, 9, 8, 10 pixels from its
  // position), each axis sorted.
  CHECK(env.data->unitSearchSize == 2 && env.data->xUnitSearch[0].unitIndex == id &&
        env.data->xUnitSearch[0].searchValue == 92 && env.data->xUnitSearch[1].searchValue == 108 &&
        env.data->yUnitSearch[0].searchValue == 191 && env.data->yUnitSearch[1].searchValue == 210);
}

// UnitUpdate.cpp attack state from the damage dealer's main image and the
// weapon cooldowns of the previous published frame.
void attack_state() {
  Env env;
  env.fx.local_player = 0;
  auto marine = unit_row(1, UT::Terran_Marine, 0, 100, 100);
  marine.sub_unit = 0;
  marine.ground_cooldown = 0;
  marine.air_cooldown = 0;
  marine.order_target = unit_id(3);
  auto tank = unit_row(2, UT::Terran_Siege_Tank_Tank_Mode, 0, 200, 100);
  tank.sub_unit = unit_id(4);
  tank.order_target = unit_id(3);
  auto target = unit_row(3, UT::Zerg_Zergling, 0, 150, 100);
  target.sub_unit = 0;
  auto turret = unit_row(4, UT::Terran_Siege_Tank_Tank_Mode_Turret, 0, 200, 100);
  turret.ground_cooldown = 0;
  turret.air_cooldown = 0;
  env.fx.units.rows = {marine, tank, target, turret};
  env.fx.render.rows.resize(4);
  // The Marine repeats its ground attack on its rest frame set 34.
  env.fx.render.rows[0] = {unit_id(1), 100, 100, 0b01, 5, 34};
  // The tank attacks through its turret's image, not its own.
  env.fx.render.rows[1] = {unit_id(2), 200, 100, 0b01, 0, 0};
  env.fx.render.rows[2] = {unit_id(3), 150, 100, 0b01, 0, 0};
  env.fx.render.rows[3] = {unit_id(4), 200, 100, 0b01, 2, 0};
  CHECK(env.project());
  const int marine_id = env.sample.units[0].id;
  const int tank_id = env.sample.units[1].id;
  const int target_id = env.sample.units[2].id;
  // The first frame set differs from the remembered one, so it is an attack frame.
  CHECK(env.unit(marine_id).isAttacking && !env.unit(marine_id).isStartingAttack &&
        env.unit(marine_id).isAttackFrame);
  CHECK(env.unit(tank_id).isAttacking && env.unit(tank_id).isAttackFrame);
  CHECK(!env.unit(target_id).isAttacking && !env.unit(target_id).isAttackFrame);
  // Resting on the rest frame set twice is not an attack frame.
  CHECK(env.project());
  CHECK(env.unit(marine_id).isAttacking && !env.unit(marine_id).isAttackFrame);
  // A rising cooldown starts an attack; the turret's cooldown counts for the tank.
  env.fx.units.rows[0].ground_cooldown = 15;
  env.fx.units.rows[3].ground_cooldown = 37;
  CHECK(env.project());
  CHECK(env.unit(marine_id).isStartingAttack && env.unit(marine_id).isAttackFrame);
  CHECK(env.unit(tank_id).isStartingAttack);
  // A falling cooldown does not; off the rest frame set it is an attack frame.
  env.fx.units.rows[0].ground_cooldown = 14;
  env.fx.render.rows[0].frame_set = 17;
  CHECK(env.project());
  CHECK(!env.unit(marine_id).isStartingAttack && env.unit(marine_id).isAttackFrame);
  // Without an order target the animation alone is not an attack.
  env.fx.units.rows[0].order_target = 0;
  CHECK(env.project());
  CHECK(!env.unit(marine_id).isAttacking && !env.unit(marine_id).isAttackFrame);
  // An unknown image is the official no-image case.
  env.fx.units.rows[0].order_target = unit_id(3);
  env.fx.render.rows[0].animation = UINT8_MAX;
  CHECK(env.project());
  CHECK(!env.unit(marine_id).isAttacking);
}

// UnitUpdate.cpp isBeingGathered: a resource container with a worker in its
// gather line.
void resource_being_gathered() {
  Env env;
  auto mineral = unit_row(1, UT::Resource_Mineral_Field, 11, 100, 100);
  mineral.being_gathered = 1;
  auto geyser = unit_row(2, UT::Resource_Vespene_Geyser, 11, 200, 100);
  geyser.being_gathered = 0;
  env.fx.units.rows = {mineral, geyser};
  CHECK(env.project());
  CHECK(env.unit(env.sample.units[0].id).isBeingGathered);
  CHECK(!env.unit(env.sample.units[1].id).isBeingGathered);
}

void spellcaster_energy() {
  Env env;
  env.fx.local_player = 0;
  auto ghost = unit_row(1, UT::Terran_Ghost, 0, 1, 1);
  ghost.energy = 50 * 256 + 255;
  auto other = unit_row(2, UT::Terran_Ghost, 0, 2, 2);  // energy unknown
  env.fx.units.rows = {ghost, other};
  CHECK(env.project());
  CHECK(env.unit(env.sample.units[0].id).energy == 51);
}

// Related units resolve to BWAPI ids; 0 is a known none; anything else unknown.
void relations() {
  Env env;
  auto scv = unit_row(1, UT::Terran_SCV, 0, 10, 10);
  auto mineral = unit_row(2, UT::Resource_Mineral_Field, 11, 12, 12);
  scv.order_target = unit_id(2);
  scv.transport = 0;
  scv.addon = unit_id(30);  // not a unit of this frame
  mineral.order_target = 0;
  env.fx.units.rows = {scv, mineral};
  CHECK(env.project());
  const auto& a = env.unit(env.sample.units[0].id);
  const auto& b = env.unit(env.sample.units[1].id);
  CHECK(a.orderTarget == b.id);
  CHECK(a.transport == -1);
  CHECK(a.addon == -1);
  CHECK(a.carrier == -1);
  CHECK(b.orderTarget == -1);
}

// A tank reads its damage dealer: the turret is not a public unit.
void turret_cooldown() {
  Env env;
  auto tank = unit_row(1, UT::Terran_Siege_Tank_Tank_Mode, 0, 10, 10);
  tank.sub_unit = unit_id(2);
  tank.ground_cooldown = 1;
  tank.air_cooldown = 2;
  auto turret = unit_row(2, UT::Terran_Siege_Tank_Tank_Mode_Turret, 0, 10, 10);
  turret.ground_cooldown = 11;
  turret.air_cooldown = 22;
  auto lone = unit_row(3, UT::Terran_Siege_Tank_Tank_Mode, 0, 50, 50);
  lone.sub_unit = unit_id(40);  // turret row is missing
  lone.ground_cooldown = 5;
  env.fx.units.rows = {tank, turret, lone};
  CHECK(env.project());
  CHECK(env.sample.units.size() == 2 && env.data->unitSearchSize == 4);
  const auto& parent = env.unit(env.sample.units[0].id);
  CHECK(parent.groundWeaponCooldown == 11 && parent.airWeaponCooldown == 22);
  const auto& orphan = env.unit(env.sample.units[1].id);
  CHECK(orphan.groundWeaponCooldown == 0);
}

void reaver_cooldown_is_order_timer() {
  Env env;
  auto reaver = unit_row(1, UT::Protoss_Reaver, 0, 10, 10);
  reaver.order_timer = 17;
  reaver.sub_unit = 0;
  reaver.ground_cooldown = 3;
  env.fx.units.rows = {reaver};
  CHECK(env.project());
  CHECK(env.unit(env.sample.units[0].id).groundWeaponCooldown == 17);
}

// Local visibility comes from the sprite bit; hidden enemy units are not
// published at all, and counts only with complete data.
void visibility_and_selection() {
  Env env;
  env.fx.local_player = 1;
  env.fx.players.rows = {player_row(0, 0), player_row(1, 0)};
  env.fx.units.rows = {unit_row(1, UT::Zerg_Zergling, 1, 10, 10),
                       unit_row(2, UT::Zerg_Zergling, 0, 20, 20),
                       unit_row(3, UT::Zerg_Zergling, 0, 30, 30)};
  env.fx.render.rows.resize(3);
  env.fx.render.rows[0] = {unit_id(1), 10, 10, 0b10};
  env.fx.render.rows[1] = {unit_id(2), 20, 20, 0b01};  // not visible to player 1
  env.fx.render.rows[2] = {unit_id(3), 30, 30, 0b11};
  env.fx.selection.rows.resize(1);
  env.fx.selection.rows[0].unit = unit_id(1);
  CHECK(env.project());
  // Every row is sampled; the hidden enemy is not published (no search entry,
  // exists=false) while the visible one is.
  CHECK(env.sample.units.size() == 3 && env.data->unitSearchSize == 4);
  const auto& own = env.unit(env.sample.units[0].id);
  const auto& hidden = env.unit(env.sample.units[1].id);
  const auto& enemy = env.unit(env.sample.units[2].id);
  CHECK(own.isVisible[1]);
  CHECK(enemy.isVisible[1]);
  CHECK(!hidden.exists);
  // Another player's cell is a known false outside replay and CMI.
  CHECK(!own.isVisible[0]);
  CHECK(own.isSelected && !enemy.isSelected);
  CHECK(env.data->selectedUnitCount == 1 && env.data->selectedUnits[0] == own.id);
  CHECK(env.data->players[1].visibleUnitCount[UT::Zerg_Zergling] == 1);
  CHECK(env.data->players[1].visibleUnitCount[UT::AllUnits] == 1);
  CHECK(env.data->players[0].visibleUnitCount[UT::Zerg_Zergling] == 1);
}

void unknown_sprite_visibility() {
  Env env;
  env.fx.local_player = 0;
  env.fx.players.rows = {player_row(0, 0)};
  env.fx.units.rows = {unit_row(1, UT::Zerg_Zergling, 0, 10, 10)};
  CHECK(env.project());
  const auto& unit = env.unit(env.sample.units[0].id);
  // An own unit is always visible and detected to its owner (UnitUpdate.cpp).
  CHECK(unit.isVisible[0]);
  CHECK(unit.isDetected);
  // The visible count holds every accessible unit the BWAPI player sees.
  CHECK(env.data->players[0].visibleUnitCount[UT::AllUnits] == 1);
}

// An enemy is published only while the BWAPI player sees its sprite; one
// without a sprite is hidden.
void accessibility_of_hidden_units() {
  Env env;
  env.fx.local_player = 0;
  auto enemy = unit_row(2, UT::Zerg_Zergling, 1, 20, 20);
  env.fx.units.rows = {unit_row(1, UT::Zerg_Zergling, 0, 10, 10), enemy};
  // No render table: the own unit stays published, the enemy does not.
  CHECK(env.project());
  CHECK(env.sample.units.size() == 2 && env.unit(env.sample.units[0].id).exists);
  CHECK(!env.unit(env.sample.units[1].id).exists);
  // A known-invisible enemy row is withheld.
  env.fx.render.rows = {{unit_id(1), 10, 10, 0b01}, {unit_id(2), 20, 20, 0b00}};
  CHECK(env.project());
  CHECK(env.sample.units.size() == 2);
  CHECK(env.unit(env.sample.units[0].id).exists && !env.unit(env.sample.units[1].id).exists);
  CHECK(env.data->unitSearchSize == 2);
  // CompleteMapInformation publishes it again.
  env.data->flags[BWAPI::Flag::CompleteMapInformation] = true;
  CHECK(env.project());
  CHECK(env.unit(env.sample.units[1].id).exists && env.data->unitSearchSize == 4);
  CHECK(env.unit(env.sample.units[1].id).replayID != 0);
}

// Neutral units ride the match's first frame even without vision (canAccess).
void neutral_units_on_the_first_frame() {
  Env env;
  env.fx.local_player = 0;
  env.fx.units.rows = {unit_row(1, UT::Resource_Mineral_Field, 11, 10, 10)};
  env.fx.render.rows = {{unit_id(1), 10, 10, 0b00}};
  CHECK(env.project());
  CHECK(env.sample.units[0].accessible && env.unit(env.sample.units[0].id).exists);
  // After the first frame the frame-0 rule is gone: a hidden neutral and
  // one without a sprite are withheld.
  env.fx.units.rows = {unit_row(1, UT::Resource_Mineral_Field, 11, 10, 10),
                       unit_row(2, UT::Resource_Mineral_Field, 11, 12, 12)};
  env.fx.render.rows = {{unit_id(1), 10, 10, 0b00}};
  CHECK(env.project());
  CHECK(!env.unit(env.sample.units[0].id).exists);
  CHECK(!env.unit(env.sample.units[1].id).exists);
}

// The first frame's live units fill the 0..n-1 id block whatever their
// visibility is, and a new match restarts the ids from zero.
void initial_units_and_match_restart() {
  Env env;
  env.fx.local_player = 0;
  env.fx.units.rows = {unit_row(1, UT::Terran_Marine, 0, 10, 10),
                       unit_row(2, UT::Zerg_Zergling, 1, 20, 20)};
  env.fx.render.rows = {{unit_id(1), 10, 10, 0b01}, {unit_id(2), 20, 20, 0b00}};
  CHECK(env.project());
  CHECK(env.data->initialUnitCount == 2);
  CHECK(env.sample.units[0].id == 0 && env.sample.units[1].id == 1);
  CHECK(env.unit(0).exists && !env.unit(1).exists);
  // A new match renumbers from zero.
  env.fx.match_id = 2;
  env.fx.units.rows = {unit_row(3, UT::Terran_SCV, 0, 5, 5)};
  env.fx.render.rows = {{unit_id(3), 5, 5, 0b01}};
  CHECK(env.project());
  CHECK(env.data->initialUnitCount == 1 && env.sample.units[0].id == 0);
  CHECK(env.unit(0).type == UT::Terran_SCV);
}

// The engine's pool size bounds the records; a row past it has no slot.
void pool_size_bounds_records() {
  Env env;
  env.fx.local_player = 0;
  Fixture::GameRows::Row game;
  game.unit_slots = 4;
  env.fx.game.rows = {game};
  env.fx.units.rows = {unit_row(2, UT::Terran_Marine, 0, 10, 10),
                       unit_row(6, UT::Terran_Marine, 0, 20, 20)};
  CHECK(env.project());
  CHECK(env.sample.units.size() == 1);
  CHECK(env.data->unitArray[2] == env.sample.units[0].id);
  CHECK(env.data->initialUnitCount == 1);
}

// UnitUpdate.cpp for a live unit that leaves access (a mineral field fogged
// at another base): the evade frame's row is the official inaccessible
// defaults, and it stays so on later frames and across a full clear.
void evaded_unit_gets_the_inaccessible_defaults() {
  Env env;
  env.fx.local_player = 0;
  env.fx.players.rows = {player_row(0, 1), player_row(11, 4)};
  auto mineral = unit_row(1, UT::Resource_Mineral_Field_Type_2, 11, 100, 120);
  mineral.resources = 1500;
  mineral.resource_group = 3;
  mineral.being_gathered = 1;
  mineral.acid_spores = 2;
  mineral.invincible = 1;
  mineral.order = BWAPI::Orders::Enum::Nothing;
  mineral.order_timer = 5;
  mineral.button_set = 228;
  mineral.last_attacker = 1;
  env.fx.units.rows = {mineral};
  env.fx.render.rows = {{unit_id(1), 100, 120, 0b1}};
  CHECK(env.project());
  const int id = env.sample.units[0].id;
  {
    const auto& u = env.unit(id);
    CHECK(u.exists && u.isVisible[0] && u.isDetected && u.type == UT::Resource_Mineral_Field);
    CHECK(u.positionX == 100 && u.hitPoints == 40 && u.resources == 1500 && u.resourceGroup == 3);
    CHECK(u.isInvincible && u.acidSporeCount == 2 && u.isBeingGathered);
  }
  const auto check_defaults = [&](const BWAPI::UnitData& u) {
    CHECK(u.id == id && u.clearanceLevel == 0);
    CHECK(!u.exists && u.player == env.data->neutral && u.type == UT::Unknown);
    CHECK(u.positionX == BWAPI::Positions::Unknown.x && u.positionY == BWAPI::Positions::Unknown.y);
    CHECK(u.angle == 0 && u.velocityX == 0 && u.velocityY == 0);
    CHECK(u.hitPoints == 0 && u.lastHitPoints == 0 && u.shields == 0 && u.energy == 0);
    // Kept: resources, acid spores, isGathering, isInvincible.
    CHECK(u.resources == 1500 && u.acidSporeCount == 2 && u.isInvincible && !u.isGathering);
    CHECK(u.resourceGroup == 0 && u.killCount == 0);
    CHECK(u.scarabCount == 0 && u.interceptorCount == 0 && u.spiderMineCount == 0);
    CHECK(u.groundWeaponCooldown == 0 && u.airWeaponCooldown == 0 && u.spellCooldown == 0);
    CHECK(u.defenseMatrixPoints == 0 && u.defenseMatrixTimer == 0 && u.ensnareTimer == 0 &&
          u.irradiateTimer == 0 && u.lockdownTimer == 0 && u.maelstromTimer == 0 && u.orderTimer == 0 &&
          u.plagueTimer == 0 && u.removeTimer == 0 && u.stasisTimer == 0 && u.stimTimer == 0);
    CHECK(u.buildType == UT::None && u.trainingQueueCount == 0 && u.tech == BWAPI::TechTypes::Enum::None &&
          u.upgrade == BWAPI::UpgradeTypes::Enum::None);
    CHECK(u.remainingBuildTime == 0 && u.remainingTrainTime == 0 && u.remainingResearchTime == 0 &&
          u.remainingUpgradeTime == 0);
    CHECK(u.buildUnit == -1 && u.target == -1 && u.orderTarget == -1 && u.rallyUnit == -1 &&
          u.addon == -1 && u.nydusExit == -1 && u.powerUp == -1 && u.transport == -1 &&
          u.carrier == -1 && u.hatchery == -1);
    CHECK(u.targetPositionX == BWAPI::Positions::Unknown.x && u.targetPositionY == BWAPI::Positions::Unknown.y);
    CHECK(u.orderTargetPositionX == BWAPI::Positions::Unknown.x &&
          u.orderTargetPositionY == BWAPI::Positions::Unknown.y);
    CHECK(u.rallyPositionX == BWAPI::Positions::None.x && u.rallyPositionY == BWAPI::Positions::None.y);
    CHECK(u.order == BWAPI::Orders::Enum::Unknown && u.secondaryOrder == BWAPI::Orders::Enum::Unknown);
    CHECK(!u.hasNuke && !u.isAccelerating && !u.isAttacking && !u.isAttackFrame && !u.isBeingGathered &&
          !u.isBlind && !u.isBraking && !u.isBurrowed && u.carryResourceType == 0 && !u.isCloaked &&
          !u.isCompleted && !u.isConstructing && !u.isDetected && !u.isHallucination && !u.isIdle &&
          !u.isInterruptible && !u.isLifted && !u.isMorphing && !u.isMoving && !u.isParasited &&
          !u.isSelected && !u.isStartingAttack && !u.isStuck && !u.isTraining && !u.isUnderStorm &&
          !u.isUnderDarkSwarm && !u.isUnderDWeb && u.isPowered);
    for (const bool cell : u.isVisible) CHECK(!cell);
    CHECK(u.buttonset == UT::None && u.lastAttackerPlayer == -1 && !u.recentlyAttacked && u.replayID == 0);
  };
  // Fogged: alive but not accessible.
  env.fx.render.rows[0].visible_to = 0;
  CHECK(env.project());
  CHECK(env.sample.units.size() == 1 && !env.sample.units[0].accessible);
  check_defaults(env.unit(id));
  // Later frames and a full clear keep the same row.
  env.fx.units.rows[0].resources = 900;
  CHECK(env.project());
  check_defaults(env.unit(id));
  env.projection.invalidate_game_data();
  CHECK(env.project());
  check_defaults(env.unit(id));
  // Seen again: published from the table.
  env.fx.render.rows[0].visible_to = 0b1;
  CHECK(env.project());
  CHECK(env.unit(id).exists && env.unit(id).resources == 900 && env.unit(id).positionX == 100);
}

// A unit keeps its BWAPI id while its engine id stays; a reused slot is a new
// life with a new id, and ids are never reused.
void unit_ids() {
  Env env;
  env.fx.units.rows = {unit_row(4, UT::Terran_Marine, 0, 10, 10, 1)};
  CHECK(env.project());
  const int first = env.sample.units[0].id;
  CHECK(env.project());
  CHECK(env.sample.units[0].id == first);
  env.fx.units.rows = {unit_row(4, UT::Terran_Marine, 0, 12, 10, 2)};
  CHECK(env.project());
  const int second = env.sample.units[0].id;
  CHECK(second != first);
  CHECK(!env.unit(first).exists);
  CHECK(env.unit(first).positionX == 10);  // the last callback data stays
  CHECK(env.unit(second).exists);
  // The slot kept its record number and bumped its uniqueness.
  CHECK(env.data->unitArray[4] == second);
  CHECK(env.sample.units[0].replay_id == ((4 + 1) | (1 << 11)));
}

void completed_morph_and_idle() {
  Env env;
  auto egg = unit_row(1, UT::Zerg_Egg, 0, 10, 10);
  egg.order = OR::ZergBirth;
  egg.completed = 1;
  auto drone = unit_row(2, UT::Zerg_Drone, 0, 20, 20);
  drone.order = OR::DroneBuild;
  drone.secondary_order = OR::Nothing;
  drone.build_unit = 0;
  auto idle = unit_row(3, UT::Zerg_Zergling, 0, 30, 30);
  idle.order = OR::Stop;
  idle.secondary_order = OR::Nothing;
  idle.build_unit = 0;
  env.fx.units.rows = {egg, drone, idle};
  CHECK(env.project());
  const auto& e = env.unit(env.sample.units[0].id);
  CHECK(!e.isCompleted && e.isMorphing && e.isConstructing);
  CHECK(!env.sample.units[0].completed);
  const auto& d = env.unit(env.sample.units[1].id);
  CHECK(d.isConstructing && !d.isIdle);
  const auto& z = env.unit(env.sample.units[2].id);
  CHECK(z.isIdle && !z.isConstructing && !z.isMorphing);
}

void production_fields() {
  Env env;
  auto academy = unit_row(1, UT::Terran_Academy, 0, 10, 10);
  academy.researching = 1;
  academy.tech = BWAPI::TechTypes::Enum::Stim_Packs;
  academy.remaining_research_frames = 120;
  academy.upgrading = 0;
  auto barracks = unit_row(2, UT::Terran_Barracks, 0, 20, 20);
  barracks.researching = 0;
  barracks.upgrading = 0;
  barracks.train_queue_0 = UT::Terran_Marine;
  barracks.train_queue_1 = UT::Terran_Medic;
  barracks.remaining_train_frames = 33;
  env.fx.units.rows = {academy, barracks};
  CHECK(env.project());
  const auto& a = env.unit(env.sample.units[0].id);
  CHECK(a.tech == BWAPI::TechTypes::Enum::Stim_Packs && a.remainingResearchTime == 120);
  CHECK(a.upgrade == BWAPI::UpgradeTypes::Enum::None);
  const auto& b = env.unit(env.sample.units[1].id);
  CHECK(b.tech == BWAPI::TechTypes::Enum::None);
  CHECK(b.trainingQueueCount == 2 && b.trainingQueue[0] == UT::Terran_Marine &&
        b.trainingQueue[1] == UT::Terran_Medic);
  CHECK(b.isTraining && !b.isIdle);
  CHECK(b.remainingTrainTime == 33);
}

// UnitUpdate.cpp canAccessInside / canAccessDetected: a unit the BWAPI
// player does not own shows no production or private values outside
// CompleteMapInformation, and an undetected one no detected values either.
void enemy_unit_data_is_hidden() {
  Env env;
  constexpr std::uint32_t kRequiresDetection = 0x100;
  env.fx.local_player = 0;
  env.fx.players.rows = {player_row(0, 1), player_row(1, 1)};
  auto barracks = unit_row(1, UT::Terran_Barracks, 1, 100, 100);
  barracks.train_queue_0 = UT::Terran_Marine;
  barracks.remaining_train_frames = 30;
  barracks.rally_x = 5;
  barracks.rally_y = 6;
  barracks.order_timer = 4;
  // A cloaked Ghost on the move: visible, not detected.
  auto ghost = unit_row(2, UT::Terran_Ghost, 1, 200, 200);
  ghost.energy = 50 * 256;
  ghost.order_timer = 7;
  ghost.kill_count = 3;
  ghost.status_flags = kRequiresDetection;
  ghost.detected_by = 0;
  ghost.moving = 1;
  ghost.invincible = 0;
  env.fx.units.rows = {barracks, ghost};
  env.fx.render.rows = {{unit_id(1), 100, 100, 0b1}, {unit_id(2), 200, 200, 0b1}};
  CHECK(env.project());
  const auto& b = env.unit(env.sample.units[0].id);
  CHECK(b.exists && b.isTraining && b.orderTimer == 4 && b.hitPoints == 40);
  CHECK(b.trainingQueueCount == 0 && b.remainingTrainTime == 0 && b.energy == 0);
  CHECK(b.rallyPositionX == BWAPI::Positions::None.x && b.rallyPositionY == BWAPI::Positions::None.y);
  const auto& g = env.unit(env.sample.units[1].id);
  CHECK(g.exists && g.isVisible[0] && !g.isDetected && g.isMoving);
  CHECK(g.hitPoints == 0 && g.killCount == 0 && g.orderTimer == 0 && g.energy == 0);
  CHECK(g.order == BWAPI::Orders::Enum::Unknown && g.orderTarget == -1 &&
        g.targetPositionX == BWAPI::Positions::Unknown.x && g.buttonset == UT::None &&
        g.lastAttackerPlayer == -1 && g.isPowered);
  // CompleteMapInformation shows both.
  env.data->flags[BWAPI::Flag::CompleteMapInformation] = true;
  CHECK(env.project());
  CHECK(env.unit(env.sample.units[0].id).trainingQueueCount == 1 &&
        env.unit(env.sample.units[0].id).remainingTrainTime == 30 &&
        env.unit(env.sample.units[0].id).rallyPositionX == 5);
  CHECK(env.unit(env.sample.units[1].id).energy == 50 && env.unit(env.sample.units[1].id).hitPoints == 40 &&
        env.unit(env.sample.units[1].id).killCount == 3);
}

// UnitUpdate.cpp _getType: mineral field variants are Resource_Mineral_Field
// and a morphing building is the type it morphs into.
void public_unit_types() {
  Env env;
  auto mineral = unit_row(1, UT::Resource_Mineral_Field_Type_3, 11, 10, 10);
  auto hatchery = unit_row(2, UT::Zerg_Hatchery, 0, 50, 50);
  hatchery.order = BWAPI::Orders::Enum::ZergBuildingMorph;
  hatchery.build_type = UT::Zerg_Lair;
  env.fx.units.rows = {mineral, hatchery};
  CHECK(env.project());
  CHECK(env.unit(env.sample.units[0].id).type == UT::Resource_Mineral_Field);
  const auto& lair = env.unit(env.sample.units[1].id);
  CHECK(lair.type == UT::Zerg_Lair && lair.isMorphing && !lair.isCompleted);
}

// A Hatchery, Lair or Hive publishes its larva countdown as its train time;
// a hatchery without a countdown reads 0, not a training time.
void hatchery_train_time() {
  Env env;
  auto hatchery = unit_row(1, UT::Zerg_Hatchery, 0, 10, 10);
  hatchery.larva_frames = 336;
  auto lair = unit_row(2, UT::Zerg_Lair, 0, 20, 20);
  lair.remaining_train_frames = 50;
  env.fx.units.rows = {hatchery, lair};
  CHECK(env.project());
  CHECK(env.unit(env.sample.units[0].id).remainingTrainTime == 336);
  CHECK(env.unit(env.sample.units[1].id).remainingTrainTime == 0);
}

// UnitUpdate.cpp isVisible for the BWAPI player: the sprite bit, narrowed for
// a unit that requires detection; a Scanner Sweep by the tiles around it.
void detection_rules() {
  Env env;
  env.fx.local_player = 0;
  env.fx.players.rows = {player_row(0, 1), player_row(1, 2)};
  env.fx.map_width = 4;
  env.fx.map_height = 4;
  for (int tile = 0; tile < 16; ++tile) {
    Fixture::VisionRows::Row view;
    view.visible = tile == 15 ? 0b01 : 0;  // only tile (3, 3)
    view.explored = 0;
    view.creep = 0;
    view.occupied = 0;
    env.fx.vision.rows.push_back(view);
  }
  constexpr std::uint32_t kBurrowed = 0x10, kRequiresDetection = 0x100;
  // A burrowed lurker standing still: hidden without detection.
  auto lurker = unit_row(1, UT::Zerg_Lurker, 1, 20, 20);
  lurker.status_flags = kBurrowed | kRequiresDetection;
  lurker.detected_by = 0;
  lurker.moving = 0;
  lurker.ground_cooldown = 0;
  lurker.air_cooldown = 0;
  // A cloaked wraith: in sight but undetected.
  auto wraith = unit_row(2, UT::Terran_Wraith, 1, 30, 30);
  wraith.status_flags = kRequiresDetection;
  wraith.detected_by = 0;
  wraith.moving = 0;
  wraith.ground_cooldown = 0;
  wraith.air_cooldown = 0;
  // A sweep near tile (3, 3) and one far from any visible tile.
  auto near_sweep = unit_row(3, UT::Spell_Scanner_Sweep, 1, 70, 70);
  auto far_sweep = unit_row(4, UT::Spell_Scanner_Sweep, 1, 10, 10);
  // An own unit without a sprite is hidden even from its owner.
  auto spriteless = unit_row(5, UT::Terran_Marine, 0, 40, 40);
  env.fx.units.rows = {lurker, wraith, near_sweep, far_sweep, spriteless};
  env.fx.render.rows = {{unit_id(1), 20, 20, 0b11}, {unit_id(2), 30, 30, 0b11},
                        {unit_id(3), 70, 70, 0}, {unit_id(4), 10, 10, 0b11},
                        {unit_id(5), 40, 40, 0xffff}};
  CHECK(env.project());
  CHECK(!env.sample.units[0].accessible);
  CHECK(env.sample.units[1].accessible);
  const auto& cloaked = env.unit(env.sample.units[1].id);
  CHECK(cloaked.isVisible[0] && !cloaked.isDetected);
  CHECK(env.sample.units[2].accessible && env.unit(env.sample.units[2].id).isDetected);
  CHECK(!env.sample.units[3].accessible);
  CHECK(!env.sample.units[4].accessible);
  // A detected lurker is visible; so is a moving or attacking one.
  env.fx.units.rows[0].detected_by = 0b01;
  CHECK(env.project());
  CHECK(env.sample.units[0].accessible && env.unit(env.sample.units[0].id).isDetected);
  env.fx.units.rows[0].detected_by = 0;
  env.fx.units.rows[0].ground_cooldown = 5;
  CHECK(env.project());
  CHECK(env.sample.units[0].accessible && !env.unit(env.sample.units[0].id).isDetected);
  // The visible count holds the cloaked wraith and the lurker, not the
  // hidden units CompleteMapInformation publishes.
  env.data->flags[BWAPI::Flag::CompleteMapInformation] = true;
  CHECK(env.project());
  CHECK(env.sample.units[3].accessible);
  const auto& counts = env.data->players[1].visibleUnitCount;
  CHECK(counts[UT::Spell_Scanner_Sweep] == 1 && counts[UT::Zerg_Lurker] == 1 &&
        counts[UT::Terran_Wraith] == 1 && counts[UT::AllUnits] == 3);
  CHECK(env.data->players[0].visibleUnitCount[UT::AllUnits] == 0);
}

// A builder and what it constructs reference each other (augmentUnitData).
void builder_pairing() {
  Env env;
  auto scv = unit_row(1, UT::Terran_SCV, 0, 10, 10);
  scv.order = OR::ConstructingBuilding;
  scv.order_target = unit_id(2);
  auto building = unit_row(2, UT::Terran_Barracks, 0, 10, 20);
  building.completed = 0;
  building.order = OR::IncompleteBuilding;
  env.fx.units.rows = {scv, building};
  CHECK(env.project());
  const auto& builder = env.unit(env.sample.units[0].id);
  const auto& built = env.unit(env.sample.units[1].id);
  CHECK(builder.buildUnit == built.id && builder.isConstructing);
  CHECK(built.buildUnit == builder.id && built.isConstructing);
  CHECK(builder.buildType == UT::Terran_Barracks);
}

void dark_swarm() {
  Env env;
  auto swarm = unit_row(1, UT::Spell_Dark_Swarm, 11, 500, 500);
  auto inside = unit_row(2, UT::Zerg_Zergling, 0, 500, 500);
  auto flyer = unit_row(3, UT::Zerg_Mutalisk, 0, 500, 500);
  auto distant = unit_row(4, UT::Zerg_Zergling, 0, 1500, 1500);
  env.fx.units.rows = {swarm, inside, flyer, distant};
  CHECK(env.project());
  CHECK(env.unit(env.sample.units[1].id).isUnderDarkSwarm);
  CHECK(!env.unit(env.sample.units[2].id).isUnderDarkSwarm);
  CHECK(!env.unit(env.sample.units[3].id).isUnderDarkSwarm);
}

void replay_id_needs_complete_map_information() {
  Env env;
  env.fx.units.rows = {unit_row(9, UT::Terran_Marine, 0, 10, 10)};
  CHECK(env.project());
  CHECK(env.unit(env.sample.units[0].id).replayID == 0);
  env.data->flags[BWAPI::Flag::CompleteMapInformation] = true;
  CHECK(env.project());
  // A slot that was never reused has uniqueness 0.
  CHECK(env.unit(env.sample.units[0].id).replayID == 10);
}

// 3400 pool records map onto the 1700 official slots; own units keep theirs.
void slot_capacity() {
  Env env;
  env.fx.local_player = 0;
  for (unsigned slot = 0; slot < 1800; ++slot) {
    const bool own = slot < 10;
    env.fx.units.rows.push_back(unit_row(slot, UT::Terran_Marine, own ? 0 : 1, 10, 10));
    env.fx.render.rows.push_back({unit_id(slot), 10, 10, 0b11});
  }
  CHECK(env.project());
  CHECK(env.sample.units.size() == 1800);
  int indexed = 0, own_indexed = 0;
  for (const auto& unit : env.sample.units) {
    if (unit.unit_index >= 0) ++indexed;
    if (unit.owner == 0 && unit.unit_index >= 0) ++own_indexed;
  }
  CHECK(indexed == 1700 && own_indexed == 10);
  // The finder holds two edges per unit up to its official size.
  CHECK(env.data->unitSearchSize == 1700 * 2);
}

// The game's unit finder leaves out a worker inside a gas building (it holds
// HarvestGas only there), so the client's rectangle search does too.
void gas_worker_leaves_the_finder() {
  Env env;
  env.fx.local_player = 0;
  env.fx.players.rows.push_back(player_row(0, 1));
  auto outside = unit_row(1, UT::Terran_SCV, 0, 100, 100);
  outside.order = BWAPI::Orders::Enum::MoveToGas;
  auto inside = unit_row(2, UT::Terran_SCV, 0, 200, 100);
  inside.order = BWAPI::Orders::Enum::HarvestGas;
  env.fx.units.rows = {outside, inside};
  CHECK(env.project());
  CHECK(env.unit(env.sample.units[1].id).exists);
  CHECK(env.data->unitSearchSize == 2 &&
        env.data->xUnitSearch[0].unitIndex == env.sample.units[0].id);
}

}  // namespace

int main() {
  gas_worker_leaves_the_finder();
  values_and_unknown_cells();
  attack_state();
  resource_being_gathered();
  spellcaster_energy();
  relations();
  turret_cooldown();
  reaver_cooldown_is_order_timer();
  visibility_and_selection();
  unknown_sprite_visibility();
  accessibility_of_hidden_units();
  neutral_units_on_the_first_frame();
  initial_units_and_match_restart();
  pool_size_bounds_records();
  unit_ids();
  evaded_unit_gets_the_inaccessible_defaults();
  completed_morph_and_idle();
  production_fields();
  hatchery_train_time();
  enemy_unit_data_is_hidden();
  public_unit_types();
  detection_rules();
  builder_pairing();
  dark_swarm();
  replay_id_needs_complete_map_information();
  slot_capacity();
  return finish();
}
