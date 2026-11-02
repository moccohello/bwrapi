#pragma once

#include "legacy44_common.h"

#include <BWAPI/Client/GameData.h>
#include <bwrapi.h>

#include <cstdint>
#include <deque>
#include <functional>
#include <string>
#include <unordered_map>
#include <vector>

namespace BWRAPI::Legacy44 {

// The two ABI calls that carry commands, so tests can stand in for the game.
class CommandSink {
 public:
  virtual ~CommandSink() = default;
  virtual BwrStatus submit(const BwrCommand* commands, std::uint32_t count, std::uint64_t* ids) = 0;
  virtual BwrStatus poll(std::uint64_t id, BwrResult* result) = 0;
};

// Where unit commands go. Without a sink (a read-only session) they are only
// logged as not executed.
struct CommandTarget {
  CommandSink* sink = nullptr;
  // Engine unit id of a BWAPI UnitID; 0 when the unit is gone or unknown.
  std::function<std::uint64_t(int)> engine_unit;
  // Engine slot of a BWAPI player ID; -1 when no player has it.
  std::function<int(int)> player_slot;
};

// Commands a BWAPI client wrote into GameData. The official client-local
// settings (SetLatCom, SetGui, EnableFlag on the client's first received match
// frame) are applied here; unit commands are submitted to the game through the
// sink, once per frame and without waiting, and their results are logged when
// they finish. Above command optimizer level 0 the BWAPI 4.4 optimizer rules
// group the frame's compatible unit commands (same type, target, position and
// `extra` argument after the optimizer's rewrites) into one multi-unit
// command instead of one command per unit. Game commands with a game effect
// (pause, speed, text, alliance, vision, minimap ping, replay reveal, map) go
// ahead of them in the same batch; outside a network match a SendText whose
// text is a known cheat string becomes the game's own cheat command, as
// GameImpl::vSendTextEx does. The others are counted and logged as not
// executed. Draw shapes are only counted and cleared here
// (the in-game server shows them before, through BwapiServer::set_draw). No
// result is ever reported back to the client.
class CommandAdapter {
 public:
  struct Counts {
    std::uint64_t unit_commands = 0;
    std::uint64_t game_commands = 0;
    std::uint64_t shapes = 0;
    std::uint64_t local_settings = 0;
    std::uint64_t submitted = 0;
  };

  // Reopens the flag window for a match whose first frame is about to be
  // delivered (the client runs onStart there), and clears the optimizer
  // setting, as the official optimizer does at a match boundary.
  void start_match();

  // Drains the command buffers after a client update. `frame` is the game
  // frame the update belongs to; flags may only be enabled on the first match
  // frame the client receives, as in GameImpl::onStart.
  void drain(BWAPI::GameData& data, std::uint32_t frame, bool game_ready,
             const CommandTarget& target = {});
  const Counts& counts() const { return counts_; }

  // The selections and game commands the official server would issue for
  // the last drain's submitted unit commands, for the APM counter. A command
  // outside a group is one selection (of both units for Archon Warp and Dark
  // Archon Meld; none for Unload from optimizer level 2) and one command, as
  // UnitImpl::issueCommand runs it; the grouped commands of one key and
  // split flag are one selection and one command per twelve of them, as
  // CommandOptimizer::flush runs its groups of 12. Nothing counts when the
  // batch is not submitted.
  struct Actions {
    int selections = 0;
    int commands = 0;
  };
  const Actions& last_actions() const { return actions_; }
  // The last drain's unit commands handed to the game, as the client wrote
  // them (before the optimizer's rewrites), for latency compensation. Empty
  // when the batch was not submitted.
  const std::vector<BWAPIC::UnitCommand>& submitted_unit_commands() const { return submitted_; }

 private:
  // The parts of a client unit command that decide which commands may share
  // one game command: the type (after the optimizer's rewrites), the `extra`
  // argument (a unit, tech or upgrade type, or a train slot), the target unit,
  // the position, and the per-unit split flag CommandOptimizer::flush
  // compares for the command types whose executed order depends on the unit
  // (a building for Load, Attack_Unit, Unload_All and Cancel_Morph;
  // sieged, cloaked or burrowed for Use_Tech).
  struct CommandKey {
    int type;
    int extra;
    int target;
    int x;
    int y;
    int split;
    bool operator==(const CommandKey& other) const {
      return type == other.type && extra == other.extra && target == other.target && x == other.x &&
             y == other.y && split == other.split;
    }
  };
  struct CommandKeyHash {
    std::size_t operator()(const CommandKey& key) const;
  };
  // A command the optimizer takes, held until the direct commands are out.
  struct Grouped {
    CommandKey key;
    std::uint64_t actor;
    BwrCommand command;
  };

  void poll_results(CommandSink& sink, std::uint32_t frame);
  bool log_due();
  // Whether the command optimizer may group this unit command with its equals
  // of the frame, and the rewrites that take on the way; `split` receives the
  // per-unit flag the official flush compares besides the key.
  bool optimize_command(BWAPIC::UnitCommand& command, const BWAPI::GameData& data, int& split);

  Counts counts_;
  Actions actions_;
  std::uint64_t reports_ = 0;
  // The next game-ready drain is the client's first received match frame,
  // where GameImpl still accepts EnableFlag.
  bool first_match_frame_ = false;
  // The client's command optimizer level, clamped to 0..4; 0 leaves every
  // unit command by itself.
  int optimizer_level_ = 0;
  // GameImpl::cheatFlags: the cheat set the last cheat command carried; the
  // one-shot cheats are taken back out of it after sending.
  std::uint32_t cheat_flags_ = 0;
  // Submitted ids whose result is not final yet; the oldest drop out when full.
  std::deque<std::uint64_t> pending_;
  // Submission buffers, reused so a frame costs no allocation.
  std::vector<std::uint64_t> actors_;
  std::vector<BwrCommand> batch_;
  std::vector<std::uint64_t> ids_;
  std::vector<std::string> texts_;
  std::vector<std::uint64_t> group_actors_;
  std::vector<BWAPIC::UnitCommand> submitted_;
  std::vector<Grouped> grouped_;
  std::vector<bool> taken_;
  // Grouped commands of the frame per key and split flag, repeats included:
  // the official optimizer keeps a unit's repeat in its group of 12.
  std::unordered_map<CommandKey, int, CommandKeyHash> key_commands_;
};

}  // namespace BWRAPI::Legacy44
