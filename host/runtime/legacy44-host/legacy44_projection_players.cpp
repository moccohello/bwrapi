// PlayerData: players, unit counts, upgrades, techs and unit availability.
#include "legacy44_projection.h"
#include "legacy44_bwapi_rules.h"

#include <BWAPI/PlayerType.h>
#include <BWAPI/Position.h>

#include <algorithm>
#include <array>
#include <cstring>
#include <limits>

namespace BWRAPI::Legacy44 {

void GameDataProjection::project_players(const BwrFrame& frame, const FrameSample& sample,
                                         BWAPI::GameData& data) {
  const auto set = [&](auto& cell, auto value) {
    cell = static_cast<std::decay_t<decltype(cell)>>(value);
  };

  std::array<bool, kMaxPlayers> present{};
  // Slot type of the playable slots; -1 when the table leaves it unknown.
  std::array<int, kPlayablePlayerCount> slot_type{};
  slot_type.fill(-1);
  std::array<std::uint16_t, kPlayablePlayerCount> alliance{};
  std::array<bool, kPlayablePlayerCount> alliance_known{};
  std::array<std::uint8_t, kPlayablePlayerCount> start_participation{};
  start_participation.fill(std::numeric_limits<std::uint8_t>::max());
  const BwrPlayers* players = frame.players;
  if (players != nullptr) {
    for (std::size_t row = 0; row < players->rows; ++row) {
      const int slot = players->id[row];
      if (slot >= static_cast<int>(kMaxPlayers)) continue;
      present[slot] = true;
      if (slot >= static_cast<int>(kPlayablePlayerCount)) continue;
      if (known(players->player_type[row])) slot_type[slot] = players->player_type[row];
      alliance_known[slot] = known(players->allies[row]);
      alliance[slot] = alliance_known[slot] ? players->allies[row] : 0;
      start_participation[slot] = players->participating[row];
    }
  }

  // GameEvents.cpp onGameStart gives IDs to the playable slots whose type is
  // set and below Closed; the neutral player follows. A slot that left since
  // the start reads PlayerLeft or ComputerLeft and was in the set then, and a
  // slot whose type is unknown cannot be shown empty, so both keep their place.
  if (!player_ids_.known() && players != nullptr && players->rows != 0) {
    std::array<bool, kPlayablePlayerCount> listed{};
    for (std::size_t slot = 0; slot < kPlayablePlayerCount; ++slot) {
      const int type = slot_type[slot];
      listed[slot] = present[slot] &&
                     (type < 0 ||
                      (type > BWAPI::PlayerTypes::Enum::None && type < BWAPI::PlayerTypes::Enum::Closed) ||
                      type == BWAPI::PlayerTypes::Enum::PlayerLeft ||
                      type == BWAPI::PlayerTypes::Enum::ComputerLeft);
    }
    player_ids_.assign(listed);
  }
  // Server.cpp onMatchStart: playerCount is the size of the player vector.
  data.playerCount = player_ids_.count();

  // Server.cpp copies only the players of the set, each into the row of its ID.
  if (players != nullptr) {
    for (std::size_t row = 0; row < players->rows; ++row) {
      const int slot = players->id[row];
      const int id = player_ids_.listed(slot);
      if (id < 0) continue;
      auto& player = data.players[id];
      player.force = -1;
      if (known(players->race[row])) set(player.race, players->race[row]);
      if (known(players->minerals[row])) set(player.minerals, players->minerals[row]);
      if (known(players->gas[row])) set(player.gas, players->gas[row]);
      if (known(players->player_type[row])) set(player.type, players->player_type[row]);
      if (players->force[row] <= 4) set(player.force, players->force[row]);
      if (known(players->color[row])) set(player.color, players->color[row]);
      // Supply is in half units, per race: Zerg, Terran, Protoss.
      const std::uint32_t used[3] = {players->supply_used_zerg[row], players->supply_used_terran[row],
                                     players->supply_used_protoss[row]};
      const std::uint32_t total[3] = {players->supply_total_zerg[row], players->supply_total_terran[row],
                                      players->supply_total_protoss[row]};
      for (std::size_t race = 0; race < 3; ++race) {
        if (known(used[race])) set(player.supplyUsed[race], std::min<std::uint32_t>(used[race], INT32_MAX));
        if (known(total[race])) set(player.supplyTotal[race], std::min<std::uint32_t>(total[race], INT32_MAX));
      }
      if (known(players->start_x[row]) && known(players->start_y[row])) {
        set(player.startLocationX, players->start_x[row]);
        set(player.startLocationY, players->start_y[row]);
      }
      // The terminal victory state is defined only for the eight playable
      // slots, from the PlayerVictory byte table (PlayerImpl).
      const bool playable = slot < static_cast<int>(kPlayablePlayerCount);
      if (known(players->victory[row])) {
        const int victory = players->victory[row];
        set(player.isVictorious, playable && victory == 3);
        set(player.isDefeated, playable && (victory == 1 || victory == 2 || victory == 4 || victory == 6));
      }
      if (players->left_game[row] <= 1) set(player.leftGame, playable && players->left_game[row] == 1);
      // Brood War turns a leaving slot's type into PlayerLeft or ComputerLeft,
      // which PlayerImpl reads; SC:R keeps the type, so it is converted here.
      if (playable && players->left_game[row] == 1) {
        if (player.type == BWAPI::PlayerTypes::Enum::Player) set(player.type, BWAPI::PlayerTypes::Enum::PlayerLeft);
        if (player.type == BWAPI::PlayerTypes::Enum::Computer) set(player.type, BWAPI::PlayerTypes::Enum::ComputerLeft);
      }
      if (known(players->gathered_minerals[row])) set(player.gatheredMinerals, players->gathered_minerals[row]);
      if (known(players->gathered_gas[row])) set(player.gatheredGas, players->gathered_gas[row]);
      if (known(players->unit_score[row])) set(player.totalUnitScore, players->unit_score[row]);
      if (known(players->kill_score[row])) set(player.totalKillScore, players->kill_score[row]);
      if (known(players->building_score[row])) set(player.totalBuildingScore, players->building_score[row]);
      if (known(players->razing_score[row])) set(player.totalRazingScore, players->razing_score[row]);
      if (known(players->custom_score[row])) set(player.customScore, players->custom_score[row]);
      if (known(players->repaired_minerals[row])) set(player.repairedMinerals, players->repaired_minerals[row]);
      if (known(players->repaired_gas[row])) set(player.repairedGas, players->repaired_gas[row]);
      if (known(players->refunded_minerals[row])) set(player.refundedMinerals, players->refunded_minerals[row]);
      if (known(players->refunded_gas[row])) set(player.refundedGas, players->refunded_gas[row]);
      // Empty text is an unknown name; the name is at most 24 bytes.
      const std::size_t length = strnlen(players->name[row].text, sizeof(players->name[row].text));
      if (length != 0) {
        const std::size_t count = std::min<std::size_t>(length, sizeof(player.name) - 1);
        std::fill(std::begin(player.name), std::end(player.name), '\0');
        std::memcpy(player.name, players->name[row].text, count);
      }
    }
  }

  // The server's PlayerImpl::getName() is the fixed "Neutral" for slot 11 and
  // it is not a native slot name.
  data.neutral = player_ids_.listed(static_cast<int>(kNeutralPlayerId));
  if (data.neutral >= 0) {
    auto& neutral = data.players[data.neutral];
    static constexpr char kNeutralName[] = "Neutral";
    std::fill(std::begin(neutral.name), std::end(neutral.name), '\0');
    std::copy(std::begin(kNeutralName), std::end(kNeutralName), neutral.name);
    set(neutral.type, BWAPI::PlayerTypes::Enum::Neutral);
    set(neutral.isNeutral, true);
    set(neutral.force, 0);
  }

  // GameEvents.cpp onGameStart decides participation once per match: a
  // playable slot that owns a unit or that a trigger can give units to. The
  // engine reads that rule on game frame 0, for every occupied slot at once.
  // When the server did not see frame 0, a slot participates from the first
  // frame it is seen owning or having lost a unit; that matches the start
  // rule except for a slot whose units come only from a later trigger.
  if (!participation_known_ && players != nullptr && players->rows != 0 &&
      frame.game_frame == 0) {
    bool all_known = true;
    for (std::size_t slot = 0; slot < kPlayablePlayerCount; ++slot)
      if (present[slot] && !known(start_participation[slot])) all_known = false;
    if (all_known) {
      for (std::size_t slot = 0; slot < kPlayablePlayerCount; ++slot)
        participation_[slot] = present[slot] && start_participation[slot] == 1;
      participation_known_ = true;
    }
  }
  if (!participation_known_ && frame.unit_counts != nullptr) {
    const BwrUnitCounts& counts = *frame.unit_counts;
    for (std::size_t row = 0; row < counts.rows; ++row) {
      const std::uint8_t slot = counts.player[row];
      if (slot >= kPlayablePlayerCount || counts.unit_type[row] >= BWAPI::UnitTypes::Enum::None) continue;
      if ((known(counts.all[row]) && counts.all[row] != 0) ||
          (known(counts.dead[row]) && counts.dead[row] != 0))
        owned_units_[slot] = true;
    }
  }
  std::array<bool, kPlayablePlayerCount> participating{};
  for (std::size_t slot = 0; slot < kPlayablePlayerCount; ++slot) {
    const int id = player_ids_.listed(static_cast<int>(slot));
    if (id < 0) continue;
    participating[slot] =
        present[slot] && (participation_known_ ? participation_[slot] : owned_units_[slot]);
    data.players[id].isParticipating = participating[slot];
  }

  // PlayerImpl::isAlly/isEnemy: neither side neutral or an observer, then
  // the alliance matrix. Server.cpp copies both for every pair of the set,
  // indexed by ID. Only the playable slots can participate.
  const auto enemies = [&](std::size_t slot, std::size_t other) {
    return participating[slot] && participating[other] && alliance_known[slot] &&
           (alliance[slot] >> other & 1) == 0;
  };
  for (std::size_t slot = 0; slot < kPlayablePlayerCount; ++slot) {
    if (!participating[slot] || !alliance_known[slot]) continue;
    auto& player = data.players[player_ids_.listed(static_cast<int>(slot))];
    for (std::size_t other = 0; other < kPlayablePlayerCount; ++other) {
      if (!participating[other]) continue;
      const int other_id = player_ids_.listed(static_cast<int>(other));
      player.isAlly[other_id] = !enemies(slot, other);
      player.isEnemy[other_id] = enemies(slot, other);
    }
  }

  // The local player is the lease's own perspective once it names a released
  // player of the set. GameEvents.cpp sets no BWAPIPlayer in a replay, so
  // self stays unset there.
  const int self_slot = sample.local_player;
  const bool self_known = !sample.is_replay && self_slot >= 0 && self_slot < static_cast<int>(kPlayablePlayerCount) &&
                          present[self_slot] && player_ids_.listed(self_slot) >= 0;
  if (self_known) data.self = player_ids_.listed(self_slot);

  // GameEvents.cpp "find the opponent": the last playable slot of a playing
  // type that is not an observer and that self is an enemy of; a replay has
  // no enemy. The official server picks this once per match and keeps it.
  if (!enemy_known_) {
    if (sample.is_replay) {
      enemy_ = -1;
      enemy_known_ = true;
    } else if (self_known && alliance_known[self_slot]) {
      int enemy = -1;
      for (std::size_t slot = 0; slot < kPlayablePlayerCount; ++slot) {
        const int type = slot_type[slot];
        const bool playing = type == BWAPI::PlayerTypes::Enum::Computer ||
                             type == BWAPI::PlayerTypes::Enum::Player ||
                             type == BWAPI::PlayerTypes::Enum::EitherPreferComputer;
        if (present[slot] && playing && enemies(static_cast<std::size_t>(self_slot), slot))
          enemy = static_cast<int>(slot);
      }
      enemy_ = enemy;
      enemy_known_ = true;
    }
  }
  data.enemy = enemy_known_ ? player_ids_.listed(enemy_) : -1;

  // PlayerImpl::updateData: outside a replay and CompleteMapInformation the
  // BWAPI player's enemies show no resources, upgrades, techs, unit
  // availability, supply, scores or dead and killed counts; the neutral
  // player shows no resources, upgrades, techs or availability. An enemy
  // still shows the level of each upgrade a completed unit of its counts
  // uses. Server.cpp onMatchStart takes start locations while every flag is
  // off, so an enemy's is TilePositions::Unknown outside a replay.
  const bool complete_map = data.flags[BWAPI::Flag::CompleteMapInformation];
  std::array<bool, kMaxPlayers> enemy{};
  std::array<bool, kMaxPlayers> hidden{};
  for (std::size_t slot = 0; slot < kMaxPlayers; ++slot) {
    const int id = player_ids_.listed(static_cast<int>(slot));
    if (id < 0) continue;
    const bool self_enemy = !sample.is_replay && data.self >= 0 && data.players[data.self].isEnemy[id];
    enemy[id] = self_enemy && !complete_map;
    hidden[id] = enemy[id] || slot == kNeutralPlayerId;
    auto& player = data.players[id];
    if (self_enemy) {
      player.startLocationX = BWAPI::TilePositions::Unknown.x;
      player.startLocationY = BWAPI::TilePositions::Unknown.y;
    }
    if (hidden[id]) {
      player.minerals = player.gas = player.gatheredMinerals = player.gatheredGas = 0;
      player.repairedMinerals = player.repairedGas = player.refundedMinerals = player.refundedGas = 0;
    }
    if (enemy[id]) {
      std::fill(std::begin(player.supplyUsed), std::end(player.supplyUsed), 0);
      std::fill(std::begin(player.supplyTotal), std::end(player.supplyTotal), 0);
      player.totalUnitScore = player.totalKillScore = player.totalBuildingScore = 0;
      player.totalRazingScore = player.customScore = 0;
    }
  }

  // Dead and killed counts; a missing row has every count zero. All,
  // completed and visible counts come from the published units
  // (project_units).
  if (frame.unit_counts != nullptr) {
    const BwrUnitCounts& counts = *frame.unit_counts;
    for (std::size_t row = 0; row < counts.rows; ++row) {
      const int id = player_ids_.listed(counts.player[row]);
      const std::uint16_t type = counts.unit_type[row];
      if (id < 0 || enemy[id] || type >= BWAPI::UnitTypes::Enum::MAX) continue;
      auto& player = data.players[id];
      const auto count = [&](int& cell, std::uint32_t value) {
        if (known(value)) cell = static_cast<int>(std::min<std::uint32_t>(value, INT32_MAX));
      };
      count(player.killedUnitCount[type], counts.killed[row]);
      count(player.deadUnitCount[type], counts.dead[row]);
    }
  }

  // Which unit types each player may build; Nuclear Strike follows the
  // production permission of the missile (PlayerImpl::update).
  if (frame.unit_availability != nullptr) {
    const BwrUnitAvailability& availability = *frame.unit_availability;
    for (std::size_t row = 0; row < availability.rows; ++row) {
      const int id = player_ids_.listed(availability.player[row]);
      const std::uint16_t type = availability.unit_type[row];
      if (id < 0 || hidden[id] || type >= BWAPI::UnitTypes::Enum::MAX ||
          availability.available[row] > 1)
        continue;
      auto& player = data.players[id];
      set(player.isUnitAvailable[type], availability.available[row] == 1);
      if (type == BWAPI::UnitTypes::Enum::Terran_Nuclear_Missile)
        set(player.hasResearched[BWAPI::TechTypes::Enum::Nuclear_Strike], availability.available[row] == 1);
    }
  }

  if (frame.upgrades != nullptr) {
    const BwrUpgrades& upgrades = *frame.upgrades;
    for (std::size_t row = 0; row < upgrades.rows; ++row) {
      const int id = player_ids_.listed(upgrades.player[row]);
      const std::uint16_t type = upgrades.upgrade_type[row];
      if (id < 0 || type >= BWAPI::UpgradeTypes::Enum::MAX) continue;
      auto& player = data.players[id];
      if (hidden[id]) {
        if (enemy[id] && type < kUpgradeTypeCount && upgrade_users_[id][type] && known(upgrades.level[row]))
          set(player.upgradeLevel[type], upgrades.level[row]);
        continue;
      }
      if (known(upgrades.level[row])) set(player.upgradeLevel[type], upgrades.level[row]);
      if (known(upgrades.max_level[row])) set(player.maxUpgradeLevel[type], upgrades.max_level[row]);
      if (upgrades.in_progress[row] <= 1) set(player.isUpgrading[type], upgrades.in_progress[row] == 1);
    }
  }
  if (frame.techs != nullptr) {
    const BwrTechs& techs = *frame.techs;
    for (std::size_t row = 0; row < techs.rows; ++row) {
      const int id = player_ids_.listed(techs.player[row]);
      const std::uint16_t type = techs.tech_type[row];
      if (id < 0 || hidden[id] || type >= BWAPI::TechTypes::Enum::MAX) continue;
      auto& player = data.players[id];
      if (techs.researched[row] <= 1)
        set(player.hasResearched[type],
            bwapi_tech_requires_no_research(type) || techs.researched[row] == 1);
      if (techs.in_progress[row] <= 1) set(player.isResearching[type], techs.in_progress[row] == 1);
      if (techs.available[row] <= 1) set(player.isResearchAvailable[type], techs.available[row] == 1);
    }
  }
}

}  // namespace BWRAPI::Legacy44
