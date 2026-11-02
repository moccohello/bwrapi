#include "legacy44_projection.h"
#include "legacy44_bwapi_rules.h"

#include <BWAPI/Flag.h>
#include <BWAPI/Latency.h>
#include <BWAPI/Order.h>
#include <BWAPI/UnitType.h>
#include <BWAPI/Position.h>

#include <bcrypt.h>
#include <shlobj.h>

#include <algorithm>
#include <cstring>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <iterator>
#include <limits>
#include <vector>

#pragma comment(lib, "bcrypt")
#pragma comment(lib, "shell32")
#pragma comment(lib, "ole32")

namespace BWRAPI::Legacy44 {

namespace {

// Places a map path from the game's BWGame map name field can name a file:
// the path as given (absolute, or relative to the game's working
// directory), then relative to the game executable's folder, the install
// root above it (SC:R keeps the executable in an architecture folder), and
// the user's StarCraft documents folder, where SC:R keeps its Maps folder.
std::vector<std::filesystem::path> map_file_candidates(const std::filesystem::path& path) {
  std::vector<std::filesystem::path> candidates{path};
  if (!path.is_relative()) return candidates;
  wchar_t module[MAX_PATH];
  const DWORD length = GetModuleFileNameW(nullptr, module, MAX_PATH);
  if (length != 0 && length < MAX_PATH) {
    const std::filesystem::path folder = std::filesystem::path(module).parent_path();
    candidates.push_back(folder / path);
    candidates.push_back(folder.parent_path() / path);
  }
  PWSTR documents = nullptr;
  if (SUCCEEDED(SHGetKnownFolderPath(FOLDERID_Documents, 0, nullptr, &documents)))
    candidates.push_back(std::filesystem::path(documents) / L"StarCraft" / path);
  CoTaskMemFree(documents);
  return candidates;
}

}  // namespace

// Map::calculateMapHash: the lowercase SHA-1 hex of the map file's bytes.
std::string map_file_sha1(const std::string& path_utf8) {
  const int wide_length = MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, path_utf8.data(),
                                              static_cast<int>(path_utf8.size()), nullptr, 0);
  if (wide_length <= 0) return {};
  std::wstring wide(static_cast<std::size_t>(wide_length), L'\0');
  MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, path_utf8.data(),
                      static_cast<int>(path_utf8.size()), wide.data(), wide_length);
  std::vector<char> bytes;
  bool read = false;
  for (const auto& candidate : map_file_candidates(std::filesystem::path(wide))) {
    std::ifstream file(candidate, std::ios::binary);
    if (!file) continue;
    bytes.assign(std::istreambuf_iterator<char>(file), std::istreambuf_iterator<char>());
    read = !file.bad();
    break;
  }
  if (!read) return {};
  unsigned char digest[20] = {};
  BCRYPT_ALG_HANDLE algorithm = nullptr;
  BCRYPT_HASH_HANDLE hash = nullptr;
  bool hashed = false;
  if (BCRYPT_SUCCESS(BCryptOpenAlgorithmProvider(&algorithm, BCRYPT_SHA1_ALGORITHM, nullptr, 0))) {
    if (BCRYPT_SUCCESS(BCryptCreateHash(algorithm, &hash, nullptr, 0, nullptr, 0, 0))) {
      hashed = BCRYPT_SUCCESS(BCryptHashData(hash, reinterpret_cast<PUCHAR>(bytes.data()),
                                             static_cast<ULONG>(bytes.size()), 0)) &&
               BCRYPT_SUCCESS(BCryptFinishHash(hash, digest, sizeof(digest), 0));
      BCryptDestroyHash(hash);
    }
    BCryptCloseAlgorithmProvider(algorithm, 0);
  }
  if (!hashed) return {};
  static constexpr char kHex[] = "0123456789abcdef";
  std::string hex;
  hex.reserve(sizeof(digest) * 2);
  for (const unsigned char byte : digest) {
    hex.push_back(kHex[byte >> 4]);
    hex.push_back(kHex[byte & 0xf]);
  }
  return hex;
}

