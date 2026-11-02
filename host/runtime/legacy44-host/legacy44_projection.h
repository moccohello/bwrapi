#pragma once

#include "legacy44_cunit_slots.h"
#include "legacy44_metrics.h"
#include "legacy44_session.h"

#include <array>
#include <cstddef>
#include <cstdint>
#include <limits>
#include <memory>
#include <string>
#include <utility>
#include <vector>

namespace BWRAPI::Legacy44 {

// Engine unit ids are (slot + 1) | uniqueness << 16.
constexpr std::size_t engine_slot(std::uint64_t id) { return (id & 0xffff) - 1; }

// BWAPI player IDs of one match, between engine slots and GameData rows.
// Server::getPlayerID numbers players in first-lookup order: onGameStart looks
// up the playable slots whose type is set and below Closed, in slot order, and
// onMatchStart then adds the neutral player. playerCount covers exactly those.
// A slot outside that set gets the next ID when it first owns a published
// unit, as the lazy lookup does, and has no PlayerData row. IDs hold for the
// whole match.
class PlayerIds {
 public:
  PlayerIds() { clear(); }
  void clear() {
    id_.fill(-1);
    slot_.fill(-1);
    next_ = 0;
    count_ = 0;
  }
  // The match-start set; `listed[slot]` marks the playable slots in it.
  void assign(const std::array<bool, kPlayablePlayerCount>& listed) {
    clear();
    for (std::size_t slot = 0; slot < kPlayablePlayerCount; ++slot)
      if (listed[slot]) add(static_cast<int>(slot));
    add(static_cast<int>(kNeutralPlayerId));
    count_ = next_;
  }
  bool known() const { return count_ != 0; }
  // playerCount: the match-start players, neutral included.
  int count() const { return count_; }
  // ID of a slot; -1 when it has none yet.
  int id(int slot) const {
    return slot >= 0 && static_cast<std::size_t>(slot) < kMaxPlayers ? id_[slot] : -1;
  }
  // ID of a match-start player, the ones with a PlayerData row; -1 otherwise.
  int listed(int slot) const {
    const int value = id(slot);
    return value < count_ ? value : -1;
  }
  // Slot of an ID; -1 when no player has it.
  int slot(int id) const {
    return id >= 0 && static_cast<std::size_t>(id) < kMaxPlayers ? slot_[id] : -1;
  }
  // ID of a unit owner, assigning the next one to a slot outside the set.
  int lookup(int slot) {
    if (!known()) return -1;
    if (id(slot) < 0) add(slot);
    return id(slot);
  }

 private:
  void add(int slot) {
    if (slot < 0 || static_cast<std::size_t>(slot) >= kMaxPlayers || id_[slot] >= 0) return;
    id_[slot] = next_;
    slot_[next_] = slot;
    ++next_;
  }
  std::array<int, kMaxPlayers> id_{};
  std::array<int, kMaxPlayers> slot_{};
  int next_ = 0;
  int count_ = 0;
};

// The unit facts the event adapter and the slot allocator need from one frame.
// Everything else is written straight from the engine tables into GameData.
struct UnitSample {
  // Row in the frame's units table.
  std::size_t row = 0;
  // BWAPI UnitID, allocated per engine unit id and never reused.
  int id = -1;
  std::uint64_t engine_id = 0;
  // Official unitArray index from the slot allocator; -1 without one.
  int unit_index = -1;
  // BWAPI replayID: (unitArray index + 1) | slot uniqueness << 11, 0 without an index.
  int replay_id = 0;
  int owner = -1;
  int type = 0;
  int order = 0;
  int x = 0;
  int y = 0;
  bool completed = false;
  // Visible to (and detected by) the local player by UnitUpdate.cpp's rule;
  // known whenever there is a local player outside a replay.
  bool visible = false;
  bool visible_known = false;
  bool detected = false;
  bool selected = false;
  // UnitImpl::canAccess this frame: the row is published with exists=true.
  bool accessible = false;
};

struct FrameSample {
  std::vector<UnitSample> units;
  std::uint32_t match_id = 0;
  std::uint32_t frame = 0;
  int local_player = -1;
  bool game_ready = false;
  bool is_replay = false;
  bool paused_known = false;
  bool paused = false;
  // First projected frame of a match.
  bool match_started = false;
  // NukeDetect positions of this frame in unit order (GameUnits.cpp
  // extractUnitData); Positions::Unknown when the target is not visible.
  std::vector<std::pair<int, int>> nuke_detects;
  // The match's player IDs as this frame left them.
  PlayerIds players;
};

// Lowercase SHA-1 hex of the map file a BWGame map path names (BWAPI
// mapHash); empty when no candidate file can be read.
std::string map_file_sha1(const std::string& path_utf8);

// UnitUpdate.cpp for a live unit that UnitImpl::canAccess rejects: the row
// Server::updateSharedMemory copies for it on the frame it evades and that
// stays until it is accessible again. `neutral` is the neutral player's ID.
void reset_inaccessible_unit(BWAPI::UnitData& unit, int neutral);

class GameDataProjection {
 public:
  GameDataProjection() = default;
  GameDataProjection(const GameDataProjection&) = delete;
  GameDataProjection& operator=(const GameDataProjection&) = delete;

