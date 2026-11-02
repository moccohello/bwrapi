#include "legacy44_events.h"

#include <algorithm>
#include <cstring>

#include <BWAPI/Flag.h>
#include <BWAPI/UnitType.h>

namespace BWRAPI::Legacy44 {

void EventFrameAdapter::reset() {
  match_active_ = false;
  match_id_ = 0;
  pending_lifecycle_ = PendingLifecycle::None;
  last_terminal_winner_known_ = false;
  last_terminal_winner_ = false;
  units_.clear();
  unit_epoch_.clear();
  epoch_ = 1;
  pending_.clear();
  secondary_.clear();
  missing_.clear();
  chat_.clear();
  saves_.clear();
  players_.clear();
  player_left_.fill(255);
}

void EventFrameAdapter::queue_chat(bool sent, std::uint32_t player, std::string text) {
  // Server::addString keeps the text in a 256-byte event string.
  if (text.size() > 255) text.resize(255);
  chat_.push_back({sent, player, std::move(text)});
}

void EventFrameAdapter::queue_save(std::string name) {
  if (name.size() > 255) name.resize(255);
  saves_.push_back(std::move(name));
}

// GameImpl::onSaveGame queues SaveGame when the game copies the save name,
// ahead of the next update; Server::addEvent keeps the name in an event
// string.
bool EventFrameAdapter::stage_saves(BWAPI::GameData& data, std::vector<PendingEvent>& events,
                                    std::string& error) {
  for (const auto& name : saves_) {
    if (data.eventStringCount >= BWAPI::GameData::MAX_EVENT_STRINGS) {
      error = "official GameData event string buffer exhausted";
      return false;
    }
    const int index = data.eventStringCount++;
    std::memcpy(data.eventStrings[index], name.c_str(), name.size() + 1);
    events.push_back({BWAPI::EventType::SaveGame, index, 0});
  }
  saves_.clear();
  return true;
}

bool EventFrameAdapter::stage_chat(BWAPI::GameData& data, bool sent,
                                   std::vector<PendingEvent>& events, std::string& error) {
  for (const auto& line : chat_) {
    if (line.sent != sent) continue;
    int player = -1;
    if (!sent) {
      // GameImpl::onReceiveText: another player's line, while the client
      // takes user input.
      if (!data.flags[BWAPI::Flag::UserInput]) continue;
      player = players_.listed(static_cast<int>(line.player));
      if (player < 0 || player == data.self) continue;
    }
    if (line.text.empty()) continue;
    if (data.eventStringCount >= BWAPI::GameData::MAX_EVENT_STRINGS) {
      error = "official GameData event string buffer exhausted";
      return false;
    }
    const int index = data.eventStringCount++;
    std::memcpy(data.eventStrings[index], line.text.c_str(), line.text.size() + 1);
    if (sent)
      events.push_back({BWAPI::EventType::SendText, index, 0});
    else
      events.push_back({BWAPI::EventType::ReceiveText, player, index});
  }
  return true;
}

EventFrameAdapter::TrackedUnit* EventFrameAdapter::tracked(int id) {
  return id >= 0 && static_cast<std::size_t>(id) < units_.size() &&
                 unit_epoch_[id] == epoch_
             ? &units_[id]
             : nullptr;
}

bool EventFrameAdapter::add(BWAPI::GameData& data, BWAPI::EventType::Enum type,
                            std::string& error, int v1, int v2) {
  if (data.eventCount >= BWAPI::GameData::MAX_EVENTS) {
    error = "official GameData event buffer exhausted";
    return false;
  }
  auto& event = data.events[data.eventCount++];
  event.type = type;
  event.v1 = v1;
  event.v2 = v2;
  return true;
}

bool EventFrameAdapter::stage_match_end(BWAPI::GameData& data, std::string& error) {
  data.eventCount = 0;
  data.eventStringCount = 0;
  chat_.clear();
  saves_.clear();
  // The official server's onGameEnd sends MatchEnd(false) when no earlier
  // victory/defeat ended the match. False means no reported win, including a
  // manual exit; it does not manufacture a defeated player state. The old
  // GameData stays for this final callback; advance_pending_lifecycle() then
  // moves to MenuFrame before any next-match frame is projected.
  data.isInGame = true;
  if (!add(data, BWAPI::EventType::MatchFrame, error) ||
      !add(data, BWAPI::EventType::MatchEnd, error,
           last_terminal_winner_known_ && last_terminal_winner_ ? 1 : 0))
    return false;
  pending_lifecycle_ = PendingLifecycle::Menu;
  return true;
}

bool EventFrameAdapter::advance_pending_lifecycle(BWAPI::GameData& data, bool& advanced,
                                                  std::string& error) {
  advanced = false;
  if (pending_lifecycle_ != PendingLifecycle::Menu) return true;
  data.eventCount = 0;
  data.eventStringCount = 0;
  data.isInGame = false;
  if (!add(data, BWAPI::EventType::MenuFrame, error)) return false;
  pending_lifecycle_ = PendingLifecycle::None;
  match_active_ = false;
  units_.clear();
  unit_epoch_.clear();
  advanced = true;
  return true;
}

bool EventFrameAdapter::stage_menu_frame(BWAPI::GameData& data, std::string& error) {
  data.eventCount = 0;
  data.eventStringCount = 0;
  return add(data, BWAPI::EventType::MenuFrame, error);
}

bool EventFrameAdapter::stage_match_frame(BWAPI::GameData& data, std::string& error) {
  data.eventCount = 0;
  data.eventStringCount = 0;
  pending_.clear();
  if (!stage_saves(data, pending_, error) || !stage_chat(data, false, pending_, error) ||
      !stage_chat(data, true, pending_, error))
    return false;
  chat_.clear();
  pending_.push_back({BWAPI::EventType::MatchFrame});
  for (const auto& event : pending_)
    if (!add(data, event.type, error, event.v1, event.v2)) return false;
  return true;
}

// GameUnits.cpp computePrimaryUnitSets/computeSecondaryUnitSets as a diff
// against the previous delivered frame. The engine table lists every live
// unit, so a row missing from the new frame is dead: it was alive, but only a
// unit that was still accessible is evaded and destroyed. A unit that stays
// alive but loses accessibility is evaded; one that gains it is discovered.
void EventFrameAdapter::diff_units(const FrameSample& frame,
                                   std::vector<PendingEvent>& primary,
                                   std::vector<PendingEvent>& secondary) {
  const auto push = [](std::vector<PendingEvent>& events, BWAPI::EventType::Enum type, int id) {
    events.push_back({type, id, 0});
  };
  ++epoch_;
  for (const auto& unit : frame.units) {
    const auto index = static_cast<std::size_t>(unit.id);
    if (index >= units_.size()) {
      units_.resize(index + 1);
      unit_epoch_.resize(index + 1, 0);
    }
    const bool known = unit_epoch_[index] == epoch_ - 1;
    TrackedUnit& tracked = units_[index];
    if (!known) tracked = TrackedUnit{};
    unit_epoch_[index] = epoch_;
    if (unit.accessible) {
      if (!tracked.was_alive) push(primary, BWAPI::EventType::UnitCreate, unit.id);
      if (!tracked.was_completed && unit.completed) {
        push(primary, BWAPI::EventType::UnitComplete, unit.id);
        tracked.was_completed = true;
      }
      if (!tracked.was_accessible) push(primary, BWAPI::EventType::UnitDiscover, unit.id);
      if (unit.visible != tracked.was_visible)
        push(primary, unit.visible ? BWAPI::EventType::UnitShow : BWAPI::EventType::UnitHide,
             unit.id);
      tracked.was_visible = unit.visible;
      tracked.was_accessible = true;
    } else {
      if (tracked.was_accessible) {
        if (tracked.was_visible) push(primary, BWAPI::EventType::UnitHide, unit.id);
        tracked.was_visible = false;
        push(primary, BWAPI::EventType::UnitEvade, unit.id);
      }
      tracked.was_accessible = false;
    }
    tracked.was_alive = true;
  }

  missing_.clear();
  for (std::size_t id = 0; id < units_.size(); ++id)
    if (unit_epoch_[id] == epoch_ - 1) missing_.push_back(static_cast<int>(id));
  std::sort(missing_.begin(), missing_.end());
  for (const int id : missing_) {
    const TrackedUnit& tracked = units_[id];
    if (tracked.was_accessible) {
      if (tracked.was_visible) push(primary, BWAPI::EventType::UnitHide, id);
      push(primary, BWAPI::EventType::UnitEvade, id);
      push(primary, BWAPI::EventType::UnitDestroy, id);
    }
    unit_epoch_[id] = 0;
  }

  for (const auto& unit : frame.units) {
    const TrackedUnit* tracked = this->tracked(unit.id);
    if (tracked == nullptr || !unit.accessible) continue;
    const int unknown = BWAPI::UnitTypes::Enum::Unknown;
    if (tracked->last_type >= 0 && tracked->last_type != unknown && unit.type != unknown &&
        tracked->last_type != unit.type) {
      // After morphing, units mostly are not completed.
      units_[unit.id].was_completed = false;
      push(secondary, BWAPI::EventType::UnitMorph, unit.id);
    }
    if (tracked->last_owner >= 0 && unit.owner >= 0 && tracked->last_owner != unit.owner)
      push(secondary, BWAPI::EventType::UnitRenegade, unit.id);
    units_[unit.id].last_type = unit.type;
    units_[unit.id].last_owner = unit.owner;
  }
}

// A player who leaves the game: left_game turns from 0 to 1. The event names
// the player's ID.
void EventFrameAdapter::diff_players(const FrameSample& frame, const BwrFrame& source,
                                     bool starting_match, std::vector<PendingEvent>& events) {
  std::array<std::uint8_t, 8> now;
  now.fill(255);
  if (source.players != nullptr) {
    for (std::size_t row = 0; row < source.players->rows; ++row) {
      const std::uint8_t id = source.players->id[row];
      if (id < now.size() && source.players->left_game[row] <= 1) now[id] = source.players->left_game[row];
    }
  }
  for (std::size_t slot = 0; slot < now.size(); ++slot) {
    const int id = frame.players.listed(static_cast<int>(slot));
    if (!starting_match && player_left_[slot] == 0 && now[slot] == 1 && id >= 0)
      events.push_back({BWAPI::EventType::PlayerLeft, id, 0});
    if (now[slot] != 255) player_left_[slot] = now[slot];
  }
}

void EventFrameAdapter::remember_frame(const FrameSample& frame, const BWAPI::GameData& data) {
  if (frame.match_started) {
    last_terminal_winner_known_ = false;
    last_terminal_winner_ = false;
  }
  // GameUpdate builds Event::MatchEnd(bool) from the local PlayerImpl's
  // terminal PlayerVictory state. Keep that result until the host emits the
  // terminal lifecycle callback, because the following menu frame has no
  // projected player table.
  if (data.self >= 0 && data.self < static_cast<int>(kMaxPlayers)) {
    const auto& local = data.players[data.self];
    if (local.isVictorious || local.isDefeated) {
      last_terminal_winner_known_ = true;
      last_terminal_winner_ = local.isVictorious;
    }
  }
}

bool EventFrameAdapter::append(const FrameSample& frame, const BwrFrame& source,
                               BWAPI::GameData& data, std::string& error) {
  const bool starting_match = !match_active_;
  if (match_active_ && frame.match_id != match_id_) {
    error = "match boundary reached projection before terminal lifecycle staging";
    return false;
  }
  if (starting_match) {
    units_.clear();
    unit_epoch_.clear();
    player_left_.fill(255);
  }
  pending_.clear();
  if (starting_match) {
    pending_.push_back({BWAPI::EventType::MatchStart});
    match_active_ = true;
    match_id_ = frame.match_id;
  }
  // The match's player IDs as this frame listed them, for the chat lines a
  // staged round maps to BWAPI players.
  players_ = frame.players;
  // GameImpl queues a save and a received line when the game takes them,
  // before the update that lists players and units, and the intercepted sent
  // lines after updateUnits.
  if (!stage_saves(data, pending_, error) || !stage_chat(data, false, pending_, error)) return false;
  diff_players(frame, source, starting_match, pending_);
  // GameImpl::updateUnits order: primary sets, NukeDetect from
  // extractUnitData, then Morph/Renegade from the secondary sets.
  secondary_.clear();
  diff_units(frame, pending_, secondary_);
  for (const auto& [x, y] : frame.nuke_detects)
    pending_.push_back({BWAPI::EventType::NukeDetect, x, y});
  pending_.insert(pending_.end(), secondary_.begin(), secondary_.end());
  if (!stage_chat(data, true, pending_, error)) return false;
  chat_.clear();
  pending_.push_back({BWAPI::EventType::MatchFrame});
  if (pending_.size() > static_cast<std::size_t>(BWAPI::GameData::MAX_EVENTS - data.eventCount)) {
    error = "official GameData event buffer exhausted";
    return false;
  }
  for (const auto& event : pending_)
    if (!add(data, event.type, error, event.v1, event.v2)) return false;
  remember_frame(frame, data);
  return true;
}

}  // namespace BWRAPI::Legacy44