namespace {

// BWAPI Position.h defines TilePositions::None and ::Unknown as these exact
// build-tile coordinates.  GameData has no nullable PlayerData start field.
constexpr int kTilePositionSentinelX = 32000 / 32;
constexpr int kTilePositionUnknownY = 32064 / 32;

// Milliseconds the QPC counter moved past `stamp`; the in-game watcher
// stamps turn sends with the same counter.
std::int64_t ms_since_qpc(std::uint64_t stamp) {
  LARGE_INTEGER now, frequency;
  QueryPerformanceCounter(&now);
  QueryPerformanceFrequency(&frequency);
  return (now.QuadPart - static_cast<std::int64_t>(stamp)) * 1000 / frequency.QuadPart;
}

// BW::StatusFlags and BW::MovementFlags bits UnitUpdate.cpp tests.
constexpr std::uint32_t kStatusBurrowed = 0x10;
constexpr std::uint32_t kStatusRequiresDetection = 0x100;

struct SelfView {
  bool visible = false;
  bool detected = false;
};

// UnitUpdate.cpp isVisible[self] and isDetected for a unit the BWAPI player
// does not own, or one without a sprite. No sprite (sprite bits unknown) is
// not visible. A Scanner Sweep is visible when a map tile within 64 pixels is;
// a unit that requires detection hides from a player that does not detect it
// while it stands still, holds no Move order, has both cooldowns at 0 and is
// burrowed.
SelfView self_view(const BwrFrame& frame, std::size_t row, bool sprite_known, std::uint16_t bits,
                   int self) {
  const BwrUnits& t = *frame.units;
  SelfView view;
  if (!sprite_known) return view;
  if (t.unit_type[row] == BWAPI::UnitTypes::Enum::Spell_Scanner_Sweep) {
    const BwrVision* vision = frame.vision;
    const int width = static_cast<int>(frame.map_width);
    const int height = static_cast<int>(frame.map_height);
    const int x = t.x[row];
    const int y = t.y[row];
    // Tiles outside the map are not read: the official read there is past
    // the tile array.
    if (vision != nullptr && vision->rows == static_cast<std::size_t>(width) * height) {
      for (int tile_x = (x - 64) / 32; tile_x <= (x + 64) / 32 && !view.visible; ++tile_x)
        for (int tile_y = (y - 64) / 32; tile_y <= (y + 64) / 32; ++tile_y)
          if (tile_x >= 0 && tile_y >= 0 && tile_x < width && tile_y < height &&
              ((vision->visible[static_cast<std::size_t>(tile_y) * width + tile_x] >> self) & 1) != 0) {
            view.visible = true;
            break;
          }
    }
    view.detected = true;
    return view;
  }
  const std::uint32_t status = t.status_flags[row];
  // The raw detection mask; -1 (every bit) is the official "detected by all".
  const bool detected_bit = ((t.detected_by[row] >> self) & 1) != 0;
  view.visible = ((bits >> self) & 1) != 0;
  if ((status & kStatusRequiresDetection) != 0)
    view.visible = view.visible &&
                   (detected_bit || t.moving[row] == 1 || t.order[row] == BWAPI::Orders::Enum::Move ||
                    t.ground_cooldown[row] > 0 || t.air_cooldown[row] > 0 ||
                    (status & kStatusBurrowed) == 0);
  view.detected = view.visible && ((status & kStatusRequiresDetection) == 0 || detected_bit);
  return view;
}

// UnitUpdate.cpp _getType: every mineral field variant is
// Resource_Mineral_Field, and a building under a Zerg morph order is the type
// its build queue names (what it morphs into) once that is not None.
int public_unit_type(int type, int order, std::uint16_t build_type) {
  if (BWAPI::UnitType(type).isMineralField()) return BWAPI::UnitTypes::Enum::Resource_Mineral_Field;
  if (is_bwapi_building_type(type) && is_morphing_order(order) && known(build_type) &&
      build_type < BWAPI::UnitTypes::Enum::None)
    return build_type;
  return type;
}

// UnitImpl::canAccess: replay and CompleteMapInformation publish everything
// (a replay turns every flag on upstream), otherwise a unit is published
// while the BWAPI player sees it. Without a local player there is no
// perspective to filter by.
bool unit_is_accessible(const UnitSample& unit, int local_player, bool replay,
                        bool complete_map, bool match_started) {
  if (replay || complete_map || local_player < 0 || !unit.visible_known || unit.visible)
    return true;
  // Neutral units are accessible on the match's first frame, as in
  // canAccess's frame-0 rule.
  return match_started && unit.owner == static_cast<int>(kNeutralPlayerId);
}

}  // namespace

void GameDataProjection::reset() {
  forget_match();
  reused_data_ = nullptr;
  match_present_ = false;
  match_id_ = 0;
  last_game_ready_ = false;
  frame_metrics_.reset();
}

void GameDataProjection::forget_match() {
  turn_known_ = false;
  last_turn_qpc_ = std::numeric_limits<std::uint64_t>::max();
  unit_slots_->report(unit_slots_generation_);
  unit_slots_->reset(static_cast<std::uint32_t>(pool_slots_));
  // The official server clears its unit vector and resets every UnitImpl id on
  // a new match, so BWAPI UnitIDs start from 0 again.
  slot_life_.clear();
  slot_unit_id_.clear();
  unit_engine_id_.clear();
  bullet_slots_.clear();
  bullet_stamp_.clear();
  bullet_epoch_ = 1;
  unit_history_.clear();
  unit_history_known_.clear();
  active_unit_ids_.clear();
  unit_accessible_.clear();
  next_id_ = 0;
  next_bullet_id_ = 0;
  initial_unit_count_ = -1;
  enemy_known_ = false;
  enemy_ = -1;
  player_ids_.clear();
  participation_known_ = false;
  participation_.fill(false);
  owned_units_.fill(false);
  for (auto& users : upgrade_users_) users.fill(false);
  map_hash_tried_ = false;
  map_hash_.clear();
  unit_track_.clear();
  frame_epoch_ = 0;
  sprite_stamp_.clear();
  sprite_bits_.clear();
  sprite_animation_.clear();
  sprite_frame_set_.clear();
  selected_stamp_.clear();
  row_stamp_.clear();
  row_index_.clear();
  frame_access_.clear();
}