  // Writes one frame of engine tables into GameData.
  bool project(const BwrFrame& frame, BWAPI::GameData& data, FrameSample& sample,
               std::string& error);
  // The game left its match (menu or between matches): the map is unpublished
  // and unit rows keep their last callback data.
  void leave_match(BWAPI::GameData& data);
  // GameImpl::getRemainingLatencyFrames/Time from the turn the game sent
  // last, its send stamped by the in-game watcher. A paused game keeps
  // sending turns, so paused rounds call it too.
  void project_latency(const BwrFrame& frame, BWAPI::GameData& data);
  void record_match_frame(
      BWAPI::GameData& data, int frame, bool force_match_reset = false,
      Legacy44FrameMetrics::Clock::time_point observed_at =
          Legacy44FrameMetrics::Clock::now()) noexcept;
  void reset();
  // Engine unit id of a BWAPI UnitID; 0 when that unit is gone or unknown.
  std::uint64_t engine_unit_id(int unit_id) const {
    return unit_id >= 0 && static_cast<std::size_t>(unit_id) < unit_engine_id_.size()
               ? unit_engine_id_[unit_id]
               : 0;
  }
  // Engine slot of a BWAPI player ID; -1 when no player has it.
  int player_slot(int id) const { return player_ids_.slot(id); }
  // The next projection clears the whole GameData (except strings) instead of
  // reusing it.
  void invalidate_game_data() { force_full_clear_ = true; }

