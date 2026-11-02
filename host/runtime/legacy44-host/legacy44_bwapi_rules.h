#pragma once
// BWAPI 4.4 unit-type, tech and order rules the host applies on top of the
// engine tables; they reproduce the entries of the official BWAPILIB tables.
#include <BWAPI/Order.h>
#include <BWAPI/TechType.h>
#include <BWAPI/UnitType.h>
#include <BWAPI/UpgradeType.h>

#include <limits>
#include <type_traits>

namespace BWRAPI::Legacy44 {

// Engine tables mark an unknown cell with the type maximum (unsigned) or
// minimum (signed); a flag is 0, 1 or unknown.
template <typename T>
constexpr bool known(T value) {
  if constexpr (std::is_signed_v<T>) return value != std::numeric_limits<T>::min();
  else return value != std::numeric_limits<T>::max();
}

inline bool is_bwapi_flyer_type(int type) {
  // Exact BWAPI 4.4 UnitPrototypeFlags::Flyer entries.
  switch (type) {
    case BWAPI::UnitTypes::Enum::Terran_Wraith:
    case BWAPI::UnitTypes::Enum::Terran_Science_Vessel:
    case BWAPI::UnitTypes::Enum::Terran_Dropship:
    case BWAPI::UnitTypes::Enum::Terran_Battlecruiser:
    case BWAPI::UnitTypes::Enum::Terran_Nuclear_Missile:
    case BWAPI::UnitTypes::Enum::Hero_Tom_Kazansky:
    case BWAPI::UnitTypes::Enum::Hero_Magellan:
    case BWAPI::UnitTypes::Enum::Hero_Arcturus_Mengsk:
    case BWAPI::UnitTypes::Enum::Hero_Hyperion:
    case BWAPI::UnitTypes::Enum::Hero_Norad_II:
    case BWAPI::UnitTypes::Enum::Spell_Scanner_Sweep:
    case BWAPI::UnitTypes::Enum::Zerg_Overlord:
    case BWAPI::UnitTypes::Enum::Zerg_Mutalisk:
    case BWAPI::UnitTypes::Enum::Zerg_Guardian:
    case BWAPI::UnitTypes::Enum::Zerg_Queen:
    case BWAPI::UnitTypes::Enum::Zerg_Scourge:
    case BWAPI::UnitTypes::Enum::Hero_Matriarch:
    case BWAPI::UnitTypes::Enum::Hero_Kukulza_Mutalisk:
    case BWAPI::UnitTypes::Enum::Hero_Kukulza_Guardian:
    case BWAPI::UnitTypes::Enum::Hero_Yggdrasill:
    case BWAPI::UnitTypes::Enum::Terran_Valkyrie:
    case BWAPI::UnitTypes::Enum::Zerg_Cocoon:
    case BWAPI::UnitTypes::Enum::Protoss_Corsair:
    case BWAPI::UnitTypes::Enum::Zerg_Devourer:
    case BWAPI::UnitTypes::Enum::Protoss_Shuttle:
    case BWAPI::UnitTypes::Enum::Protoss_Scout:
    case BWAPI::UnitTypes::Enum::Protoss_Arbiter:
    case BWAPI::UnitTypes::Enum::Protoss_Carrier:
    case BWAPI::UnitTypes::Enum::Protoss_Interceptor:
    case BWAPI::UnitTypes::Enum::Hero_Mojo:
    case BWAPI::UnitTypes::Enum::Hero_Gantrithor:
    case BWAPI::UnitTypes::Enum::Protoss_Observer:
    case BWAPI::UnitTypes::Enum::Hero_Danimoth:
    case BWAPI::UnitTypes::Enum::Hero_Artanis:
    case BWAPI::UnitTypes::Enum::Critter_Kakaru:
    case BWAPI::UnitTypes::Enum::Hero_Raszagal:
    case BWAPI::UnitTypes::Enum::Special_Map_Revealer:
    case BWAPI::UnitTypes::Enum::Hero_Gerard_DuGalle:
      return true;
    default:
      return false;
  }
}

inline bool is_bwapi_subunit_type(int type) {
  // Exact Subunit entries in the BWAPI 4.4 UnitPrototypeFlags table.
  switch (type) {
    case BWAPI::UnitTypes::Enum::Terran_Goliath_Turret:
    case BWAPI::UnitTypes::Enum::Terran_Siege_Tank_Tank_Mode_Turret:
    case BWAPI::UnitTypes::Enum::Hero_Alan_Schezar_Turret:
    case BWAPI::UnitTypes::Enum::Hero_Edmund_Duke_Tank_Mode_Turret:
    case BWAPI::UnitTypes::Enum::Hero_Edmund_Duke_Siege_Mode_Turret:
    case BWAPI::UnitTypes::Enum::Terran_Siege_Tank_Siege_Mode_Turret:
      return true;
    default:
      return false;
  }
}

inline bool is_bwapi_building_type(int type) {
  // Exact Building entries in the BWAPI 4.4 UnitPrototypeFlags table.
  return (type >= BWAPI::UnitTypes::Enum::Terran_Command_Center &&
          type <= BWAPI::UnitTypes::Enum::Special_Ion_Cannon) ||
         (type >= BWAPI::UnitTypes::Enum::Zerg_Infested_Command_Center &&
          type <= BWAPI::UnitTypes::Enum::Special_Overmind_Cocoon) ||
         type == BWAPI::UnitTypes::Enum::Special_Start_Location;
}

inline bool is_bwapi_factory_type(int type) {
  // BWAPI counts buildings that canProduce() or produceLarva().
  switch (type) {
    case BWAPI::UnitTypes::Enum::Terran_Command_Center:
    case BWAPI::UnitTypes::Enum::Terran_Barracks:
    case BWAPI::UnitTypes::Enum::Terran_Factory:
    case BWAPI::UnitTypes::Enum::Terran_Starport:
    case BWAPI::UnitTypes::Enum::Zerg_Infested_Command_Center:
    case BWAPI::UnitTypes::Enum::Zerg_Hatchery:
    case BWAPI::UnitTypes::Enum::Zerg_Lair:
    case BWAPI::UnitTypes::Enum::Zerg_Hive:
    case BWAPI::UnitTypes::Enum::Protoss_Nexus:
    case BWAPI::UnitTypes::Enum::Protoss_Robotics_Facility:
    case BWAPI::UnitTypes::Enum::Protoss_Gateway:
    case BWAPI::UnitTypes::Enum::Protoss_Stargate:
      return true;
    default:
      return false;
  }
}

inline bool is_bwapi_spell_type(int type) {
  // Exact BWAPI 4.4 UnitType::isSpell definition.
  return type == BWAPI::UnitTypes::Enum::Spell_Dark_Swarm ||
         type == BWAPI::UnitTypes::Enum::Spell_Disruption_Web ||
         type == BWAPI::UnitTypes::Enum::Spell_Scanner_Sweep;
}

inline bool bwapi_tech_requires_no_research(int type) {
  // The techs whose BWAPI 4.4 TechType.cpp whatResearches is None (tech table
  // 0..43).
  switch (type) {
    case BWAPI::TechTypes::Enum::Scanner_Sweep:
    case BWAPI::TechTypes::Enum::Defensive_Matrix:
    case BWAPI::TechTypes::Enum::Infestation:
    case BWAPI::TechTypes::Enum::Dark_Swarm:
    case BWAPI::TechTypes::Enum::Parasite:
    case BWAPI::TechTypes::Enum::Archon_Warp:
    case BWAPI::TechTypes::Enum::Unused_26:
    case BWAPI::TechTypes::Enum::Dark_Archon_Meld:
    case BWAPI::TechTypes::Enum::Feedback:
      return true;
    default:
      return type >= BWAPI::TechTypes::Enum::Unused_33 &&
             type < BWAPI::TechTypes::Enum::None;
  }
}

inline double direction_to_angle(int raw) {
  // This is the conversion used by the upstream BWAPI UnitUpdate path:
  // direction1=64 is north and one raw step is pi/128 radians.
  constexpr double pi = 3.14159265358979323846;
  return (static_cast<double>(raw) - 64.0) * pi / 128.0;
}

inline bool is_liftable_terran_building(int type) {
  switch (type) {
    case BWAPI::UnitTypes::Enum::Terran_Command_Center:
    case BWAPI::UnitTypes::Enum::Terran_Comsat_Station:
    case BWAPI::UnitTypes::Enum::Terran_Nuclear_Silo:
    case BWAPI::UnitTypes::Enum::Terran_Science_Facility:
    case BWAPI::UnitTypes::Enum::Terran_Covert_Ops:
    case BWAPI::UnitTypes::Enum::Terran_Physics_Lab:
    case BWAPI::UnitTypes::Enum::Terran_Starport:
    case BWAPI::UnitTypes::Enum::Terran_Control_Tower:
    case BWAPI::UnitTypes::Enum::Terran_Factory:
    case BWAPI::UnitTypes::Enum::Terran_Machine_Shop:
    case BWAPI::UnitTypes::Enum::Terran_Barracks:
    case BWAPI::UnitTypes::Enum::Terran_Academy:
    case BWAPI::UnitTypes::Enum::Terran_Engineering_Bay:
    case BWAPI::UnitTypes::Enum::Terran_Armory:
      return true;
    default:
      return false;
  }
}

// BWAPILIB UnitType.cpp unitFlags with the Spellcaster bit (0x00200000).
inline bool is_bwapi_spellcaster(int type) {
  switch (type) {
    case 1: case 8: case 9: case 12: case 16: case 21: case 22: case 27: case 28:
    case 29: case 34: case 45: case 46: case 49: case 51: case 52: case 60: case 63:
    case 67: case 71: case 79: case 86: case 87: case 98: case 99: case 100:
    case 102: case 104: case 107: case 172:
      return true;
    default:
      return false;
  }
}

inline bool is_protoss_building(int type) {
  return type >= BWAPI::UnitTypes::Enum::Protoss_Nexus &&
         type <= BWAPI::UnitTypes::Enum::Protoss_Shield_Battery;
}

inline bool is_spell_affected_unit(int type, bool lifted) {
  return !is_bwapi_spell_type(type) && !is_bwapi_flyer_type(type) && !lifted;
}

inline bool is_bwapi_idle_order(int order) {
  switch (order) {
    case BWAPI::Orders::Enum::PlayerGuard:
    case BWAPI::Orders::Enum::Guard:
    case BWAPI::Orders::Enum::Stop:
    case BWAPI::Orders::Enum::PickupIdle:
    case BWAPI::Orders::Enum::Nothing:
    case BWAPI::Orders::Enum::Medic:
    case BWAPI::Orders::Enum::Carrier:
    case BWAPI::Orders::Enum::Reaver:
    case BWAPI::Orders::Enum::Critter:
    case BWAPI::Orders::Enum::Neutral:
    case BWAPI::Orders::Enum::TowerGuard:
    case BWAPI::Orders::Enum::Burrowed:
    case BWAPI::Orders::Enum::NukeTrain:
    case BWAPI::Orders::Enum::Larva:
      return true;
    default:
      return false;
  }
}

inline bool is_morphing_order(int order) {
  switch (order) {
    case BWAPI::Orders::Enum::ZergBirth:
    case BWAPI::Orders::Enum::ZergBuildingMorph:
    case BWAPI::Orders::Enum::ZergUnitMorph:
    case BWAPI::Orders::Enum::IncompleteMorphing:
      return true;
    default:
      return false;
  }
}

inline bool is_constructing_order(int order) {
  switch (order) {
    case BWAPI::Orders::Enum::ConstructingBuilding:
    case BWAPI::Orders::Enum::PlaceBuilding:
    case BWAPI::Orders::Enum::DroneBuild:
    case BWAPI::Orders::Enum::DroneStartBuild:
    case BWAPI::Orders::Enum::DroneLand:
    case BWAPI::Orders::Enum::PlaceProtossBuilding:
    case BWAPI::Orders::Enum::CreateProtossBuilding:
    case BWAPI::Orders::Enum::IncompleteBuilding:
    case BWAPI::Orders::Enum::IncompleteWarping:
    case BWAPI::Orders::Enum::IncompleteMorphing:
    case BWAPI::Orders::Enum::BuildNydusExit:
    case BWAPI::Orders::Enum::BuildAddon:
      return true;
    default:
      return false;
  }
}

// Reaver and Warbringer fire from the order timer, not a weapon cooldown.
inline bool uses_order_timer_as_ground_cooldown(int type) {
  return type == BWAPI::UnitTypes::Enum::Protoss_Reaver ||
         type == BWAPI::UnitTypes::Enum::Hero_Warbringer;
}

}  // namespace BWRAPI::Legacy44