void GameDataProjection::cover_slot(std::size_t slot) {
  if (slot < sprite_stamp_.size()) return;
  sprite_stamp_.resize(slot + 1, 0);
  sprite_bits_.resize(slot + 1, 0);
  sprite_animation_.resize(slot + 1, 0);
  sprite_frame_set_.resize(slot + 1, 0);
  selected_stamp_.resize(slot + 1, 0);
  row_stamp_.resize(slot + 1, 0);
  row_index_.resize(slot + 1, 0);
  frame_access_.resize(slot + 1, 0);
  slot_life_.resize(slot + 1, 0);
  slot_unit_id_.resize(slot + 1, -1);
}

void GameDataProjection::project_latency(const BwrFrame& frame, BWAPI::GameData& data) {
  const BwrGame* game = frame.game;
  if (game == nullptr || game->rows == 0 || !known(game->turns_sent[0])) return;
  const std::uint32_t turns = game->turns_sent[0];
  if (!turn_known_ || turns != turns_sent_) {
    // BWAPI counts a frame as done when its update ends, before the game
    // sends the turn and steps: a turn sent before this frame carries this
    // frame's count, as does one a paused game sends on the frame it stands on.
    last_turn_frame_ = data.frameCount;
    turns_sent_ = turns;
    turn_known_ = true;
  }
  // A known stamp always names the frame's own turn count; an unknown one
  // (the watcher has not caught up) keeps the last stamp for that count.
  if (known(game->last_turn_qpc[0])) last_turn_qpc_ = game->last_turn_qpc[0];
  // GameImpl: latency frames less the frames since the last turn, latency
  // time less the milliseconds since its send.
  data.remainingLatencyFrames = data.latencyFrames - (data.frameCount - last_turn_frame_);
  if (known(last_turn_qpc_)) {
    data.remainingLatencyTime = data.latencyTime - static_cast<int>(ms_since_qpc(last_turn_qpc_));
  }
}

void GameDataProjection::record_match_frame(
    BWAPI::GameData& data, int frame, bool force_match_reset,
    Legacy44FrameMetrics::Clock::time_point observed_at) noexcept {
  frame_metrics_.on_frame(frame, observed_at, force_match_reset);
  data.fps = frame_metrics_.fps();
  data.averageFPS = frame_metrics_.average_fps();
}

void GameDataProjection::zero_game_data(BWAPI::GameData& data, std::size_t offset,
                                        std::size_t bytes) {
  if (bytes == 0) return;
  std::memset(reinterpret_cast<unsigned char*>(&data) + offset, 0, bytes);
}

// Zeroes what this frame rewrites. The client string table is never cleared
// (only stringCount resets). A GameData that this projection wrote last time
// keeps its unit rows and static map members, which their own steps rewrite,
// and clears only the used prefixes of the event and client command tables.
void GameDataProjection::prepare_game_data(BWAPI::GameData& data, bool has_lat_com,
                                           bool has_gui, int instance_id,
                                           const std::array<bool, BWAPI::Flag::Max>& flags) {
  using Game = BWAPI::GameData;
  const bool full = reused_data_ != &data || force_full_clear_;
  const auto zero_between = [&](std::size_t begin, std::size_t end) {
    zero_game_data(data, begin, end - begin);
  };
  if (full) {
    constexpr std::size_t strings_end = offsetof(Game, strings) + sizeof(Game::strings);
    zero_between(0, offsetof(Game, strings));
    zero_between(strings_end, sizeof(Game));
    // Units no longer released keep their last callback data.
    for (std::size_t id = 0; id < unit_history_.size(); ++id)
      if (unit_history_known_[id]) data.units[id] = unit_history_[id];
    static_map_written_ = false;
    static_region_rows_ = 0;
  } else {
    const auto used = [](int count, int capacity) {
      return static_cast<std::size_t>(std::clamp(count, 0, capacity));
    };
    const auto events = used(data.eventCount, Game::MAX_EVENTS);
    const auto event_strings = used(data.eventStringCount, Game::MAX_EVENT_STRINGS);
    const auto shapes = used(data.shapeCount, Game::MAX_SHAPES);
    const auto commands = used(data.commandCount, Game::MAX_COMMANDS);
    const auto unit_commands = used(data.unitCommandCount, Game::MAX_UNIT_COMMANDS);
    zero_between(0, offsetof(Game, units));
    zero_between(offsetof(Game, unitArray), offsetof(Game, getGroundHeight));
    zero_between(offsetof(Game, isVisible), offsetof(Game, mapTileRegionId));
    zero_between(offsetof(Game, regionCount), offsetof(Game, regions));
    zero_between(offsetof(Game, startLocationCount), offsetof(Game, events));
    zero_game_data(data, offsetof(Game, events), events * sizeof(Game::events[0]));
    zero_between(offsetof(Game, eventStringCount), offsetof(Game, eventStrings));
    zero_game_data(data, offsetof(Game, eventStrings), event_strings * sizeof(Game::eventStrings[0]));
    zero_between(offsetof(Game, stringCount), offsetof(Game, strings));
    zero_between(offsetof(Game, shapeCount), offsetof(Game, shapes));
    zero_game_data(data, offsetof(Game, shapes), shapes * sizeof(Game::shapes[0]));
    zero_between(offsetof(Game, commandCount), offsetof(Game, commands));
    zero_game_data(data, offsetof(Game, commands), commands * sizeof(Game::commands[0]));
    zero_between(offsetof(Game, unitCommandCount), offsetof(Game, unitCommands));
    zero_game_data(data, offsetof(Game, unitCommands), unit_commands * sizeof(Game::unitCommands[0]));
    zero_between(offsetof(Game, unitSearchSize), sizeof(Game));
  }
  reused_data_ = &data;
  force_full_clear_ = false;
  data.client_version = static_cast<int>(kClientVersion);
  data.revision = kBwapiRevision;
  data.isDebug = false;
  data.instanceID = instance_id;
  data.hasLatCom = has_lat_com;
  data.hasGUI = has_gui;
  std::copy(flags.begin(), flags.end(), std::begin(data.flags));
  std::fill(std::begin(data.unitArray), std::end(data.unitArray), -1);
  std::fill(std::begin(data.selectedUnits), std::end(data.selectedUnits), -1);
  std::fill(std::begin(data.xUnitSearch), std::end(data.xUnitSearch),
            BWAPI::unitFinder{-1, -1});
  std::fill(std::begin(data.yUnitSearch), std::end(data.yUnitSearch),
            BWAPI::unitFinder{-1, -1});
  for (auto& player : data.players) {
    player.startLocationX = kTilePositionSentinelX;
    player.startLocationY = kTilePositionUnknownY;
  }
}

