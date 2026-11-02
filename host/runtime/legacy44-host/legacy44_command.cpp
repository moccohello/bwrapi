#include "legacy44_command.h"

#include <BWAPI/Client/Command.h>
#include <BWAPI/Client/UnitCommand.h>
#include <BWAPI/TechType.h>
#include <BWAPI/UnitType.h>

#include <algorithm>
#include <cctype>
#include <cstring>
#include <iostream>
#include <optional>
#include <string>
#include <string_view>
#include <vector>

namespace BWRAPI::Legacy44 {

namespace {

constexpr std::size_t kPendingLimit = 256;

// Most unit ids of one grouped command: CommandOptimizer::flush groups 12,
// one game selection.
constexpr std::uint32_t kGroupUnits = 12;

// How one BWAPI unit command maps onto a BwrCommand: its kind and which of
// the UnitCommand fields the game command reads.
struct Shape {
  std::uint32_t kind;
  bool queue = false;   // extra is the shift-queue flag
  bool pos = false;     // x, y are pixels
  bool tile = false;    // x, y are build tiles
  bool target = false;  // targetIndex is a unit
  bool type = false;    // extra is a unit, tech or upgrade type
  bool slot = false;    // extra is the train queue slot
};

// BWAPI 4.4 UnitCommand.cpp: the commands that take a shift-queue flag in
// `extra` are those of UnitCommand::isQueued(); Build, Build_Addon, Train,
// Morph, Research, Upgrade and Use_Tech* carry their type in `extra`;
// Cancel_Train_Slot carries the slot; Build, Land and Place_COP carry tile
// coordinates in x and y. Build_Addon has no position: the addon tile is
// derived from the producer.
std::optional<Shape> shape_of(int type) {
  namespace E = BWAPI::UnitCommandTypes::Enum;
  switch (type) {
    case E::Move: return Shape{BWR_COMMAND_MOVE, true, true};
    case E::Attack_Move: return Shape{BWR_COMMAND_ATTACK_POSITION, true, true};
    case E::Attack_Unit: return Shape{BWR_COMMAND_ATTACK_UNIT, true, false, false, true};
    case E::Patrol: return Shape{BWR_COMMAND_PATROL, true, true};
    case E::Hold_Position: return Shape{BWR_COMMAND_HOLD_POSITION, true};
    case E::Stop: return Shape{BWR_COMMAND_STOP, true};
    case E::Right_Click_Position: return Shape{BWR_COMMAND_RIGHT_CLICK_POSITION, true, true};
    case E::Right_Click_Unit: return Shape{BWR_COMMAND_SMART_TARGET_UNIT, true, false, false, true};
    case E::Follow: return Shape{BWR_COMMAND_FOLLOW, true, false, false, true};
    case E::Gather: return Shape{BWR_COMMAND_GATHER, true, false, false, true};
    case E::Repair: return Shape{BWR_COMMAND_REPAIR, true, false, false, true};
    case E::Return_Cargo: return Shape{BWR_COMMAND_RETURN_CARGO, true};
    case E::Build: return Shape{BWR_COMMAND_BUILD, false, false, true, false, true};
    case E::Build_Addon: return Shape{BWR_COMMAND_BUILD_ADDON, false, false, false, false, true};
    case E::Train: return Shape{BWR_COMMAND_TRAIN, false, false, false, false, true};
    case E::Morph: return Shape{BWR_COMMAND_MORPH, false, false, false, false, true};
    case E::Research: return Shape{BWR_COMMAND_RESEARCH, false, false, false, false, true};
    case E::Upgrade: return Shape{BWR_COMMAND_UPGRADE, false, false, false, false, true};
    case E::Set_Rally_Position: return Shape{BWR_COMMAND_SET_RALLY_POSITION, false, true};
    case E::Set_Rally_Unit: return Shape{BWR_COMMAND_SET_RALLY_UNIT, false, false, false, true};
    case E::Burrow: return Shape{BWR_COMMAND_BURROW};
    case E::Unburrow: return Shape{BWR_COMMAND_UNBURROW};
    case E::Cloak: return Shape{BWR_COMMAND_CLOAK};
    case E::Decloak: return Shape{BWR_COMMAND_DECLOAK};
    case E::Siege: return Shape{BWR_COMMAND_SIEGE};
    case E::Unsiege: return Shape{BWR_COMMAND_UNSIEGE};
    case E::Lift: return Shape{BWR_COMMAND_LIFT};
    case E::Land: return Shape{BWR_COMMAND_LAND, false, false, true};
    case E::Load: return Shape{BWR_COMMAND_LOAD, true, false, false, true};
    case E::Unload: return Shape{BWR_COMMAND_UNLOAD, false, false, false, true};
    case E::Unload_All: return Shape{BWR_COMMAND_UNLOAD_ALL, true};
    case E::Unload_All_Position: return Shape{BWR_COMMAND_UNLOAD_ALL_POSITION, true, true};
    case E::Halt_Construction: return Shape{BWR_COMMAND_HALT_CONSTRUCTION};
    case E::Cancel_Construction: return Shape{BWR_COMMAND_CANCEL_CONSTRUCTION};
    case E::Cancel_Addon: return Shape{BWR_COMMAND_CANCEL_ADDON};
    case E::Cancel_Train: return Shape{BWR_COMMAND_CANCEL_TRAIN};
    case E::Cancel_Train_Slot: return Shape{BWR_COMMAND_CANCEL_TRAIN_SLOT, false, false, false, false, false, true};
    case E::Cancel_Morph: return Shape{BWR_COMMAND_CANCEL_MORPH};
    case E::Cancel_Research: return Shape{BWR_COMMAND_CANCEL_RESEARCH};
    case E::Cancel_Upgrade: return Shape{BWR_COMMAND_CANCEL_UPGRADE};
    case E::Use_Tech: return Shape{BWR_COMMAND_USE_TECH, false, false, false, false, true};
    case E::Use_Tech_Position: return Shape{BWR_COMMAND_USE_TECH_POSITION, false, true, false, false, true};
    case E::Use_Tech_Unit: return Shape{BWR_COMMAND_USE_TECH_UNIT, false, false, false, true, true};
    case E::Place_COP: return Shape{BWR_COMMAND_PLACE_COP, false, false, true};
    default: return std::nullopt;
  }
}

// Fills `out` (whose `units` points at `actor`) from a client command. Returns
// why it cannot be sent, or an empty string.
std::string convert(const BWAPIC::UnitCommand& command, const CommandTarget& target,
                    std::uint64_t& actor, BwrCommand& out) {
  const auto shape = shape_of(command.type.getID());
  if (!shape) return "unit command type " + std::to_string(command.type.getID()) + " has no game command";
  actor = target.engine_unit ? target.engine_unit(command.unitIndex) : 0;
  if (actor == 0) return "unit " + std::to_string(command.unitIndex) + " is not a live unit";
  out = {};
  out.size = sizeof(BwrCommand);
  out.kind = shape->kind;
  out.units = &actor;
  out.unit_count = 1;
  if (shape->target && command.targetIndex >= 0) {
    out.target = target.engine_unit ? target.engine_unit(command.targetIndex) : 0;
    if (out.target == 0) return "target unit " + std::to_string(command.targetIndex) + " is not a live unit";
  }
  if (shape->queue && command.extra != 0) out.flags |= BWR_COMMAND_FLAG_QUEUED;
  if (shape->pos || shape->tile) {
    out.x = command.x;
    out.y = command.y;
  }
  if (shape->tile) out.flags |= BWR_COMMAND_FLAG_TILE;
  if (shape->type) out.type_id = static_cast<std::uint32_t>(command.extra);
  if (shape->slot) out.slot = static_cast<std::uint32_t>(command.extra);
  return {};
}

// The game command BWAPI's GameImpl runs for a client command, or 0 for one
// that has no game effect here.
std::uint32_t game_kind_of(int type) {
  namespace C = BWAPIC::CommandType;
  switch (type) {
    case C::PauseGame: return BWR_COMMAND_PAUSE;
    case C::ResumeGame: return BWR_COMMAND_RESUME;
    case C::RestartGame: return BWR_COMMAND_RESTART;
    case C::LeaveGame: return BWR_COMMAND_LEAVE;
    case C::SetLocalSpeed: return BWR_COMMAND_SET_SPEED;
    case C::SetFrameSkip: return BWR_COMMAND_SET_FRAME_SKIP;
    case C::Printf: return BWR_COMMAND_PRINT_TEXT;
    case C::SendText: return BWR_COMMAND_SEND_TEXT;
    case C::SetScreenPosition: return BWR_COMMAND_SET_SCREEN_POSITION;
    case C::SetAllies: return BWR_COMMAND_SET_ALLIES;
    case C::SetVision: return BWR_COMMAND_SET_VISION;
    case C::PingMinimap: return BWR_COMMAND_PING_MINIMAP;
    case C::SetRevealAll: return BWR_COMMAND_SET_REVEAL_ALL;
    case C::SetMap: return BWR_COMMAND_SET_MAP;
    default: return 0;
  }
}

// Longest text a game command carries (bytes, without the NUL).
constexpr std::size_t kTextLimit = 191;

// BW::CheatFlags: each known cheat string and the flag bit the game's own
// cheat command takes. GameImpl::vSendTextEx sends the whole running set and
// takes the one-shot cheats back out of it.
struct Cheat {
  std::string_view name;
  std::uint32_t flag;
  bool once;
};

constexpr Cheat kCheats[] = {
    {"black sheep wall", 0x1, false},          {"operation cwal", 0x2, false},
    {"power overwhelming", 0x4, false},        {"something for nothing", 0x8, true},
    {"show me the money", 0x10, true},         {"game over man", 0x40, false},
    {"there is no cow level", 0x80, false},    {"staying alive", 0x100, false},
    {"ophelia", 0x200, false},                 {"the gathering", 0x800, false},
    {"medieval man", 0x1000, false},           {"modify the phase variance", 0x2000, false},
    {"war aint what it used to be", 0x4000, false}, {"food for thought", 0x20000, false},
    {"whats mine is mine", 0x40000, true},     {"breathe deep", 0x80000, true},
    {"noglues", 0x20000000, false},
};

// BW::getCheatFlag: the cheat whose string `text` is, whatever its case.
const Cheat* cheat_of(const std::string& text) {
  std::string lower;
  lower.reserve(text.size());
  for (const char c : text) lower.push_back(static_cast<char>(std::tolower(static_cast<unsigned char>(c))));
  for (const Cheat& cheat : kCheats)
    if (lower == cheat.name) return &cheat;
  return nullptr;
}

// Cuts `text` to the command limit without splitting a UTF-8 sequence.
std::string command_text(const char* text) {
  std::string value(text, strnlen(text, sizeof(BWAPI::GameData::strings[0])));
  if (value.size() <= kTextLimit) return value;
  std::size_t end = kTextLimit;
  while (end > 0 && (static_cast<unsigned char>(value[end]) & 0xc0) == 0x80) --end;
  value.resize(end);
  return value;
}

const char* state_name(std::uint32_t state) {
  static const char* const names[] = {"PENDING", "OK", "REJECTED", "UNAVAILABLE",
                                      "NOT_EXECUTED", "PANIC", "UNKNOWN"};
  return state < std::size(names) ? names[state] : "UNKNOWN";
}

// The unit facts the official optimizer reads from the live unit, from the
// frame's GameData: the projection of the same frame the official server
// reads its units in. An unknown unit has the None type.
BWAPI::UnitType unit_type_of(const BWAPI::GameData& data, int index) {
  if (index < 0 || index >= static_cast<int>(std::size(data.units))) return BWAPI::UnitTypes::None;
  return BWAPI::UnitType(data.units[index].type);
}

const BWAPI::UnitData* live_unit(const BWAPI::GameData& data, int index) {
  if (index < 0 || index >= static_cast<int>(std::size(data.units))) return nullptr;
  return &data.units[index];
}

// UnitInterface::isSieged: the tank's sieged mode is its unit type.
bool is_sieged(const BWAPI::GameData& data, int index) {
  const BWAPI::UnitType type = unit_type_of(data, index);
  return type == BWAPI::UnitTypes::Terran_Siege_Tank_Siege_Mode ||
         type == BWAPI::UnitTypes::Hero_Edmund_Duke_Siege_Mode;
}

// GameData's state flags, as UnitUpdate fills them.
bool is_cloaked(const BWAPI::GameData& data, int index) {
  const BWAPI::UnitData* unit = live_unit(data, index);
  return unit != nullptr && unit->isCloaked;
}

bool is_burrowed(const BWAPI::GameData& data, int index) {
  const BWAPI::UnitData* unit = live_unit(data, index);
  return unit != nullptr && unit->isBurrowed;
}

// The optimizer's flush splits Use_Tech groups by sieged, cloaked or
// burrowed.
bool toggled_state(const BWAPI::GameData& data, int index) {
  return is_cloaked(data, index) || is_burrowed(data, index) || is_sieged(data, index);
}

// PlayerImpl::isEnemy of the target's owner against self, through the
// client's player table.
bool is_enemy_target(const BWAPI::GameData& data, int index) {
  if (data.self < 0 || data.self >= static_cast<int>(std::size(data.players))) return false;
  const BWAPI::UnitData* unit = live_unit(data, index);
  if (unit == nullptr) return false;
  const int player = unit->player;
  return player >= 0 && player < static_cast<int>(std::size(data.players[data.self].isEnemy)) &&
         data.players[data.self].isEnemy[player];
}

}  // namespace

std::size_t CommandAdapter::CommandKeyHash::operator()(const CommandKey& key) const {
  std::size_t hash = 17;
  for (const int value : {key.type, key.extra, key.target, key.x, key.y, key.split})
    hash = hash * 31 + static_cast<std::size_t>(value);
  return hash;
}

// The BWAPI 4.4 CommandOptimizer rules: the rewrites of CommandOptimizer::add
// that make more commands equal, the commands each level refuses, and the
// per-unit split flag CommandOptimizer::flush compares besides the key, so a
// group stays homogeneous where the executed order depends on the unit (its
// type for Load, Attack_Unit, Unload_All and Cancel_Morph; its state for the
// remaining Use_Tech commands). A command that never groups returns false
// with the rewrites it picked up on the way, as the official issueCommand
// path executes it in that form.
bool CommandAdapter::optimize_command(BWAPIC::UnitCommand& command, const BWAPI::GameData& data,
                                       int& split) {
  namespace E = BWAPI::UnitCommandTypes::Enum;
  namespace T = BWAPI::TechTypes::Enum;
  const int type = command.type.getID();
  const auto shape = shape_of(type);
  // The optimizer takes no queued commands and no type without a game
  // command.
  if (!shape || (shape->queue && command.extra != 0)) return false;
  // Commands the official optimizer never takes, whatever the level.
  switch (type) {
    case E::Build:
    case E::Build_Addon:
    case E::Land:
    case E::Place_COP:
    case E::Research:
    case E::Upgrade:
      return false;
    default:
      break;
  }
  const BWAPI::UnitType this_type = unit_type_of(data, command.unitIndex);
  const BWAPI::UnitType targ_type = unit_type_of(data, command.targetIndex);
  const bool merge_tech = command.extra == T::Archon_Warp || command.extra == T::Dark_Archon_Meld;
  if (type == E::Use_Tech_Unit && merge_tech && optimizer_level_ < 4) return false;

  // The official rewrites, in official order. Attack_Unit keeps its target
  // and queue flag: only the type becomes the right click.
  const int original_type = type;
  const int original_target = command.targetIndex;
  const int original_extra = command.extra;
  const auto right_click_position = [&command] {
    command.type = BWAPI::UnitCommandTypes::Right_Click_Position;
    command.targetIndex = -1;
    command.extra = 0;
  };
  const auto right_click_unit = [&command] {
    command.type = BWAPI::UnitCommandTypes::Right_Click_Unit;
    command.x = 0;
    command.y = 0;
    command.extra = 0;
  };
  const bool rally_producer = this_type.canProduce() &&
                              this_type != BWAPI::UnitTypes::Protoss_Carrier &&
                              this_type != BWAPI::UnitTypes::Hero_Gantrithor &&
                              this_type != BWAPI::UnitTypes::Protoss_Reaver &&
                              this_type != BWAPI::UnitTypes::Hero_Warbringer;
  if (type == E::Attack_Unit) {
    if (this_type.canAttack() && command.targetIndex >= 0 && is_enemy_target(data, command.targetIndex))
      command.type = BWAPI::UnitCommandTypes::Right_Click_Unit;
  } else if (type == E::Move) {
    // A plain move is the game's own right click on the ground.
    right_click_position();
  } else if (type == E::Gather) {
    if (targ_type.isResourceContainer()) right_click_unit();
  } else if (type == E::Set_Rally_Position) {
    if (rally_producer) right_click_position();
  } else if (type == E::Set_Rally_Unit) {
    if (rally_producer) right_click_unit();
  } else if (type == E::Use_Tech_Unit && command.extra == T::Infestation &&
             (this_type == BWAPI::UnitTypes::Zerg_Queen || this_type == BWAPI::UnitTypes::Hero_Matriarch) &&
             targ_type == BWAPI::UnitTypes::Terran_Command_Center) {
    right_click_unit();
  } else if (type == E::Train) {
    // A carrier or reaver always trains its subordinate; one placeholder
    // type lets their commands group.
    if (this_type == BWAPI::UnitTypes::Protoss_Carrier ||
        this_type == BWAPI::UnitTypes::Hero_Gantrithor ||
        this_type == BWAPI::UnitTypes::Protoss_Reaver ||
        this_type == BWAPI::UnitTypes::Hero_Warbringer) {
      command.type = BWAPI::UnitCommandTypes::Train;
      command.targetIndex = -1;
      command.x = 0;
      command.y = 0;
      command.extra = BWAPI::UnitTypes::Protoss_Interceptor;
    }
  } else if (type == E::Use_Tech) {
    // The toggle techs become their specific commands after the unit's live
    // state, which is what lets them group.
    switch (command.extra) {
      case T::Tank_Siege_Mode:
        command.type = is_sieged(data, command.unitIndex) ? BWAPI::UnitCommandTypes::Unsiege
                                                          : BWAPI::UnitCommandTypes::Siege;
        break;
      case T::Personnel_Cloaking:
      case T::Cloaking_Field:
        command.type = is_cloaked(data, command.unitIndex) ? BWAPI::UnitCommandTypes::Decloak
                                                           : BWAPI::UnitCommandTypes::Cloak;
        break;
      case T::Burrowing:
        command.type = is_burrowed(data, command.unitIndex) ? BWAPI::UnitCommandTypes::Unburrow
                                                            : BWAPI::UnitCommandTypes::Burrow;
        break;
      default:
        break;
    }
    if (command.type.getID() != E::Use_Tech) {
      command.targetIndex = -1;
      command.x = 0;
      command.y = 0;
      command.extra = 0;
    }
  }

  // Buildings never group at optimizer level 1.
  if (optimizer_level_ <= 1 && this_type != BWAPI::UnitTypes::None && this_type.isBuilding())
    return false;

  // Position commands never group at levels 1 and 2, by their original type,
  // so a rewritten move still stays by itself.
  const bool position = original_type == E::Attack_Move || original_type == E::Move ||
                        original_type == E::Patrol || original_type == E::Right_Click_Position ||
                        original_type == E::Set_Rally_Position ||
                        original_type == E::Unload_All_Position || original_type == E::Use_Tech_Position;
  if (optimizer_level_ <= 2 && position) return false;

  if (optimizer_level_ >= 4) {
    if (original_type == E::Use_Tech_Unit && BWAPI::TechType(command.extra).targetsPosition() &&
        live_unit(data, command.targetIndex) != nullptr) {
      // A unit-targeted tech with a position shape becomes the position
      // command at the target's live place; the alignment below does not
      // take it (it checks the original type).
      command.type = BWAPI::UnitCommandTypes::Use_Tech_Position;
      command.x = data.units[command.targetIndex].positionX;
      command.y = data.units[command.targetIndex].positionY;
      command.targetIndex = -1;
    } else if (position) {
      // Nearby positions merge through 32 pixel alignment, which also puts
      // the rewritten move and rally commands back into their own shape.
      command.type = BWAPI::UnitCommandType(original_type);
      command.targetIndex = original_target;
      command.extra = original_extra;
      command.x &= ~0x1F;
      command.y &= ~0x1F;
    } else if (original_type == E::Use_Tech_Unit && merge_tech) {
      // The archon merges lose their partner, so the templar commands of a
      // pair join one group.
      command.targetIndex = -1;
      command.x = 0;
      command.y = 0;
    }
  }

  // The flush's split flag, by the command type the rewrites left.
  const int final_type = command.type.getID();
  if (final_type == E::Attack_Unit || final_type == E::Unload_All || final_type == E::Load ||
      final_type == E::Cancel_Morph)
    split = this_type.isBuilding() ? 1 : 0;
  else if (final_type == E::Use_Tech)
    split = toggled_state(data, command.unitIndex) ? 1 : 0;
  else
    split = 0;
  return true;
}

bool CommandAdapter::log_due() {
  // The first reports are logged one by one, later ones every 256th.
  const bool due = reports_ < 32 || reports_ % 256 == 0;
  ++reports_;
  return due;
}

// Reads the result table without waiting; finished ids leave the ring.
void CommandAdapter::poll_results(CommandSink& sink, std::uint32_t frame) {
  for (auto it = pending_.begin(); it != pending_.end();) {
    BwrResult result{};
    const BwrStatus status = sink.poll(*it, &result);
    if (status == BWR_OK && result.state == BWR_RESULT_PENDING) {
      ++it;
      continue;
    }
    if (log_due())
      std::cerr << "[command] status=" << (status == BWR_OK ? state_name(result.state) : "UNKNOWN")
                << " id=" << *it << " frame=" << frame << '\n';
    it = pending_.erase(it);
  }
}

void CommandAdapter::start_match() {
  first_match_frame_ = true;
  // The official optimizer resets at a match boundary, so a new match starts
  // at level 0 whatever the last one set; GameImpl clears its cheat set with
  // the rest of the match state.
  optimizer_level_ = 0;
  cheat_flags_ = 0;
}

void CommandAdapter::drain(BWAPI::GameData& data, std::uint32_t frame, bool game_ready,
                           const CommandTarget& target) {
  // The first game-ready drain of a match is the client's first received
  // match frame (onStart); menu drains leave the window as it is.
  const bool first_match_frame = game_ready && first_match_frame_;
  if (game_ready) first_match_frame_ = false;
  const auto clamp_count = [](int count, int capacity) {
    return std::clamp(count, 0, capacity);
  };
  const int game_count = clamp_count(data.commandCount, BWAPI::GameData::MAX_COMMANDS);
  const int unit_count = clamp_count(data.unitCommandCount, BWAPI::GameData::MAX_UNIT_COMMANDS);
  const int shape_count = clamp_count(data.shapeCount, BWAPI::GameData::MAX_SHAPES);

  std::uint64_t not_executed_game = 0;
  batch_.clear();
  texts_.clear();
  // Commands point at their text, so the strings must not move.
  texts_.reserve(static_cast<std::size_t>(game_count));
  for (int index = 0; index < game_count; ++index) {
    const auto& command = data.commands[index];
    if (const std::uint32_t kind = game_kind_of(command.type); kind != 0) {
      if (target.sink == nullptr) {
        ++not_executed_game;
        continue;
      }
      // GameImpl::printf and setScreenPosition do nothing while the GUI is
      // off.
      if ((kind == BWR_COMMAND_PRINT_TEXT || kind == BWR_COMMAND_SET_SCREEN_POSITION) && !data.hasGUI)
        continue;
      BwrCommand converted{};
      converted.size = sizeof(BwrCommand);
      converted.kind = kind;
      if (kind == BWR_COMMAND_SET_SPEED || kind == BWR_COMMAND_SET_FRAME_SKIP) converted.x = command.value1;
      if (kind == BWR_COMMAND_SET_SCREEN_POSITION || kind == BWR_COMMAND_PING_MINIMAP) {
        converted.x = command.value1;
        converted.y = command.value2;
      }
      // Server::processCommands: setAlliance(getPlayer(v1), v2 != 0, v2 == 2),
      // setVision(getPlayer(v1), v2 != 0), setRevealAll(v1 != 0). v1 is a
      // player ID; the game takes its slot, and an ID without a player does
      // nothing.
      if (kind == BWR_COMMAND_SET_ALLIES || kind == BWR_COMMAND_SET_VISION) {
        const int slot = target.player_slot ? target.player_slot(command.value1) : -1;
        if (slot < 0) {
          ++not_executed_game;
          continue;
        }
        converted.x = slot;
        if (kind == BWR_COMMAND_SET_ALLIES)
          converted.y = command.value2 == 0 ? 0 : command.value2 == 2 ? 2 : 1;
        else
          converted.y = command.value2 != 0 ? 1 : 0;
      }
      if (kind == BWR_COMMAND_SET_REVEAL_ALL) converted.x = command.value1 != 0 ? 1 : 0;
      if (kind == BWR_COMMAND_PRINT_TEXT || kind == BWR_COMMAND_SEND_TEXT || kind == BWR_COMMAND_SET_MAP) {
        if (command.value1 < 0 || command.value1 >= std::min(data.stringCount, BWAPI::GameData::MAX_STRINGS)) {
          ++not_executed_game;
          continue;
        }
        texts_.push_back(command_text(data.strings[command.value1]));
        // A cut map file name would name another file.
        if (kind == BWR_COMMAND_SET_MAP &&
            texts_.back().size() != strnlen(data.strings[command.value1], sizeof(data.strings[0]))) {
          texts_.pop_back();
          ++not_executed_game;
          continue;
        }
        converted.text = texts_.back().c_str();
        if (kind == BWR_COMMAND_SEND_TEXT) {
          // GameImpl::vSendTextEx: outside a network match a known cheat
          // string becomes the game's own cheat command, not a chat line.
          if (!data.isMultiplayer && !data.isReplay) {
            if (const Cheat* cheat = cheat_of(texts_.back())) {
              cheat_flags_ ^= cheat->flag;
              const std::int32_t flags = static_cast<std::int32_t>(cheat_flags_);
              if (cheat->once) cheat_flags_ ^= cheat->flag;
              texts_.pop_back();
              BwrCommand cheat_command{};
              cheat_command.size = sizeof(BwrCommand);
              cheat_command.kind = BWR_COMMAND_USE_CHEAT;
              cheat_command.x = flags;
              batch_.push_back(cheat_command);
              continue;
            }
          }
          converted.type_id = command.value2 != 0 ? 1 : 0;
        }
      }
      batch_.push_back(converted);
    } else if (command.type == BWAPIC::CommandType::SetLatCom) {
      // Official server rule: the shared GameData flag is the client's own
      // isLatComEnabled state.
      data.hasLatCom = command.value1 == 1;
      ++counts_.local_settings;
    } else if (command.type == BWAPIC::CommandType::SetGui) {
      // GameImpl::setGUI: the flag BWAPIClient checks before it writes shapes
      // and GameImpl before printf and setScreenPosition, and the game's
      // screen updates, which stop while it is off.
      data.hasGUI = command.value1 == 1;
      ++counts_.local_settings;
      if (target.sink != nullptr) {
        BwrCommand converted{};
        converted.size = sizeof(BwrCommand);
        converted.kind = BWR_COMMAND_SET_GUI;
        converted.x = data.hasGUI ? 1 : 0;
        batch_.push_back(converted);
      }
    } else if (command.type == BWAPIC::CommandType::EnableFlag) {
      // GameImpl applies the flags the client queues in onStart, on the
      // first match frame it receives, and only for the fixed Flag::Enum
      // range; the flag is host-local GameData state as upstream.
      if (first_match_frame && command.value1 >= 0 && command.value1 < BWAPI::Flag::Max)
        data.flags[command.value1] = true;
      ++counts_.local_settings;
    } else if (command.type == BWAPIC::CommandType::SetCommandOptimizerLevel) {
      // GameImpl clamps the level to 0..4; it lasts until the client changes
      // it or the match ends.
      optimizer_level_ = std::clamp(command.value1, 0, 4);
      ++counts_.local_settings;
    } else {
      ++not_executed_game;
    }
  }
  counts_.game_commands += not_executed_game;
  counts_.unit_commands += static_cast<std::uint64_t>(unit_count);
  counts_.shapes += static_cast<std::uint64_t>(shape_count);

  // Results of earlier frames first, so a command is polled one frame after
  // it was submitted and the frame loop never waits for the game window.
  if (target.sink != nullptr) poll_results(*target.sink, frame);

  std::uint64_t unit_not_executed = target.sink != nullptr ? 0 : static_cast<std::uint64_t>(unit_count);
  actions_ = {};
  submitted_.clear();
  if (target.sink != nullptr) {
    Actions actions;
    // Each command outside a group points at its own actor slot, so the slots
    // are sized up front. Game commands go first, as GameImpl runs them
    // before unit commands.
    actors_.resize(static_cast<std::size_t>(unit_count));
    grouped_.clear();
    key_commands_.clear();
    std::size_t next_actor = 0;
    for (int index = 0; index < unit_count; ++index) {
      // The optimizer rewrites its own copy; a command it refuses the group
      // still runs in the client's form, as the official issueCommand path
      // executes its untouched command.
      const BWAPIC::UnitCommand command = data.unitCommands[index];
      BWAPIC::UnitCommand optimized = command;
      int split = 0;
      const bool grouped = optimizer_level_ > 0 && optimize_command(optimized, data, split);
      const BWAPIC::UnitCommand& sent = grouped ? optimized : command;
      std::uint64_t actor = 0;
      BwrCommand converted;
      const std::string reason = convert(sent, target, actor, converted);
      if (!reason.empty()) {
        ++unit_not_executed;
        if (log_due())
          std::cerr << "[command] status=not_executed reason=" << reason << " frame=" << frame << '\n';
        continue;
      }
      // UnitImpl::prepareIssueCommand buffers every accepted command, a
      // grouped repeat included, before the optimizer rewrites it.
      submitted_.push_back(command);
      if (!grouped) {
        namespace E = BWAPI::UnitCommandTypes::Enum;
        namespace T = BWAPI::TechTypes::Enum;
        const int type = command.type.getID();
        const bool merge = type == E::Use_Tech_Unit && command.targetIndex >= 0 &&
                           (command.extra == T::Archon_Warp || command.extra == T::Dark_Archon_Meld);
        if (merge || type != E::Unload || optimizer_level_ < 2) ++actions.selections;
        ++actions.commands;
        actors_[next_actor] = actor;
        converted.units = &actors_[next_actor];
        ++next_actor;
        batch_.push_back(converted);
        continue;
      }
      const CommandKey key{sent.type.getID(), sent.extra, sent.targetIndex, sent.x, sent.y, split};
      ++key_commands_[key];
      grouped_.push_back(Grouped{key, actor, converted});
    }
    // CommandOptimizer::flush runs after the direct commands: command type by
    // type id, and within a type the newest command first, each group taking
    // up to 12 commands of its key in that order.
    std::stable_sort(grouped_.begin(), grouped_.end(),
                     [](const Grouped& a, const Grouped& b) { return a.key.type < b.key.type; });
    group_actors_.clear();
    // The batch points into it, so it must not grow past this.
    group_actors_.reserve(grouped_.size());
    taken_.assign(grouped_.size(), false);
    for (std::size_t begin = 0, end = 0; begin < grouped_.size(); begin = end) {
      while (end < grouped_.size() && grouped_[end].key.type == grouped_[begin].key.type) ++end;
      for (std::size_t lead = end; lead-- > begin;) {
        if (taken_[lead]) continue;
        BwrCommand out = grouped_[lead].command;
        out.units = group_actors_.data() + group_actors_.size();
        out.unit_count = 0;
        for (std::size_t at = lead + 1; at-- > begin && out.unit_count < kGroupUnits;) {
          if (taken_[at] || !(grouped_[at].key == grouped_[lead].key)) continue;
          taken_[at] = true;
          group_actors_.push_back(grouped_[at].actor);
          ++out.unit_count;
        }
        batch_.push_back(out);
      }
    }
    for (const auto& [key, commands] : key_commands_) {
      const int groups_of_12 = (commands + 11) / 12;
      actions.selections += groups_of_12;
      actions.commands += groups_of_12;
    }
    if (!batch_.empty()) {
      ids_.resize(batch_.size());
      const BwrStatus status = target.sink->submit(batch_.data(), static_cast<std::uint32_t>(batch_.size()), ids_.data());
      if (status == BWR_OK) {
        actions_ = actions;
        counts_.submitted += batch_.size();
        for (const std::uint64_t id : ids_) {
          if (pending_.size() == kPendingLimit) pending_.pop_front();
          pending_.push_back(id);
        }
      } else {
        submitted_.clear();
        unit_not_executed += batch_.size();
        if (log_due())
          std::cerr << "[command] status=not_executed reason=bwr_submit failed with status "
                    << static_cast<int>(status) << " frame=" << frame << '\n';
      }
    }
  }

  if (not_executed_game != 0 || (target.sink == nullptr && unit_not_executed != 0)) {
    if (log_due()) {
      std::cerr << "[command] status=not_executed reason="
                << (target.sink != nullptr ? "game commands are not executed" : "command submission is not available")
                << " frame=" << frame << " unit_commands=" << unit_not_executed
                << " game_commands=" << not_executed_game << " total_unit_commands="
                << counts_.unit_commands << '\n';
    }
  }
  data.commandCount = 0;
  data.shapeCount = 0;
  data.stringCount = 0;
  data.unitCommandCount = 0;
}

}  // namespace BWRAPI::Legacy44