 private:
  bool project_frame(const BwrFrame& frame, BWAPI::GameData& data, FrameSample& sample,
                     std::string& error);
  bool read_sample(const BwrFrame& frame, const std::array<bool, BWAPI::Flag::Max>& flags,
                   FrameSample& sample, std::string& error);
  void project_game(const BwrFrame& frame, const FrameSample& sample, BWAPI::GameData& data);
  void project_players(const BwrFrame& frame, const FrameSample& sample, BWAPI::GameData& data);
  bool project_units(const BwrFrame& frame, const FrameSample& sample, BWAPI::GameData& data,
                     std::string& error);
  bool project_map(const BwrFrame& frame, const FrameSample& sample, BWAPI::GameData& data,
                   std::string& error);
  bool project_static_map(const BwrFrame& frame, BWAPI::GameData& data, std::string& error);
  void project_bullets(const BwrFrame& frame, const FrameSample& sample, BWAPI::GameData& data);
  // Nuke dots and NukeDetect events; reads the isVisible grid project_map wrote.
  void project_nukes(const BwrFrame& frame, FrameSample& sample, BWAPI::GameData& data);
  void prepare_game_data(BWAPI::GameData& data, bool has_lat_com, bool has_gui,
                         int instance_id, const std::array<bool, BWAPI::Flag::Max>& flags);
  void zero_game_data(BWAPI::GameData& data, std::size_t offset, std::size_t bytes);
  void clear_static_map(BWAPI::GameData& data);
  void forget_match();
  // Grows the per-slot scratch to cover `slot`.
  void cover_slot(std::size_t slot);
  // Sprite visibility of a slot this frame; false when there is no known row.
  bool sprite_visibility(std::size_t slot, std::uint16_t& bits) const {
    if (slot >= sprite_stamp_.size() || sprite_stamp_[slot] != frame_epoch_) return false;
    bits = sprite_bits_[slot];
    return true;
  }
  // Animation and frame set of a slot's main image this frame; false when
  // there is no known row (the cells themselves may still be unknown).
  bool sprite_image(std::size_t slot, std::uint8_t& animation, std::uint16_t& frame_set) const {
    if (slot >= sprite_stamp_.size() || sprite_stamp_[slot] != frame_epoch_) return false;
    animation = sprite_animation_[slot];
    frame_set = sprite_frame_set_[slot];
    return true;
  }
  bool selection_has(std::size_t slot) const {
    return slot < selected_stamp_.size() && selected_stamp_[slot] == frame_epoch_;
  }
  bool unit_row_of(std::size_t slot, std::size_t& row) const {
    if (slot >= row_stamp_.size() || row_stamp_[slot] != frame_epoch_) return false;
    row = row_index_[slot];
    return true;
  }