void GameDataProjection::leave_match(BWAPI::GameData& data) {
  const bool has_lat_com = data.hasLatCom;
  const bool has_gui = data.hasGUI;
  const int instance_id = data.instanceID;
  std::array<bool, BWAPI::Flag::Max> flags{};
  std::copy(std::begin(data.flags), std::end(data.flags), flags.begin());
  for (const int id : active_unit_ids_)
    if (static_cast<std::size_t>(id) < unit_history_.size()) unit_history_[id] = data.units[id];
  prepare_game_data(data, has_lat_com, has_gui, instance_id, flags);
  last_game_ready_ = false;
  data.isInGame = false;
  data.neutral = -1;
  data.self = -1;
  data.enemy = -1;
  // Unit rows keep their last callback data; the map is unpublished.
  clear_static_map(data);
  frame_metrics_.leave_match();
  data.fps = frame_metrics_.fps();
  data.averageFPS = frame_metrics_.average_fps();
}

bool GameDataProjection::project(const BwrFrame& frame, BWAPI::GameData& data,
                                 FrameSample& sample, std::string& error) {
  if (project_frame(frame, data, sample, error)) return true;
  // A partial frame may leave rows that the reuse bookkeeping does not know.
  force_full_clear_ = true;
  return false;
}

// Units of the frame as the event adapter and the slot allocator see them.
// Turret sub-units never become public BWAPI units.
bool GameDataProjection::read_sample(const BwrFrame& frame,
                                     const std::array<bool, BWAPI::Flag::Max>& flags,
                                     FrameSample& sample, std::string& error) {
  // The units buffer keeps its capacity across frames.
  sample.units.clear();
  sample.nuke_detects.clear();
  sample.game_ready = true;
  sample.match_id = frame.match_id;
  sample.frame = frame.game_frame;
  sample.local_player = frame.local_player < kPlayablePlayerCount ? frame.local_player : -1;
  sample.is_replay = (frame.flags & BWR_FRAME_REPLAY) != 0;
  if (frame.game != nullptr && frame.game->rows > 0 && known(frame.game->paused[0])) {
    sample.paused_known = true;
    sample.paused = frame.game->paused[0] == 1;
  }

  // The pool size is a game fact; it holds for the whole match and sizes the
  // per-slot state a new match resets.
  if ((sample.match_started || pool_slots_ == 0) && frame.game != nullptr &&
      frame.game->rows > 0 && known(frame.game->unit_slots[0]) &&
      frame.game->unit_slots[0] > 0)
    pool_slots_ = frame.game->unit_slots[0];
  if (sample.match_started) {
    forget_match();
    unit_slots_generation_ = frame.match_id;
    // The previous match's unit rows go with a whole GameData clear.
    force_full_clear_ = true;
  }

  const BwrUnits* units = frame.units;
  if (units == nullptr) return true;

  // Per-slot frame stamps: no per-frame hashing or clearing.
  ++frame_epoch_;
  if (frame.render != nullptr)
    for (std::size_t row = 0; row < frame.render->rows; ++row) {
      const std::uint64_t id = frame.render->unit[row];
      if (id == 0) continue;
      const auto slot = engine_slot(id);
      cover_slot(slot);
      sprite_stamp_[slot] = frame_epoch_;
      sprite_bits_[slot] = frame.render->visible_to[row];
      sprite_animation_[slot] = frame.render->animation[row];
      sprite_frame_set_[slot] = frame.render->frame_set[row];
    }
  if (frame.selection != nullptr)
    for (std::size_t row = 0; row < frame.selection->rows; ++row) {
      const std::uint64_t id = frame.selection->unit[row];
      if (id == 0) continue;
      const auto slot = engine_slot(id);
      cover_slot(slot);
      selected_stamp_[slot] = frame_epoch_;
    }

  const bool complete_map = flags[BWAPI::Flag::CompleteMapInformation];
  sample.units.reserve(units->rows);
  for (std::size_t row = 0; row < units->rows; ++row) {
    const int type = units->unit_type[row];
    const int owner = units->owner[row];
    if (type >= BWAPI::UnitTypes::Enum::MAX || owner >= static_cast<int>(kMaxPlayers)) continue;
    const std::uint64_t engine_id = units->id[row];
    if (engine_id == 0) continue;
    const auto slot = engine_slot(engine_id);
    // Every table row is addressable by slot, turret sub-units included; the
    // damage-dealer cooldowns read the turret's row.
    cover_slot(slot);
    row_stamp_[slot] = frame_epoch_;
    row_index_[slot] = row;
    if (is_bwapi_subunit_type(type)) continue;
    if (pool_slots_ != 0 && slot >= pool_slots_) continue;
    // One BWAPI UnitID per engine unit life; the ids restart every match.
    if (slot_life_[slot] != engine_id) {
      if (next_id_ >= static_cast<int>(kMaxUnits)) {
        // UnitIDs index the client's fixed 10000 UnitImpl objects and are
        // never reused; a life without one stays unpublished.
        if (!warned_unit_ids_) {
          std::cerr << "[projection-capacity] all " << kMaxUnits
                    << " BWAPI UnitIDs are used; new units are not published\n";
          warned_unit_ids_ = true;
        }
        continue;
      }
      if (slot_life_[slot] != 0) unit_engine_id_[slot_unit_id_[slot]] = 0;
      slot_life_[slot] = engine_id;
      slot_unit_id_[slot] = next_id_;
      if (static_cast<std::size_t>(next_id_) >= unit_engine_id_.size())
        unit_engine_id_.resize(static_cast<std::size_t>(next_id_) + 1, 0);
      unit_engine_id_[next_id_] = engine_id;
      ++next_id_;
    }
    UnitSample unit;
    unit.row = row;
    unit.id = slot_unit_id_[slot];
    unit.engine_id = engine_id;
    unit.owner = owner;
    unit.order = units->order[row] < BWAPI::Orders::Enum::MAX ? units->order[row]
                                                              : BWAPI::Orders::Enum::Unknown;
    unit.type = public_unit_type(type, unit.order, units->build_type[row]);
    unit.x = units->x[row];
    unit.y = units->y[row];
    unit.completed = units->completed[row] == 1 && !is_morphing_order(unit.order);
    unit.selected = selection_has(slot);
    std::uint16_t bits = 0;
    const bool sprite_known = sprite_visibility(slot, bits) && known(bits);
    if (sample.is_replay) {
      if (sprite_known) {
        unit.visible_known = true;
        unit.visible = bits != 0;
      }
    } else if (sample.local_player >= 0) {
      unit.visible_known = true;
      // A unit with a sprite is always visible and detected to its owner
      // (UnitUpdate.cpp); without one it is hidden even from its owner. A
      // frame without the render table says nothing about sprites.
      const bool has_sprite = sprite_known || frame.render == nullptr;
      const SelfView view = owner == sample.local_player && has_sprite
                                ? SelfView{true, true}
                                : self_view(frame, row, sprite_known, bits, sample.local_player);
      unit.visible = view.visible;
      unit.detected = view.detected;
    }
    unit.accessible = unit_is_accessible(unit, sample.local_player, sample.is_replay,
                                         complete_map, sample.match_started);
    frame_access_[slot] = static_cast<std::uint8_t>(unit.accessible ? 2 : 1);
    sample.units.push_back(unit);
  }
  // Server::onMatchStart: the first update's unit vector covers every live
  // unit, published or not, as the client's getInitialUnits block.
  if (sample.match_started) initial_unit_count_ = next_id_;
  // A unit missing from the table is gone: its CUnit record is no longer live.
  for (const int id : active_unit_ids_) {
    const std::uint64_t engine_id =
        id >= 0 && static_cast<std::size_t>(id) < unit_engine_id_.size()
            ? unit_engine_id_[static_cast<std::size_t>(id)]
            : 0;
    if (engine_id == 0) continue;  // already ended by a slot reuse
    const auto slot = engine_slot(engine_id);
    if (slot < slot_life_.size() && slot_life_[slot] == engine_id &&
        row_stamp_[slot] == frame_epoch_)
      continue;
    if (slot < slot_life_.size() && slot_life_[slot] == engine_id) {
      slot_life_[slot] = 0;
      slot_unit_id_[slot] = -1;
    }
    unit_engine_id_[static_cast<std::size_t>(id)] = 0;
  }
  active_unit_ids_.clear();
  active_unit_ids_.reserve(sample.units.size());
  for (const auto& unit : sample.units) active_unit_ids_.push_back(unit.id);

  // Every live record asks the slot allocator for a unitArray slot; beyond
  // 1700 rows the eviction order leaves units without an index.
  request_scratch_.clear();
  request_rows_.clear();
  for (std::size_t index = 0; index < sample.units.size(); ++index) {
    const auto& unit = sample.units[index];
    const auto ordinal = engine_slot(unit.engine_id);
    if (pool_slots_ != 0 && ordinal >= pool_slots_) continue;
    request_scratch_.push_back({static_cast<std::uint32_t>(ordinal),
                                ClassifyCunitSlotTier(unit.owner, unit.type, sample.local_player,
                                                      unit.visible_known && unit.visible),
                                unit.engine_id});
    request_rows_.push_back(index);
  }
  if (!unit_slots_->assign_public(request_scratch_, slot_scratch_, error)) return false;
  for (std::size_t i = 0; i < request_rows_.size(); ++i) {
    auto& unit = sample.units[request_rows_[i]];
    unit.unit_index = slot_scratch_[i].slot;
    // BWAPI UnitTarget: (unitArray index + 1) | uniqueness << 11.
    unit.replay_id = slot_scratch_[i].slot >= 0
        ? static_cast<int>((static_cast<std::uint32_t>(slot_scratch_[i].slot) + 1u) |
                           (slot_scratch_[i].uniqueness << 11))
        : 0;
  }
  return true;
}

