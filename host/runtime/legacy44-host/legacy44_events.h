#pragma once

#include "legacy44_projection.h"

#include <BWAPI/EventType.h>

#include <array>
#include <cstddef>
#include <cstdint>
#include <string>
#include <vector>

namespace BWRAPI::Legacy44 {

// Builds the BWAPI callbacks of a frame from the difference to the previous
// frame (the official GameUnits order) and from the match lifecycle, plus the
// chat lines the in-game DLL took and the local player's saves.
class EventFrameAdapter {
 public:
  EventFrameAdapter() = default;
  EventFrameAdapter(const EventFrameAdapter&) = delete;
  EventFrameAdapter& operator=(const EventFrameAdapter&) = delete;

  // Events of an accepted game frame: MatchStart, PlayerLeft, unit events,
  // NukeDetect and MatchFrame. `frame` must be the frame just projected into
  // `data`.
  bool append(const FrameSample& frame, const BwrFrame& source, BWAPI::GameData& data,
              std::string& error);
  // The game left its match: MatchFrame and MatchEnd are staged on the old
  // GameData; advance_pending_lifecycle() then turns it into a MenuFrame.
  bool stage_match_end(BWAPI::GameData& data, std::string& error);
  bool advance_pending_lifecycle(BWAPI::GameData& data, bool& advanced, std::string& error);
  // GameData stays in the menu: one MenuFrame replaces any stale event list.
  bool stage_menu_frame(BWAPI::GameData& data, std::string& error);
  // An extra MatchFrame of a paused game.
  bool stage_match_frame(BWAPI::GameData& data, std::string& error);
  // A chat line for the next staged frame: the local player's typed line
  // (`sent`, SendText) or one the game recorded as received from `player`, a
  // game player number (ReceiveText, which the official server reports only
  // for another player while the client enabled user input).
  void queue_chat(bool sent, std::uint32_t player, std::string text);
  // A save the local player made, for the next staged frame (SaveGame).
  void queue_save(std::string name);
  void reset();
  bool match_active() const { return match_active_; }
  std::uint32_t match_id() const { return match_id_; }

 private:
  struct PendingEvent {
    BWAPI::EventType::Enum type = BWAPI::EventType::None;
    int v1 = 0;
    int v2 = 0;
  };
  // The per-UnitImpl flags GameUnits.cpp keeps from first access until death.
  struct TrackedUnit {
    int last_type = -1;
    int last_owner = -1;
    bool was_alive = false;
    bool was_accessible = false;
    bool was_visible = false;
    bool was_completed = false;
  };
  enum class PendingLifecycle { None, Menu };
  struct PendingChat {
    bool sent = false;
    std::uint32_t player = 0;
    std::string text;
  };

  bool add(BWAPI::GameData& data, BWAPI::EventType::Enum type, std::string& error,
           int v1 = 0, int v2 = 0);
  // Moves the queued chat lines of one kind into `events`, their text into
  // the GameData event strings.
  bool stage_chat(BWAPI::GameData& data, bool sent, std::vector<PendingEvent>& events,
                  std::string& error);
  bool stage_saves(BWAPI::GameData& data, std::vector<PendingEvent>& events, std::string& error);
  void diff_units(const FrameSample& frame, std::vector<PendingEvent>& primary,
                  std::vector<PendingEvent>& secondary);
  void diff_players(const FrameSample& frame, const BwrFrame& source, bool starting_match,
                    std::vector<PendingEvent>& events);
  void remember_frame(const FrameSample& frame, const BWAPI::GameData& data);
  // Tracking of one UnitID; nullptr while the id is unused.
  TrackedUnit* tracked(int id);

  bool match_active_ = false;
  std::uint32_t match_id_ = 0;
  PendingLifecycle pending_lifecycle_ = PendingLifecycle::None;
  bool last_terminal_winner_known_ = false;
  bool last_terminal_winner_ = false;
  // Dense by BWAPI UnitID, grown on first sight and cleared per match; a zero
  // entry means the id was never seen or its unit died.
  std::vector<TrackedUnit> units_;
  std::vector<std::uint64_t> unit_epoch_;
  std::uint64_t epoch_ = 1;
  // Frame buffers, reused so a frame costs no allocation.
  std::vector<PendingEvent> pending_, secondary_;
  std::vector<PendingChat> chat_;
  std::vector<std::string> saves_;
  std::vector<int> missing_;
  // The match's player IDs as the last appended frame listed them, so a
  // staged round maps a chat line's game player number to its BWAPI player.
  PlayerIds players_;
  // left_game of the eight playable slots in the previous frame, by slot;
  // 255 unknown.
  std::array<std::uint8_t, 8> player_left_{};
};

}  // namespace BWRAPI::Legacy44