  // Engine unit lives, indexed by CUnit slot. Engine unit ids are
  // (slot + 1) | uniqueness << 16, so one life owns a slot until the slot is
  // reused; 0/-1 mean no life. BWAPI UnitIDs restart every match, like the
  // official server's clearAll and UnitImpl id reset.
  std::vector<std::uint64_t> slot_life_;
  // Detours.cpp _SNetSendTurn's lastTurnFrame and lastTurnTime: the frame
  // the last sent turn carries and the QPC of its send, seen as a change of
  // the game's sent-turn count.
  bool turn_known_ = false;
  std::uint32_t turns_sent_ = 0;
  int last_turn_frame_ = 0;
  std::uint64_t last_turn_qpc_ = std::numeric_limits<std::uint64_t>::max();
  std::vector<std::int32_t> slot_unit_id_;
  // The reverse: BWAPI UnitID -> engine unit id (0 once the life ended), for
  // the commands a client issues.
  std::vector<std::uint64_t> unit_engine_id_;
  struct BulletSlot {
    int id = -1;
    int type = -1;
  };
  // Bullets of the last projected frame by engine bullet slot; a slot keeps
  // its BWAPI ID while it stays present in consecutive frames. The stamp names
  // the frame a slot was last seen in.
  std::vector<BulletSlot> bullet_slots_;
  std::vector<std::uint64_t> bullet_stamp_;
  std::uint64_t bullet_epoch_ = 1;
  // Last callback data of every UnitID published since the match began,
  // indexed by UnitID; unit_history_known_ marks the published IDs.
  std::vector<BWAPI::UnitData> unit_history_;
  std::vector<std::uint8_t> unit_history_known_;
  // UnitIDs of the units alive in the last projected frame, and whether that
  // frame still published them (exists); indexed by UnitID alongside
  // unit_history_.
  std::vector<int> active_unit_ids_;
  std::vector<std::uint8_t> unit_accessible_;
  // Per-slot scratch stamped with frame_epoch_: sprite visibility, selection
  // membership and the units-table row of this frame. frame_access_ is 0 for
  // no row, 1 for an alive row and 2 for a published (accessible) one.
  std::uint64_t frame_epoch_ = 0;
  std::vector<std::uint64_t> sprite_stamp_, selected_stamp_, row_stamp_;
  std::vector<std::uint16_t> sprite_bits_;
  std::vector<std::uint8_t> sprite_animation_;
  std::vector<std::uint16_t> sprite_frame_set_;
  // State the official UnitImpl keeps between frames, by BWAPI UnitID: the
  // previous weapon cooldowns and main-image frame set (isStartingAttack,
  // isAttackFrame) and a Ghost's painted nuke target (NukeDetect).
  struct UnitTrack {
    int last_ground_cooldown = 0;
    int last_air_cooldown = 0;
    int last_frame_set = -1;
    int nuke_x = 0;
    int nuke_y = 0;
    bool nuke_detected = false;
  };
  std::vector<UnitTrack> unit_track_;
  UnitTrack& unit_track(int id) {
    if (static_cast<std::size_t>(id) >= unit_track_.size()) unit_track_.resize(id + 1);
    return unit_track_[id];
  }
  std::vector<std::size_t> row_index_;
  std::vector<std::uint8_t> frame_access_;
  // Reused frame buffers, so a frame costs no allocation.
  std::vector<CunitSlotAllocator::Request> request_scratch_;
  std::vector<std::size_t> request_rows_;
  std::vector<CunitSlotAllocator::Slot> slot_scratch_;
  std::vector<BWAPI::unitFinder> x_search_, y_search_;
  std::vector<std::uint8_t> published_scratch_;
  // GameData reuse: the previous projection wrote reused_data_, so only the
  // per-frame members need zeroing. Unit rows and the static map members
  // persist and are rewritten by their own steps.
  const BWAPI::GameData* reused_data_ = nullptr;
  bool force_full_clear_ = false;
  // Static map members (terrain, tiles, regions, walkability) are written once
  // per match; the key is the match plus the sizes of the tables they come from.
  bool static_map_written_ = false;
  std::size_t static_region_rows_ = 0;
  std::array<std::uint64_t, 6> static_map_key_{};
  bool warned_unit_ids_ = false;
  bool warned_bullets_ = false;
  std::uint32_t match_id_ = 0;
  bool match_present_ = false;
  int next_id_ = 0;
  int next_bullet_id_ = 0;
  // UnitIDs of the match's first frame; -1 while the first frame is unknown.
  int initial_unit_count_ = -1;
  bool last_game_ready_ = false;
  // The enemy of the match start frame, kept for the whole match the way the
  // official server computes it once (GameEvents.cpp "find the opponent").
  bool enemy_known_ = false;
  int enemy_ = -1;
  // Player IDs of the match, set from the first players table of the match.
  PlayerIds player_ids_;
  // Match-start participation of the playable slots (GameEvents.cpp
  // onGameStart), taken from the engine's frame-0 column. Without frame 0
  // the slots seen owning or having lost a unit participate.
  bool participation_known_ = false;
  std::array<bool, kPlayablePlayerCount> participation_{};
  std::array<bool, kPlayablePlayerCount> owned_units_{};
  // SHA-1 hex of the match's map file, computed once per match; empty when
  // the file could not be read.
  bool map_hash_tried_ = false;
  std::string map_hash_;
  // By player ID, the upgrades (below BW's 61) one of whose user types the
  // player had a completed unit of in the last projected frame's counts.
  // PlayerImpl::updateData shows an enemy's level of those upgrades only, and
  // runs before the frame's unit counts are taken.
  static constexpr std::size_t kUpgradeTypeCount = 61;
  std::array<std::array<bool, kUpgradeTypeCount>, kMaxPlayers> upgrade_users_{};
  // CUnit records the engine's pool reported for this match; 0 until known.
  std::size_t pool_slots_ = 0;
  Legacy44FrameMetrics frame_metrics_;
  std::unique_ptr<CunitSlotAllocator> unit_slots_ = std::make_unique<CunitSlotAllocator>();
  std::uint64_t unit_slots_generation_ = 0;
};

}  // namespace BWRAPI::Legacy44