void GameDataProjection::project_game(const BwrFrame& frame, const FrameSample& sample,
                                      BWAPI::GameData& data) {
  const auto cell = [&](auto& destination, auto value) {
    destination = static_cast<std::decay_t<decltype(destination)>>(value);
  };
  data.isInGame = true;
  if (known(frame.game_frame)) {
    data.frameCount = frame.game_frame > static_cast<std::uint32_t>(std::numeric_limits<int>::max())
                          ? std::numeric_limits<int>::max()
                          : static_cast<int>(frame.game_frame);
  }
  data.isReplay = sample.is_replay;

  const BwrGame* game = frame.game;
  const bool has_game = game != nullptr && game->rows > 0;
  if (has_game) {
    if (known(game->game_type[0])) {
      data.gameType = game->game_type[0];
    }
    const auto multiplayer = game->multiplayer[0];
    if (multiplayer <= 1) {
      data.isMultiplayer = multiplayer == 1;
    }
    const auto battle_net = game->battle_net[0];
    if (battle_net <= 1) {
      data.isBattleNet = battle_net == 1;
    }
    // GameImpl::getLatency: one category in single player; a network match
    // takes the game's latency setting, an unknown one as the lowest.
    if (multiplayer == 0) {
      data.latency = BWAPI::Latency::SinglePlayer;
    } else if (multiplayer == 1 && battle_net <= 1) {
      using L = BWAPI::Latency::Enum;
      const L lan[3] = {L::LanLow, L::LanMedium, L::LanHigh};
      const L battlenet[3] = {L::BattlenetLow, L::BattlenetMedium, L::BattlenetHigh};
      const auto setting = game->latency[0] <= 2 ? game->latency[0] : 0;
      data.latency = (battle_net == 1 ? battlenet : lan)[setting];
    }
    if (sample.paused_known) {
      data.isPaused = sample.paused;
    }
    // GameImpl::getLatencyFrames is the turn delay (latency plus call delay)
    // plus the frame the turn is sent on: a command issued now takes effect
    // that many frames later (two in single player).
    const auto turn_delay = game->latency_frames[0];
    if (known(turn_delay) && turn_delay < static_cast<std::uint32_t>(std::numeric_limits<int>::max())) {
      const std::uint32_t latency_frames = turn_delay + 1;
      data.latencyFrames = static_cast<int>(latency_frames);
      if (known(game->frame_ms[0])) {
        data.latencyTime = static_cast<std::int32_t>(latency_frames * game->frame_ms[0]);
      }
    }
    project_latency(frame, data);
    if (known(game->elapsed_seconds[0])) {
      data.elapsedTime = game->elapsed_seconds[0];
    }
    if (known(game->random_seed[0])) {
      data.randomSeed = game->random_seed[0];
    }
    if (known(game->replay_frames[0])) {
      // The official API returns the header u32 as a signed int without
      // checking isReplay.
      data.replayFrameCount = static_cast<std::int32_t>(game->replay_frames[0]);
    }
    if (known(game->countdown_seconds[0])) {
      data.countdownTimer = static_cast<int>(
          std::min<std::uint32_t>(game->countdown_seconds[0], std::numeric_limits<int>::max()));
    }
    // Empty text is unknown; a non-empty one is copied with its terminator.
    const auto text = [&](auto& destination, const BwrText256& source) {
      const std::size_t length = strnlen(source.text, sizeof(source.text));
      if (length == 0) return;
      const std::size_t count = std::min(length, sizeof(destination) - 1);
      std::memcpy(destination, source.text, count);
      destination[count] = '\0';
    };
    text(data.mapFileName, game->map_file[0]);
    text(data.mapPathName, game->map_path[0]);
    text(data.mapName, game->map_title[0]);
    // Map::calculateMapHash runs once per match; a file that cannot be read
    // leaves the hash empty for the match.
    const std::size_t path_length = strnlen(game->map_path[0].text, sizeof(game->map_path[0].text));
    if (!map_hash_tried_ && path_length != 0) {
      map_hash_tried_ = true;
      map_hash_ = map_file_sha1(std::string(game->map_path[0].text, path_length));
    }
  }
  if (!map_hash_.empty()) {
    const std::size_t count = std::min(map_hash_.size(), sizeof(data.mapHash) - 1);
    std::memcpy(data.mapHash, map_hash_.data(), count);
    data.mapHash[count] = '\0';
  }

  if (frame.viewport != nullptr && frame.viewport->rows > 0 &&
      known(frame.viewport->x[0]) && known(frame.viewport->y[0])) {
    cell(data.screenX, frame.viewport->x[0]);
    cell(data.screenY, frame.viewport->y[0]);
  }
  // GameImpl::getMousePosition: the cursor only with user input enabled,
  // Positions::Unknown otherwise.
  const bool mouse_known = frame.viewport != nullptr && frame.viewport->rows > 0 &&
                           known(frame.viewport->mouse_x[0]) && known(frame.viewport->mouse_y[0]);
  if (mouse_known && data.flags[BWAPI::Flag::UserInput]) {
    data.mouseX = frame.viewport->mouse_x[0];
    data.mouseY = frame.viewport->mouse_y[0];
  } else {
    data.mouseX = BWAPI::Positions::Unknown.x;
    data.mouseY = BWAPI::Positions::Unknown.y;
  }

  // Start locations of the map, in slot order.
  if (frame.start_locations != nullptr) {
    const std::size_t count = std::min(frame.start_locations->rows, kMaxStartLocations);
    data.startLocationCount = static_cast<int>(count);
    for (std::size_t index = 0; index < count; ++index) {
      cell(data.startLocations[index].x, frame.start_locations->x[index]);
      cell(data.startLocations[index].y, frame.start_locations->y[index]);
    }
  }

  // The Client's getForces skips ID 0 (the neutral force); IDs 1..4 are the
  // native teams, not the official Server's pointer-order IDs.
  if (frame.forces != nullptr && frame.forces->rows > 0) {
    data.forceCount = 5;
    data.forces[0].name[0] = 0;
    for (std::size_t row = 0; row < frame.forces->rows; ++row) {
      const std::uint8_t id = frame.forces->id[row];
      if (id < 1 || id > 4) continue;
      auto& name = data.forces[id].name;
      const std::size_t length = strnlen(frame.forces->name[row].text, 31);
      std::memcpy(name, frame.forces->name[row].text, length);
      name[length] = '\0';
    }
  }

  // Selected units in selection order, at most twelve; only published units
  // are selectable (updateUnits drops the rest of the selection). The slot
  // life check drops entries whose row is gone this frame.
  if (frame.selection != nullptr && frame.units != nullptr) {
    int count = 0;
    for (std::size_t row = 0; row < frame.selection->rows && count < 12; ++row) {
      const std::uint64_t id = frame.selection->unit[row];
      if (id == 0) continue;
      const auto slot = engine_slot(id);
      if (slot >= frame_access_.size() || frame_access_[slot] != 2) continue;
      if (slot >= slot_life_.size() || slot_life_[slot] != id) continue;
      data.selectedUnits[count++] = slot_unit_id_[slot];
    }
    data.selectedUnitCount = count;
  }
}

bool GameDataProjection::project_frame(const BwrFrame& frame, BWAPI::GameData& data,
                                       FrameSample& sample, std::string& error) {
  const auto frame_observed_at = Legacy44FrameMetrics::Clock::now();
  const bool has_lat_com = data.hasLatCom;
  const bool has_gui = data.hasGUI;
  const int instance_id = data.instanceID;
  std::array<bool, BWAPI::Flag::Max> flags{};
  std::copy(std::begin(data.flags), std::end(data.flags), flags.begin());
  sample.match_started = !match_present_ || frame.match_id != match_id_ || !last_game_ready_;
  // GameImpl::initializeData clears every flag on a new match, while
  // GameEvents enables all of them for replay playback.
  if (sample.match_started) flags.fill(sample.is_replay);
  // The Evade/Destroy data settled in the last callback carries into the next
  // projection: a released unit's row stays in GameData, so only the previous
  // frame's units need their history refreshed.
  for (const int id : active_unit_ids_)
    if (static_cast<std::size_t>(id) < unit_history_.size()) unit_history_[id] = data.units[id];
  if (!read_sample(frame, flags, sample, error)) return false;
  prepare_game_data(data, has_lat_com, has_gui, instance_id, flags);

  match_present_ = true;
  match_id_ = frame.match_id;
  last_game_ready_ = true;
  // Server::onMatchStart: the unit vector of the first update, filled with
  // every live unit whether or not the client may see it.
  data.initialUnitCount = initial_unit_count_ >= 0 ? initial_unit_count_ : 0;
  data.self = -1;
  data.enemy = -1;
  data.neutral = -1;

  project_game(frame, sample, data);
  project_players(frame, sample, data);
  if (!project_units(frame, sample, data, error)) return false;
  if (!project_map(frame, sample, data, error)) return false;
  project_bullets(frame, sample, data);
  project_nukes(frame, sample, data);
  sample.players = player_ids_;

  // Unit rows keep their last callback data: a unit that left this frame has
  // exists=false and no known cells any more.
  if (unit_history_.size() < static_cast<std::size_t>(next_id_)) {
    unit_history_.resize(next_id_);
    unit_history_known_.resize(next_id_, 0);
    unit_accessible_.resize(next_id_, 0);
  }
  published_scratch_.assign(next_id_, 0);
  for (const auto& source : sample.units) {
    published_scratch_[source.id] = source.accessible ? 2 : 1;
    if (source.accessible && !unit_history_known_[source.id]) {
      unit_history_known_[source.id] = 1;
      unit_history_[source.id] = data.units[source.id];
    }
  }
  for (std::size_t id = 0; id < published_scratch_.size(); ++id) {
    const bool was_accessible = unit_accessible_[id] != 0;
    if (published_scratch_[id] == 2) {
      unit_accessible_[id] = 1;
      continue;
    }
    if (!was_accessible) continue;
    // GameImpl::update: a unit that evades while alive already holds the
    // inaccessible defaults when Server::updateSharedMemory copies the
    // evaded units. A destroyed one is copied with its last frame's data and
    // only exists (computeUnitExistence) and selection (updateUnits) ended,
    // so onUnitDestroy still sees its type and owner; UnitImpl::die resets
    // only the server's own copy. Neither row is copied again.
    if (published_scratch_[id] == 1) {
      reset_inaccessible_unit(data.units[id], player_ids_.id(static_cast<int>(kNeutralPlayerId)));
    } else {
      data.units[id].exists = false;
      data.units[id].isSelected = false;
    }
    unit_history_[id] = data.units[id];
    unit_accessible_[id] = 0;
  }

  record_match_frame(data, data.frameCount,
                     sample.match_started, frame_observed_at);
  return true;
}

// GameUnits.cpp extractUnitData: a Ghost remembers the target of its
// NukePaint order (the game holds the same target in CastNuclearStrike, the
// order it moves into range with and leaves straight for NukeTrack). While it
// holds NukeTrack the target is a nuke dot when visible (or under
// CompleteMapInformation), and the first NukeTrack frame raises NukeDetect, at
// the target or at Positions::Unknown when hidden. Every live unit counts,
// published or not.
void GameDataProjection::project_nukes(const BwrFrame& frame, FrameSample& sample,
                                       BWAPI::GameData& data) {
  sample.nuke_detects.clear();
  if (frame.units == nullptr) return;
  const BwrUnits& t = *frame.units;
  const bool complete_map = data.flags[BWAPI::Flag::CompleteMapInformation];
  const auto visible = [&](int x, int y) {
    const int tile_x = x / 32;
    const int tile_y = y / 32;
    return x >= 0 && y >= 0 && tile_x < data.mapWidth && tile_y < data.mapHeight &&
           data.isVisible[tile_x][tile_y];
  };
  const int kMaxNukeDots = static_cast<int>(sizeof(data.nukeDots) / sizeof(data.nukeDots[0]));
  int dots = 0;
  for (const auto& unit : sample.units) {
    if (unit.type != BWAPI::UnitTypes::Enum::Terran_Ghost) continue;
    auto& track = unit_track(unit.id);
    if ((unit.order == BWAPI::Orders::Enum::NukePaint ||
         unit.order == BWAPI::Orders::Enum::CastNuclearStrike) &&
        known(t.order_target_x[unit.row]) && known(t.order_target_y[unit.row])) {
      track.nuke_x = t.order_target_x[unit.row];
      track.nuke_y = t.order_target_y[unit.row];
    }
    if (unit.order != BWAPI::Orders::Enum::NukeTrack) {
      track.nuke_detected = false;
      continue;
    }
    const bool seen = complete_map || visible(track.nuke_x, track.nuke_y);
    if (seen && dots < kMaxNukeDots) {
      data.nukeDots[dots].x = track.nuke_x;
      data.nukeDots[dots].y = track.nuke_y;
      ++dots;
    }
    if (!track.nuke_detected) {
      track.nuke_detected = true;
      sample.nuke_detects.emplace_back(seen ? track.nuke_x : BWAPI::Positions::Unknown.x,
                                       seen ? track.nuke_y : BWAPI::Positions::Unknown.y);
    }
  }
  data.nukeDotCount = dots;
}

}  // namespace BWRAPI::Legacy44
