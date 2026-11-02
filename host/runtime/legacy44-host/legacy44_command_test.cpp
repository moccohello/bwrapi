// Client commands: local settings apply, unit and game commands are submitted
// through the sink, the rest are not executed. The official chat commands run
// as game commands, and outside a network match a cheat text becomes the
// game's own cheat command.
#include "legacy44_chat.h"
#include "legacy44_command.h"

#include <BWAPI/Client/Command.h>
#include <BWAPI/Client/Shape.h>
#include <BWAPI/Client/UnitCommand.h>
#include <BWAPI/Color.h>
#include <BWAPI/CoordinateType.h>
#include <BWAPI/TechType.h>

#include <cstring>
#include <functional>
#include <iostream>
#include <memory>
#include <sstream>
#include <string>
#include <vector>

using namespace BWRAPI::Legacy44;

namespace {

int failures = 0;

#define CHECK(condition)                                                \
  do {                                                                  \
    if (!(condition)) {                                                 \
      std::cerr << __FILE__ << ':' << __LINE__ << ": check failed: "    \
                << #condition << '\n';                                  \
      ++failures;                                                       \
    }                                                                   \
  } while (false)

void add_game_command(BWAPI::GameData& data, BWAPIC::CommandType::Enum type, int value1 = 0) {
  data.commands[data.commandCount++] = BWAPIC::Command(type, value1, 0);
}

void local_settings_apply() {
  auto data = std::make_unique<BWAPI::GameData>();
  data->hasLatCom = true;
  data->hasGUI = true;
  add_game_command(*data, BWAPIC::CommandType::SetLatCom, 0);
  add_game_command(*data, BWAPIC::CommandType::SetGui, 0);
  add_game_command(*data, BWAPIC::CommandType::EnableFlag, BWAPI::Flag::UserInput);
  add_game_command(*data, BWAPIC::CommandType::SetCommandOptimizerLevel, 2);
  CommandAdapter adapter;
  adapter.start_match();
  adapter.drain(*data, 0, true);
  CHECK(!data->hasLatCom && !data->hasGUI);
  CHECK(data->flags[BWAPI::Flag::UserInput]);
  CHECK(adapter.counts().local_settings == 4 && adapter.counts().game_commands == 0);
  CHECK(data->commandCount == 0 && data->unitCommandCount == 0 && data->shapeCount == 0 &&
        data->stringCount == 0);
}

// A flag may only be enabled on the client's first received match frame
// (onStart), whatever the game frame number is.
void flags_only_on_start() {
  auto data = std::make_unique<BWAPI::GameData>();
  CommandAdapter adapter;
  // The host attaches mid-match; the client's onStart still enables flags.
  adapter.start_match();
  add_game_command(*data, BWAPIC::CommandType::EnableFlag, BWAPI::Flag::UserInput);
  adapter.drain(*data, 700, true);
  CHECK(data->flags[BWAPI::Flag::UserInput]);
  add_game_command(*data, BWAPIC::CommandType::EnableFlag, BWAPI::Flag::CompleteMapInformation);
  adapter.drain(*data, 701, true);
  CHECK(!data->flags[BWAPI::Flag::CompleteMapInformation]);
  // Menu drains are not match frames.
  add_game_command(*data, BWAPIC::CommandType::EnableFlag, BWAPI::Flag::CompleteMapInformation);
  adapter.drain(*data, 0, false);
  CHECK(!data->flags[BWAPI::Flag::CompleteMapInformation]);
  // A new match reopens onStart on its first frame, still range-checked.
  adapter.start_match();
  add_game_command(*data, BWAPIC::CommandType::EnableFlag, BWAPI::Flag::CompleteMapInformation);
  adapter.drain(*data, 0, true);
  CHECK(data->flags[BWAPI::Flag::CompleteMapInformation]);
  add_game_command(*data, BWAPIC::CommandType::EnableFlag, 99);
  adapter.start_match();
  adapter.drain(*data, 0, true);
}

// Without a sink (read-only session) unit commands, like game commands and
// shapes, are not executed, and the buffers are cleared without a result.
void everything_else_is_not_executed() {
  auto data = std::make_unique<BWAPI::GameData>();
  data->unitCommandCount = 3;
  data->shapeCount = 2;
  data->stringCount = 1;
  add_game_command(*data, BWAPIC::CommandType::SendText, 0);
  add_game_command(*data, BWAPIC::CommandType::LeaveGame, 0);
  CommandAdapter adapter;
  adapter.drain(*data, 10, true);
  CHECK(adapter.counts().unit_commands == 3 && adapter.counts().shapes == 2);
  CHECK(adapter.counts().game_commands == 2 && adapter.counts().local_settings == 0);
  CHECK(data->commandCount == 0 && data->unitCommandCount == 0 && data->shapeCount == 0);
  CHECK(data->hasLatCom == false && data->hasGUI == false);
  adapter.drain(*data, 11, true);
  CHECK(adapter.counts().unit_commands == 3);
}

// A stand-in for bwr_submit / bwr_poll_result: records what the host sent.
struct FakeSink : CommandSink {
  struct Sent {
    BwrCommand command;
    std::uint64_t actor;
    std::vector<std::uint64_t> units;
  };
  std::vector<Sent> sent;
  int submit_calls = 0;
  BwrStatus submit_status = BWR_OK;
  std::uint32_t next_state = BWR_RESULT_OK;
  std::uint64_t next_id = 100;
  BwrStatus submit(const BwrCommand* commands, std::uint32_t count, std::uint64_t* ids) override {
    ++submit_calls;
    if (submit_status != BWR_OK) return submit_status;
    for (std::uint32_t i = 0; i < count; ++i) {
      sent.push_back({commands[i], commands[i].unit_count >= 1 ? commands[i].units[0] : 0,
                      {commands[i].units, commands[i].units + commands[i].unit_count}});
      ids[i] = next_id++;
    }
    return BWR_OK;
  }
  BwrStatus poll(std::uint64_t id, BwrResult* result) override {
    *result = {};
    result->id = id;
    result->state = next_state;
    return BWR_OK;
  }
};

// BWAPI unit N is engine unit 1000 + N; unit 99 is unknown. Player ID N is
// slot 7 - N for IDs 0..5; other IDs have no player.
CommandTarget target_for(FakeSink& sink) {
  return {&sink, [](int id) -> std::uint64_t { return id == 99 ? 0 : 1000 + id; },
          [](int id) { return id >= 0 && id <= 5 ? 7 - id : -1; }};
}

void add_unit_command(BWAPI::GameData& data, BWAPI::UnitCommandType type, int unit, int target = -1,
                      int x = 0, int y = 0, int extra = 0) {
  data.unitCommands[data.unitCommandCount++] = BWAPIC::UnitCommand{type, unit, target, x, y, extra};
}

// Sends one command from BWAPI unit 5 and returns what the sink received.
std::vector<FakeSink::Sent> submit_one(BWAPI::UnitCommandType type, int target = -1, int x = 0, int y = 0,
                                        int extra = 0) {
  auto data = std::make_unique<BWAPI::GameData>();
  add_unit_command(*data, type, 5, target, x, y, extra);
  FakeSink sink;
  CommandAdapter adapter;
  adapter.drain(*data, 7, true, target_for(sink));
  CHECK(sink.submit_calls == 1);
  CHECK(data->unitCommandCount == 0);
  return sink.sent;
}

// Every mapped command type reaches bwr_submit with the BwrCommand fields the
// BWAPI UnitCommand encodes: the shift-queue flag and the type or slot come
// from `extra`, positions from x/y (tiles for Build, Land and Place_COP).
void unit_commands_map_to_bwr_commands() {
  namespace T = BWAPI::UnitCommandTypes;
  const std::uint32_t queued = BWR_COMMAND_FLAG_QUEUED;
  const std::uint32_t tile = BWR_COMMAND_FLAG_TILE;
  struct Case {
    BWAPI::UnitCommandType type;
    std::uint32_t kind;
    std::uint32_t flags;
    bool uses_target, uses_position, uses_type, uses_slot;
  };
  // The arguments are x=64, y=96, target unit 6 and extra 5 (1 for a queueable
  // command, where extra is the shift flag); a field a command does not use
  // must stay zero.
  const Case cases[] = {
      {T::Move, BWR_COMMAND_MOVE, queued, false, true, false, false},
      {T::Attack_Move, BWR_COMMAND_ATTACK_POSITION, queued, false, true, false, false},
      {T::Attack_Unit, BWR_COMMAND_ATTACK_UNIT, queued, true, false, false, false},
      {T::Patrol, BWR_COMMAND_PATROL, queued, false, true, false, false},
      {T::Hold_Position, BWR_COMMAND_HOLD_POSITION, queued, false, false, false, false},
      {T::Stop, BWR_COMMAND_STOP, queued, false, false, false, false},
      {T::Right_Click_Position, BWR_COMMAND_RIGHT_CLICK_POSITION, queued, false, true, false, false},
      {T::Right_Click_Unit, BWR_COMMAND_SMART_TARGET_UNIT, queued, true, false, false, false},
      {T::Follow, BWR_COMMAND_FOLLOW, queued, true, false, false, false},
      {T::Gather, BWR_COMMAND_GATHER, queued, true, false, false, false},
      {T::Repair, BWR_COMMAND_REPAIR, queued, true, false, false, false},
      {T::Return_Cargo, BWR_COMMAND_RETURN_CARGO, queued, false, false, false, false},
      {T::Load, BWR_COMMAND_LOAD, queued, true, false, false, false},
      {T::Unload_All, BWR_COMMAND_UNLOAD_ALL, queued, false, false, false, false},
      {T::Unload_All_Position, BWR_COMMAND_UNLOAD_ALL_POSITION, queued, false, true, false, false},
      {T::Build, BWR_COMMAND_BUILD, tile, false, true, true, false},
      {T::Build_Addon, BWR_COMMAND_BUILD_ADDON, 0, false, false, true, false},
      {T::Train, BWR_COMMAND_TRAIN, 0, false, false, true, false},
      {T::Morph, BWR_COMMAND_MORPH, 0, false, false, true, false},
      {T::Research, BWR_COMMAND_RESEARCH, 0, false, false, true, false},
      {T::Upgrade, BWR_COMMAND_UPGRADE, 0, false, false, true, false},
      {T::Set_Rally_Position, BWR_COMMAND_SET_RALLY_POSITION, 0, false, true, false, false},
      {T::Set_Rally_Unit, BWR_COMMAND_SET_RALLY_UNIT, 0, true, false, false, false},
      {T::Burrow, BWR_COMMAND_BURROW, 0, false, false, false, false},
      {T::Unburrow, BWR_COMMAND_UNBURROW, 0, false, false, false, false},
      {T::Cloak, BWR_COMMAND_CLOAK, 0, false, false, false, false},
      {T::Decloak, BWR_COMMAND_DECLOAK, 0, false, false, false, false},
      {T::Siege, BWR_COMMAND_SIEGE, 0, false, false, false, false},
      {T::Unsiege, BWR_COMMAND_UNSIEGE, 0, false, false, false, false},
      {T::Lift, BWR_COMMAND_LIFT, 0, false, false, false, false},
      {T::Land, BWR_COMMAND_LAND, tile, false, true, false, false},
      {T::Unload, BWR_COMMAND_UNLOAD, 0, true, false, false, false},
      {T::Halt_Construction, BWR_COMMAND_HALT_CONSTRUCTION, 0, false, false, false, false},
      {T::Cancel_Construction, BWR_COMMAND_CANCEL_CONSTRUCTION, 0, false, false, false, false},
      {T::Cancel_Addon, BWR_COMMAND_CANCEL_ADDON, 0, false, false, false, false},
      {T::Cancel_Train, BWR_COMMAND_CANCEL_TRAIN, 0, false, false, false, false},
      {T::Cancel_Train_Slot, BWR_COMMAND_CANCEL_TRAIN_SLOT, 0, false, false, false, true},
      {T::Cancel_Morph, BWR_COMMAND_CANCEL_MORPH, 0, false, false, false, false},
      {T::Cancel_Research, BWR_COMMAND_CANCEL_RESEARCH, 0, false, false, false, false},
      {T::Cancel_Upgrade, BWR_COMMAND_CANCEL_UPGRADE, 0, false, false, false, false},
      {T::Use_Tech, BWR_COMMAND_USE_TECH, 0, false, false, true, false},
      {T::Use_Tech_Position, BWR_COMMAND_USE_TECH_POSITION, 0, false, true, true, false},
      {T::Use_Tech_Unit, BWR_COMMAND_USE_TECH_UNIT, 0, true, false, true, false},
      {T::Place_COP, BWR_COMMAND_PLACE_COP, tile, false, true, false, false},
  };
  for (const Case& c : cases) {
    const int extra = c.flags == queued ? 1 : 5;
    const auto sent = submit_one(c.type, 6, 64, 96, extra);
    CHECK(sent.size() == 1);
    if (sent.size() != 1) continue;
    const BwrCommand& got = sent[0].command;
    CHECK(got.size == sizeof(BwrCommand) && got.unit_count == 1);
    CHECK(sent[0].actor == 1005);
    CHECK(got.kind == c.kind);
    CHECK(got.flags == c.flags);
    CHECK(got.target == (c.uses_target ? 1006u : 0u));
    CHECK(got.x == (c.uses_position ? 64 : 0) && got.y == (c.uses_position ? 96 : 0));
    CHECK(got.type_id == (c.uses_type ? 5u : 0u));
    CHECK(got.slot == (c.uses_slot ? 5u : 0u));
  }
  // Without the shift flag a queueable command is not queued.
  CHECK(submit_one(T::Move, -1, 1, 2, 0)[0].command.flags == 0);
}

// One bwr_submit per frame carries the whole batch, one actor per command.
void batch_is_submitted_once() {
  auto data = std::make_unique<BWAPI::GameData>();
  add_unit_command(*data, BWAPI::UnitCommandTypes::Move, 1, -1, 10, 20);
  add_unit_command(*data, BWAPI::UnitCommandTypes::Stop, 2);
  add_unit_command(*data, BWAPI::UnitCommandTypes::Attack_Unit, 3, 4);
  FakeSink sink;
  CommandAdapter adapter;
  adapter.drain(*data, 7, true, target_for(sink));
  CHECK(sink.submit_calls == 1 && sink.sent.size() == 3);
  CHECK(sink.sent[0].actor == 1001 && sink.sent[1].actor == 1002 && sink.sent[2].actor == 1003);
  CHECK(sink.sent[2].command.target == 1004);
  CHECK(adapter.counts().submitted == 3);
  // Nothing to send on an empty frame.
  adapter.drain(*data, 8, true, target_for(sink));
  CHECK(sink.submit_calls == 1);
}

std::string capture_log(const std::function<void()>& run);

// The command optimizer level decides what groups: nothing at 0, non-position
// commands from 1, position commands from 3, and from 4 on nearby positions
// merge through 32 pixel alignment. A move the optimizer does not take runs
// as the client's own move command, and level 4 puts it back into that shape.
void optimizer_groups_by_level() {
  namespace T = BWAPI::UnitCommandTypes;
  namespace C = BWAPIC::CommandType;
  const auto moves = [](BWAPI::GameData& data) {
    add_unit_command(data, T::Move, 1, -1, 10, 20);
    add_unit_command(data, T::Move, 2, -1, 10, 20);
  };
  const auto stops = [](BWAPI::GameData& data) {
    add_unit_command(data, T::Stop, 3);
    add_unit_command(data, T::Stop, 4);
    add_unit_command(data, T::Stop, 5);
  };
  // Level 0: one command per unit command, and a move stays a move.
  {
    auto data = std::make_unique<BWAPI::GameData>();
    moves(*data);
    stops(*data);
    add_game_command(*data, C::SetCommandOptimizerLevel, 0);
    FakeSink sink;
    CommandAdapter adapter;
    adapter.drain(*data, 7, true, target_for(sink));
    CHECK(sink.sent.size() == 5);
    for (const FakeSink::Sent& entry : sink.sent) CHECK(entry.command.unit_count == 1);
    CHECK(sink.sent[0].command.kind == BWR_COMMAND_MOVE);
  }
  // Levels 1 and 2: position commands stay per unit in the client's own form
  // (a move is a move order) and go first; the stops then share one command,
  // newest unit first.
  for (const int level : {1, 2}) {
    auto data = std::make_unique<BWAPI::GameData>();
    add_game_command(*data, C::SetCommandOptimizerLevel, level);
    moves(*data);
    stops(*data);
    FakeSink sink;
    CommandAdapter adapter;
    adapter.drain(*data, 7, true, target_for(sink));
    CHECK(sink.sent.size() == 3);
    if (sink.sent.size() != 3) continue;
    CHECK(sink.sent[0].command.kind == BWR_COMMAND_MOVE);
    CHECK(sink.sent[0].units == std::vector<std::uint64_t>{1001});
    CHECK(sink.sent[1].command.kind == BWR_COMMAND_MOVE);
    CHECK(sink.sent[1].units == std::vector<std::uint64_t>{1002});
    CHECK(sink.sent[2].command.kind == BWR_COMMAND_STOP);
    CHECK(sink.sent[2].units == std::vector<std::uint64_t>({1005, 1004, 1003}));
  }
  // Level 3: equal position commands group as right clicks, a different
  // position does not; the newest command's group leads.
  {
    auto data = std::make_unique<BWAPI::GameData>();
    add_game_command(*data, C::SetCommandOptimizerLevel, 3);
    moves(*data);
    add_unit_command(*data, T::Move, 3, -1, 11, 21);
    FakeSink sink;
    CommandAdapter adapter;
    adapter.drain(*data, 7, true, target_for(sink));
    CHECK(sink.sent.size() == 2);
    if (sink.sent.size() == 2) {
      CHECK(sink.sent[0].command.kind == BWR_COMMAND_RIGHT_CLICK_POSITION);
      CHECK(sink.sent[0].units == std::vector<std::uint64_t>{1003});
      CHECK(sink.sent[0].command.x == 11 && sink.sent[0].command.y == 21);
      CHECK(sink.sent[1].command.kind == BWR_COMMAND_RIGHT_CLICK_POSITION);
      CHECK(sink.sent[1].units == std::vector<std::uint64_t>({1002, 1001}));
      CHECK(sink.sent[1].command.x == 10 && sink.sent[1].command.y == 20);
    }
  }
  // Level 4: positions align to the 32 pixel tile grid, so nearby commands
  // merge into one, and the move is its own command again.
  {
    auto data = std::make_unique<BWAPI::GameData>();
    add_game_command(*data, C::SetCommandOptimizerLevel, 4);
    add_unit_command(*data, T::Move, 1, -1, 35, 67);
    add_unit_command(*data, T::Move, 2, -1, 40, 72);
    FakeSink sink;
    CommandAdapter adapter;
    adapter.drain(*data, 7, true, target_for(sink));
    CHECK(sink.sent.size() == 1);
    if (!sink.sent.empty()) {
      CHECK(sink.sent[0].command.kind == BWR_COMMAND_MOVE);
      CHECK(sink.sent[0].command.x == 32 && sink.sent[0].command.y == 64);
      CHECK(sink.sent[0].units == std::vector<std::uint64_t>({1002, 1001}));
    }
  }
}

// A group holds only commands the official optimizer treats as one: same
// type, same extra argument, same target, same position, same split flag. A
// unit's identical repeat stays in the group, and the follow-up of a full
// group goes into a new command. Commands the optimizer leaves alone go first;
// groups follow by command type id, newest command first.
void optimizer_groups_only_equal_commands() {
  namespace T = BWAPI::UnitCommandTypes;
  auto data = std::make_unique<BWAPI::GameData>();
  add_game_command(*data, BWAPIC::CommandType::SetCommandOptimizerLevel, 4);
  add_unit_command(*data, T::Attack_Unit, 1, 6);
  add_unit_command(*data, T::Attack_Unit, 2, 6);
  add_unit_command(*data, T::Attack_Unit, 3, 7);
  add_unit_command(*data, T::Hold_Position, 4, -1, 0, 0, 1);
  add_unit_command(*data, T::Hold_Position, 5, -1, 0, 0, 1);
  add_unit_command(*data, T::Stop, 6);
  add_unit_command(*data, T::Stop, 6);
  add_unit_command(*data, T::Stop, 7);
  FakeSink sink;
  CommandAdapter adapter;
  adapter.drain(*data, 7, true, target_for(sink));
  CHECK(sink.sent.size() == 5);
  if (sink.sent.size() == 5) {
    // Queued commands never group, so they go first.
    CHECK(sink.sent[0].command.kind == BWR_COMMAND_HOLD_POSITION);
    CHECK(sink.sent[0].command.flags == BWR_COMMAND_FLAG_QUEUED && sink.sent[0].units == std::vector<std::uint64_t>{1004});
    CHECK(sink.sent[1].units.size() == 1 && sink.sent[1].units[0] == 1005);
    // The newest attack (on unit 7) leads its type.
    CHECK(sink.sent[2].command.kind == BWR_COMMAND_ATTACK_UNIT && sink.sent[2].command.target == 1007);
    CHECK(sink.sent[2].units == std::vector<std::uint64_t>{1003});
    CHECK(sink.sent[3].command.kind == BWR_COMMAND_ATTACK_UNIT && sink.sent[3].command.target == 1006);
    CHECK(sink.sent[3].units == std::vector<std::uint64_t>({1002, 1001}));
    // Unit 6's second stop stays in the group, as the official optimizer
    // keeps a repeat; the game selection lists the unit twice.
    CHECK(sink.sent[4].command.kind == BWR_COMMAND_STOP);
    CHECK(sink.sent[4].units == std::vector<std::uint64_t>({1007, 1006, 1006}));
  }
  CHECK(adapter.counts().unit_commands == 8);

  // A group ends at 12 units and continues in a new command, newest first.
  auto split = std::make_unique<BWAPI::GameData>();
  add_game_command(*split, BWAPIC::CommandType::SetCommandOptimizerLevel, 4);
  for (int unit = 1; unit <= 30; ++unit) add_unit_command(*split, T::Stop, unit);
  FakeSink split_sink;
  CommandAdapter split_adapter;
  split_adapter.drain(*split, 7, true, target_for(split_sink));
  CHECK(split_sink.sent.size() == 3);
  if (split_sink.sent.size() == 3) {
    CHECK(split_sink.sent[0].command.unit_count == 12 && split_sink.sent[0].units.front() == 1030 &&
          split_sink.sent[0].units.back() == 1019);
    CHECK(split_sink.sent[1].command.unit_count == 12 && split_sink.sent[1].units.front() == 1018 &&
          split_sink.sent[1].units.back() == 1007);
    CHECK(split_sink.sent[2].command.unit_count == 6 && split_sink.sent[2].units.front() == 1006 &&
          split_sink.sent[2].units.back() == 1001);
  }
}

// What the official optimizer groups at level 4: Train, Lift, Unload, the
// merges and the toggle techs join one command each; Research stays one by
// one.
void optimizer_groups_what_official_groups() {
  namespace T = BWAPI::UnitCommandTypes;
  namespace Tech = BWAPI::TechTypes::Enum;
  auto data = std::make_unique<BWAPI::GameData>();
  add_game_command(*data, BWAPIC::CommandType::SetCommandOptimizerLevel, 4);
  add_unit_command(*data, T::Train, 1, -1, 0, 0, 7);
  add_unit_command(*data, T::Train, 2, -1, 0, 0, 7);
  add_unit_command(*data, T::Research, 1, -1, 0, 0, 2);
  add_unit_command(*data, T::Research, 2, -1, 0, 0, 2);
  add_unit_command(*data, T::Unload, 1, 6);
  add_unit_command(*data, T::Unload, 2, 6);
  add_unit_command(*data, T::Lift, 1);
  add_unit_command(*data, T::Lift, 2);
  add_unit_command(*data, T::Use_Tech_Unit, 1, 6, 0, 0, Tech::Archon_Warp);
  add_unit_command(*data, T::Use_Tech_Unit, 2, 6, 0, 0, Tech::Archon_Warp);
  add_unit_command(*data, T::Use_Tech, 1, -1, 0, 0, Tech::Tank_Siege_Mode);
  add_unit_command(*data, T::Use_Tech, 2, -1, 0, 0, Tech::Tank_Siege_Mode);
  FakeSink sink;
  CommandAdapter adapter;
  adapter.drain(*data, 7, true, target_for(sink));
  CHECK(sink.sent.size() == 7);
  if (sink.sent.size() != 7) return;
  const auto& sent = sink.sent;
  // The research commands are not taken, so they go first in client order;
  // the groups follow by command type id (Train, Siege, Lift, Unload,
  // Use_Tech_Unit), newest unit first.
  CHECK(sent[0].command.kind == BWR_COMMAND_RESEARCH && sent[0].units == std::vector<std::uint64_t>{1001});
  CHECK(sent[1].command.kind == BWR_COMMAND_RESEARCH && sent[1].units == std::vector<std::uint64_t>{1002});
  CHECK(sent[2].command.kind == BWR_COMMAND_TRAIN && sent[2].units == std::vector<std::uint64_t>({1002, 1001}));
  CHECK(sent[3].command.kind == BWR_COMMAND_SIEGE && sent[3].units == std::vector<std::uint64_t>({1002, 1001}));
  CHECK(sent[4].command.kind == BWR_COMMAND_LIFT && sent[4].units == std::vector<std::uint64_t>({1002, 1001}));
  CHECK(sent[5].command.kind == BWR_COMMAND_UNLOAD && sent[5].command.target == 1006 &&
        sent[5].units == std::vector<std::uint64_t>({1002, 1001}));
  // The merges drop their partner at level 4, so both templar join one
  // targetless merge.
  CHECK(sent[6].command.kind == BWR_COMMAND_USE_TECH_UNIT && sent[6].command.target == 0 &&
        sent[6].command.type_id == static_cast<std::uint32_t>(Tech::Archon_Warp) &&
        sent[6].units == std::vector<std::uint64_t>({1002, 1001}));
}

// The level lasts across frames, clamps to 4, and a new match resets it.
void optimizer_level_persists_and_resets() {
  namespace T = BWAPI::UnitCommandTypes;
  auto data = std::make_unique<BWAPI::GameData>();
  add_game_command(*data, BWAPIC::CommandType::SetCommandOptimizerLevel, 9);
  add_unit_command(*data, T::Move, 1, -1, 33, 33);
  add_unit_command(*data, T::Move, 2, -1, 33, 33);
  FakeSink sink;
  CommandAdapter adapter;
  adapter.drain(*data, 7, true, target_for(sink));
  CHECK(sink.sent.size() == 1);
  if (!sink.sent.empty())
    CHECK(sink.sent[0].command.kind == BWR_COMMAND_MOVE && sink.sent[0].command.x == 32 &&
          sink.sent[0].command.y == 32);
  // The level holds on later frames without a new setting.
  add_unit_command(*data, T::Stop, 1);
  add_unit_command(*data, T::Stop, 2);
  adapter.drain(*data, 8, true, target_for(sink));
  CHECK(sink.sent.size() == 2 && sink.sent[1].command.unit_count == 2);
  // A new match starts at level 0.
  adapter.start_match();
  add_unit_command(*data, T::Stop, 1);
  add_unit_command(*data, T::Stop, 2);
  adapter.drain(*data, 9, true, target_for(sink));
  CHECK(sink.sent.size() == 4);
  CHECK(sink.sent[2].command.unit_count == 1 && sink.sent[3].command.unit_count == 1);
}

// The official rewrites: an attack on an enemy and gathering a resource
// container become right clicks, a rally command of a producing building
// too (but not of a carrier or reaver), infestation is the queen's right
// click on the command center, a carrier or reaver trains one placeholder
// type whatever it builds, and the toggle techs become the unit-state
// command.
void optimizer_rewrites_commands_like_official() {
  namespace T = BWAPI::UnitCommandTypes;
  namespace Tech = BWAPI::TechTypes::Enum;
  namespace U = BWAPI::UnitTypes::Enum;
  auto data = std::make_unique<BWAPI::GameData>();
  add_game_command(*data, BWAPIC::CommandType::SetCommandOptimizerLevel, 4);
  // Unit 5 is a marine of self (player 0); unit 6 an enemy (player 1) ghost
  // and unit 7 a mineral field and unit 8 a command center of the enemy.
  data->self = 0;
  data->players[0].isEnemy[1] = true;
  data->units[5].type = U::Terran_Marine;
  data->units[6].type = U::Terran_Ghost;
  data->units[6].player = 1;
  data->units[7].type = U::Resource_Mineral_Field;
  data->units[8].type = U::Terran_Command_Center;
  data->units[8].player = 1;
  // A queen of our own infests the enemy command center.
  data->units[9].type = U::Zerg_Queen;
  add_unit_command(*data, T::Attack_Unit, 5, 6);
  add_unit_command(*data, T::Attack_Unit, 5, 7);  // not an enemy: stays an attack
  add_unit_command(*data, T::Gather, 5, 7);
  add_unit_command(*data, T::Set_Rally_Position, 1, -1, 10, 20);  // marine: no produce
  add_unit_command(*data, T::Use_Tech_Unit, 9, 8, 0, 0, Tech::Infestation);
  add_unit_command(*data, T::Train, 5, -1, 0, 0, 7);  // marine: its own train type
  FakeSink sink;
  CommandAdapter adapter;
  adapter.drain(*data, 7, true, target_for(sink));
  CHECK(sink.sent.size() == 6);
  if (sink.sent.size() != 6) return;
  const auto& sent = sink.sent;
  // By final command type id (Attack_Unit, Train, Set_Rally_Position,
  // Right_Click_Unit); the right clicks newest first.
  CHECK(sent[0].command.kind == BWR_COMMAND_ATTACK_UNIT && sent[0].command.target == 1007);
  CHECK(sent[1].command.kind == BWR_COMMAND_TRAIN && sent[1].command.type_id == 7);
  CHECK(sent[2].command.kind == BWR_COMMAND_SET_RALLY_POSITION && sent[2].command.x == 0 &&
        sent[2].command.y == 0);
  CHECK(sent[3].command.kind == BWR_COMMAND_SMART_TARGET_UNIT && sent[3].command.target == 1008);
  CHECK(sent[4].command.kind == BWR_COMMAND_SMART_TARGET_UNIT && sent[4].command.target == 1007);
  CHECK(sent[5].command.kind == BWR_COMMAND_SMART_TARGET_UNIT && sent[5].command.target == 1006 &&
        sent[5].units == std::vector<std::uint64_t>{1005});

  // A factory's rally commands turn into right clicks of the same place and
  // target; a carrier's do not, and its train becomes the placeholder.
  data = std::make_unique<BWAPI::GameData>();
  add_game_command(*data, BWAPIC::CommandType::SetCommandOptimizerLevel, 3);
  data->units[1].type = U::Terran_Factory;
  data->units[2].type = U::Protoss_Carrier;
  data->units[3].type = U::Protoss_Reaver;
  add_unit_command(*data, T::Set_Rally_Position, 1, -1, 40, 60);
  add_unit_command(*data, T::Set_Rally_Unit, 1, 4);
  add_unit_command(*data, T::Set_Rally_Position, 2, -1, 40, 60);
  add_unit_command(*data, T::Train, 2, -1, 0, 0, U::Protoss_Scarab);
  add_unit_command(*data, T::Train, 3, -1, 0, 0, U::Protoss_Interceptor);
  FakeSink carrier_sink;
  CommandAdapter carrier_adapter;
  carrier_adapter.drain(*data, 7, true, target_for(carrier_sink));
  CHECK(carrier_sink.sent.size() == 4);
  if (carrier_sink.sent.size() != 4) return;
  const auto& carrier = carrier_sink.sent;
  // Both carrier and reaver trains share the interceptor placeholder group.
  CHECK(carrier[0].command.kind == BWR_COMMAND_TRAIN &&
        carrier[0].command.type_id == static_cast<std::uint32_t>(U::Protoss_Interceptor) &&
        carrier[0].units == std::vector<std::uint64_t>({1003, 1002}));
  CHECK(carrier[1].command.kind == BWR_COMMAND_SET_RALLY_POSITION);
  CHECK(carrier[2].command.kind == BWR_COMMAND_RIGHT_CLICK_POSITION && carrier[2].command.x == 40 &&
        carrier[2].command.y == 60);
  CHECK(carrier[3].command.kind == BWR_COMMAND_SMART_TARGET_UNIT && carrier[3].command.target == 1004);

  // The toggle techs follow the live unit state at the level the optimizer
  // takes them.
  data = std::make_unique<BWAPI::GameData>();
  add_game_command(*data, BWAPIC::CommandType::SetCommandOptimizerLevel, 4);
  data->units[1].type = U::Terran_Siege_Tank_Siege_Mode;
  data->units[2].type = U::Terran_Siege_Tank_Tank_Mode;
  data->units[3].type = U::Terran_Wraith;
  data->units[3].isCloaked = true;
  data->units[4].type = U::Terran_Wraith;
  data->units[5].type = U::Zerg_Zergling;
  data->units[5].isBurrowed = true;
  add_unit_command(*data, T::Use_Tech, 1, -1, 0, 0, Tech::Tank_Siege_Mode);
  add_unit_command(*data, T::Use_Tech, 2, -1, 0, 0, Tech::Tank_Siege_Mode);
  add_unit_command(*data, T::Use_Tech, 3, -1, 0, 0, Tech::Personnel_Cloaking);
  add_unit_command(*data, T::Use_Tech, 4, -1, 0, 0, Tech::Personnel_Cloaking);
  add_unit_command(*data, T::Use_Tech, 5, -1, 0, 0, Tech::Burrowing);
  FakeSink toggle_sink;
  CommandAdapter toggle_adapter;
  toggle_adapter.drain(*data, 7, true, target_for(toggle_sink));
  CHECK(toggle_sink.sent.size() == 5);
  if (toggle_sink.sent.size() != 5) return;
  // Sent by command type id: Unburrow, Cloak, Decloak, Siege, Unsiege.
  CHECK(toggle_sink.sent[0].command.kind == BWR_COMMAND_UNBURROW && toggle_sink.sent[0].units == std::vector<std::uint64_t>{1005});
  CHECK(toggle_sink.sent[1].command.kind == BWR_COMMAND_CLOAK && toggle_sink.sent[1].units == std::vector<std::uint64_t>{1004});
  CHECK(toggle_sink.sent[2].command.kind == BWR_COMMAND_DECLOAK && toggle_sink.sent[2].units == std::vector<std::uint64_t>{1003});
  CHECK(toggle_sink.sent[3].command.kind == BWR_COMMAND_SIEGE && toggle_sink.sent[3].units == std::vector<std::uint64_t>{1002});
  CHECK(toggle_sink.sent[4].command.kind == BWR_COMMAND_UNSIEGE && toggle_sink.sent[4].units == std::vector<std::uint64_t>{1001});
}

// The official flush splits groups by a per-unit flag where the executed
// order depends on the unit: a building or not for Attack_Unit (and Load,
// Unload_All, Cancel_Morph), and the sieged, cloaked or burrowed state for
// the remaining Use_Tech commands. Below level 2 buildings never group at
// all.
void optimizer_splits_groups_by_unit_state() {
  namespace T = BWAPI::UnitCommandTypes;
  namespace Tech = BWAPI::TechTypes::Enum;
  namespace U = BWAPI::UnitTypes::Enum;
  auto data = std::make_unique<BWAPI::GameData>();
  add_game_command(*data, BWAPIC::CommandType::SetCommandOptimizerLevel, 4);
  // Nobody is an enemy of the neutral target, so the attacks stay attacks.
  data->units[1].type = U::Terran_Missile_Turret;
  data->units[2].type = U::Terran_Marine;
  data->units[3].type = U::Terran_Marine;
  data->units[3].isCloaked = true;
  data->units[6].type = U::Resource_Mineral_Field;
  data->units[6].player = 11;  // neutral
  add_unit_command(*data, T::Attack_Unit, 1, 6);
  add_unit_command(*data, T::Attack_Unit, 2, 6);
  add_unit_command(*data, T::Use_Tech, 2, -1, 0, 0, Tech::Stim_Packs);
  add_unit_command(*data, T::Use_Tech, 3, -1, 0, 0, Tech::Stim_Packs);
  FakeSink sink;
  CommandAdapter adapter;
  adapter.drain(*data, 7, true, target_for(sink));
  CHECK(sink.sent.size() == 4);
  if (sink.sent.size() == 4) {
    // The turret and the marine split; so do the plain and cloaked marines'
    // stim commands.
    CHECK(sink.sent[0].command.kind == BWR_COMMAND_ATTACK_UNIT && sink.sent[0].units.size() == 1);
    CHECK(sink.sent[1].command.kind == BWR_COMMAND_ATTACK_UNIT && sink.sent[1].units.size() == 1);
    CHECK(sink.sent[2].command.kind == BWR_COMMAND_USE_TECH && sink.sent[2].units.size() == 1);
    CHECK(sink.sent[3].command.kind == BWR_COMMAND_USE_TECH && sink.sent[3].units.size() == 1);
  }

  // At level 1 a building's commands never group; at level 2 they do.
  const auto trains = [](BWAPI::GameData& data) {
    add_unit_command(data, T::Train, 1, -1, 0, 0, 0);
    add_unit_command(data, T::Train, 2, -1, 0, 0, 0);
  };
  for (const int level : {1, 2}) {
    auto barracks = std::make_unique<BWAPI::GameData>();
    add_game_command(*barracks, BWAPIC::CommandType::SetCommandOptimizerLevel, level);
    barracks->units[1].type = U::Terran_Barracks;
    barracks->units[2].type = U::Terran_Barracks;
    trains(*barracks);
    FakeSink barracks_sink;
    CommandAdapter barracks_adapter;
    barracks_adapter.drain(*barracks, 7, true, target_for(barracks_sink));
    if (level == 1) {
      CHECK(barracks_sink.sent.size() == 2);
      for (const FakeSink::Sent& entry : barracks_sink.sent) CHECK(entry.command.unit_count == 1);
    } else {
      CHECK(barracks_sink.sent.size() == 1 &&
            barracks_sink.sent[0].units == std::vector<std::uint64_t>({1002, 1001}));
    }
  }
}

// Level 4 turns a unit-targeted tech with a position shape into the position
// command at the target's live place, without the 32 pixel alignment the
// plain position commands take.
void optimizer_level4_rewrites_unit_techs_to_positions() {
  namespace T = BWAPI::UnitCommandTypes;
  namespace Tech = BWAPI::TechTypes::Enum;
  auto data = std::make_unique<BWAPI::GameData>();
  add_game_command(*data, BWAPIC::CommandType::SetCommandOptimizerLevel, 4);
  data->units[5].type = BWAPI::UnitTypes::Enum::Terran_Science_Vessel;
  data->units[6].positionX = 101;
  data->units[6].positionY = 205;
  add_unit_command(*data, T::Use_Tech_Unit, 5, 6, 0, 0, Tech::Scanner_Sweep);
  add_unit_command(*data, T::Use_Tech_Position, 4, -1, 96, 192, Tech::Scanner_Sweep);
  FakeSink sink;
  CommandAdapter adapter;
  adapter.drain(*data, 7, true, target_for(sink));
  // The rewritten sweep keeps the target's exact place, so it does not join
  // the aligned one at (96, 192), which as the newest command goes first.
  CHECK(sink.sent.size() == 2);
  if (sink.sent.size() == 2) {
    CHECK(sink.sent[0].command.x == 96 && sink.sent[0].command.y == 192 &&
          sink.sent[0].units == std::vector<std::uint64_t>{1004});
    CHECK(sink.sent[1].command.kind == BWR_COMMAND_USE_TECH_POSITION &&
          sink.sent[1].command.target == 0 && sink.sent[1].command.x == 101 &&
          sink.sent[1].command.y == 205 && sink.sent[1].units == std::vector<std::uint64_t>{1005});
  }
}

// APM counts what the official server would issue for the submitted
// commands: a selection and a command per lone command (both units in one
// selection for an archon merge), and one selection and one command per
// twelve grouped commands of a key, repeats included. A batch that is not
// submitted counts nothing.
void apm_actions_follow_submitted_groups() {
  namespace T = BWAPI::UnitCommandTypes;
  namespace Tech = BWAPI::TechTypes::Enum;
  auto data = std::make_unique<BWAPI::GameData>();
  add_unit_command(*data, T::Move, 1, -1, 10, 20);
  add_unit_command(*data, T::Unload, 1, 6);
  add_unit_command(*data, T::Use_Tech_Unit, 1, 6, 0, 0, Tech::Archon_Warp);
  FakeSink sink;
  CommandAdapter adapter;
  adapter.drain(*data, 7, true, target_for(sink));
  CHECK(adapter.last_actions().selections == 3 && adapter.last_actions().commands == 3);

  add_game_command(*data, BWAPIC::CommandType::SetCommandOptimizerLevel, 2);
  add_unit_command(*data, T::Unload, 1, 6);
  add_unit_command(*data, T::Unload, 2, 6);
  // The two unloads share one group of twelve; 30 stops are three groups of
  // 12; 12 holds plus a repeat are two.
  for (int unit = 1; unit <= 30; ++unit) add_unit_command(*data, T::Stop, unit);
  for (int unit = 1; unit <= 12; ++unit) add_unit_command(*data, T::Hold_Position, unit);
  add_unit_command(*data, T::Hold_Position, 1);
  adapter.drain(*data, 8, true, target_for(sink));
  CHECK(adapter.last_actions().selections == 6 && adapter.last_actions().commands == 6);

  // Nothing was sent: no actions, and the last frame's do not carry over.
  add_unit_command(*data, T::Stop, 1);
  adapter.drain(*data, 9, true);
  CHECK(adapter.last_actions().selections == 0 && adapter.last_actions().commands == 0);
  sink.submit_status = BWR_NO_COMMANDS;
  add_unit_command(*data, T::Stop, 1);
  capture_log([&] { adapter.drain(*data, 10, true, target_for(sink)); });
  CHECK(adapter.last_actions().selections == 0 && adapter.last_actions().commands == 0);
}

// A grouped command is one submission with one result id to poll.
void grouped_results_are_polled() {
  auto data = std::make_unique<BWAPI::GameData>();
  add_game_command(*data, BWAPIC::CommandType::SetCommandOptimizerLevel, 3);
  add_unit_command(*data, BWAPI::UnitCommandTypes::Move, 1, -1, 5, 5);
  add_unit_command(*data, BWAPI::UnitCommandTypes::Move, 2, -1, 5, 5);
  FakeSink sink;
  sink.next_state = BWR_RESULT_PENDING;
  CommandAdapter adapter;
  std::string log = capture_log([&] { adapter.drain(*data, 7, true, target_for(sink)); });
  CHECK(log.empty());
  CHECK(adapter.counts().submitted == 1);
  sink.next_state = BWR_RESULT_REJECTED;
  log = capture_log([&] { adapter.drain(*data, 8, true, target_for(sink)); });
  CHECK(log.find("status=REJECTED id=100 frame=8") != std::string::npos);
}

std::string capture_log(const std::function<void()>& run) {
  std::ostringstream log;
  auto* previous = std::cerr.rdbuf(log.rdbuf());
  run();
  std::cerr.rdbuf(previous);
  return log.str();
}

// A command whose unit or target cannot be mapped is logged with the reason
// and the rest of the frame is still sent.
void unmappable_commands_are_logged() {
  auto data = std::make_unique<BWAPI::GameData>();
  add_unit_command(*data, BWAPI::UnitCommandTypes::Move, 99, -1, 1, 2);
  add_unit_command(*data, BWAPI::UnitCommandTypes::Attack_Unit, 1, 99);
  add_unit_command(*data, BWAPI::UnitCommandTypes::Stop, 1);
  add_unit_command(*data, BWAPI::UnitCommandTypes::None, 1);
  FakeSink sink;
  CommandAdapter adapter;
  const std::string log = capture_log([&] { adapter.drain(*data, 7, true, target_for(sink)); });
  CHECK(sink.sent.size() == 1 && sink.sent[0].command.kind == BWR_COMMAND_STOP);
  CHECK(log.find("status=not_executed reason=unit 99 is not a live unit") != std::string::npos);
  CHECK(log.find("status=not_executed reason=target unit 99 is not a live unit") != std::string::npos);
  CHECK(log.find("has no game command") != std::string::npos);
}

// Results are read on later frames and logged with their state.
void results_are_polled_on_later_frames() {
  auto data = std::make_unique<BWAPI::GameData>();
  add_unit_command(*data, BWAPI::UnitCommandTypes::Move, 1, -1, 1, 2);
  FakeSink sink;
  sink.next_state = BWR_RESULT_PENDING;
  CommandAdapter adapter;
  std::string log = capture_log([&] { adapter.drain(*data, 7, true, target_for(sink)); });
  CHECK(log.empty());
  log = capture_log([&] { adapter.drain(*data, 8, true, target_for(sink)); });
  CHECK(log.empty());
  sink.next_state = BWR_RESULT_REJECTED;
  log = capture_log([&] { adapter.drain(*data, 9, true, target_for(sink)); });
  CHECK(log.find("status=REJECTED id=100 frame=9") != std::string::npos);
  log = capture_log([&] { adapter.drain(*data, 10, true, target_for(sink)); });
  CHECK(log.empty());
}

// Game commands with a game effect go ahead of unit commands with the
// arguments Server::processCommands hands to GameImpl.
void game_commands_map_to_bwr_commands() {
  namespace C = BWAPIC::CommandType;
  auto data = std::make_unique<BWAPI::GameData>();
  const std::string path = "maps\\(2)Destination.scx";
  const std::string long_path(300, 'm');
  std::memcpy(data->strings[0], path.c_str(), path.size() + 1);
  std::memcpy(data->strings[1], long_path.c_str(), long_path.size() + 1);
  data->stringCount = 2;
  data->commands[data->commandCount++] = BWAPIC::Command(C::SetAllies, 3, 2);
  data->commands[data->commandCount++] = BWAPIC::Command(C::SetAllies, 4, 7);
  data->commands[data->commandCount++] = BWAPIC::Command(C::SetVision, 2, 5);
  data->commands[data->commandCount++] = BWAPIC::Command(C::SetVision, 9, 1);
  data->commands[data->commandCount++] = BWAPIC::Command(C::PingMinimap, 320, 640);
  data->commands[data->commandCount++] = BWAPIC::Command(C::SetRevealAll, 9, 0);
  data->commands[data->commandCount++] = BWAPIC::Command(C::SetMap, 0, 0);
  data->commands[data->commandCount++] = BWAPIC::Command(C::SetMap, 1, 0);
  add_unit_command(*data, BWAPI::UnitCommandTypes::Stop, 1);
  FakeSink sink;
  CommandAdapter adapter;
  adapter.drain(*data, 7, true, target_for(sink));
  CHECK(sink.sent.size() == 7);
  if (sink.sent.size() != 7) return;
  const auto& sent = sink.sent;
  // Player IDs reach the game as slots.
  CHECK(sent[0].command.kind == BWR_COMMAND_SET_ALLIES && sent[0].command.x == 4 && sent[0].command.y == 2);
  CHECK(sent[1].command.kind == BWR_COMMAND_SET_ALLIES && sent[1].command.x == 3 && sent[1].command.y == 1);
  CHECK(sent[2].command.kind == BWR_COMMAND_SET_VISION && sent[2].command.x == 5 && sent[2].command.y == 1);
  CHECK(sent[3].command.kind == BWR_COMMAND_PING_MINIMAP && sent[3].command.x == 320 && sent[3].command.y == 640);
  CHECK(sent[4].command.kind == BWR_COMMAND_SET_REVEAL_ALL && sent[4].command.x == 1);
  CHECK(sent[5].command.kind == BWR_COMMAND_SET_MAP && sent[5].command.unit_count == 0);
  CHECK(std::string(sent[5].command.text) == path);
  // The name too long to carry is not cut into another name, and an ID
  // without a player does nothing.
  CHECK(sent[6].command.kind == BWR_COMMAND_STOP);
  CHECK(adapter.counts().game_commands == 2);
}

// setGUI(false) turns the game's screen updates off and, as GameImpl does,
// drops the printf and setScreenPosition that follow until it is on again.
void gui_off_stops_drawing_and_screen_commands() {
  namespace C = BWAPIC::CommandType;
  auto data = std::make_unique<BWAPI::GameData>();
  data->hasGUI = true;
  std::memcpy(data->strings[0], "hello", 6);
  data->stringCount = 1;
  data->commands[data->commandCount++] = BWAPIC::Command(C::SetGui, 0, 0);
  data->commands[data->commandCount++] = BWAPIC::Command(C::Printf, 0, 0);
  data->commands[data->commandCount++] = BWAPIC::Command(C::SetScreenPosition, 64, 64);
  data->commands[data->commandCount++] = BWAPIC::Command(C::SetGui, 1, 0);
  data->commands[data->commandCount++] = BWAPIC::Command(C::Printf, 0, 0);
  FakeSink sink;
  CommandAdapter adapter;
  adapter.drain(*data, 7, true, target_for(sink));
  CHECK(sink.sent.size() == 3);
  if (sink.sent.size() != 3) return;
  CHECK(sink.sent[0].command.kind == BWR_COMMAND_SET_GUI && sink.sent[0].command.x == 0);
  CHECK(sink.sent[1].command.kind == BWR_COMMAND_SET_GUI && sink.sent[1].command.x == 1);
  CHECK(sink.sent[2].command.kind == BWR_COMMAND_PRINT_TEXT);
  CHECK(data->hasGUI && adapter.counts().game_commands == 0);
}

// A failed bwr_submit leaves the batch not executed and nothing to poll.
void failed_submit_is_logged() {
  auto data = std::make_unique<BWAPI::GameData>();
  add_unit_command(*data, BWAPI::UnitCommandTypes::Stop, 1);
  FakeSink sink;
  sink.submit_status = BWR_NO_COMMANDS;
  CommandAdapter adapter;
  const std::string log = capture_log([&] { adapter.drain(*data, 7, true, target_for(sink)); });
  CHECK(log.find("bwr_submit failed with status 6") != std::string::npos);
  CHECK(adapter.counts().submitted == 0);
}

// Latency compensation takes the accepted commands as the client wrote them:
// a grouped move stays a move, a repeat stays, an unknown unit and a failed
// submission leave nothing.
void submitted_commands_keep_client_form() {
  namespace T = BWAPI::UnitCommandTypes;
  auto data = std::make_unique<BWAPI::GameData>();
  add_game_command(*data, BWAPIC::CommandType::SetCommandOptimizerLevel, 4);
  add_unit_command(*data, T::Move, 1, -1, 37, 45);
  add_unit_command(*data, T::Move, 1, -1, 37, 45);
  add_unit_command(*data, T::Stop, 99);
  FakeSink sink;
  CommandAdapter adapter;
  capture_log([&] { adapter.drain(*data, 7, true, target_for(sink)); });
  const auto& submitted = adapter.submitted_unit_commands();
  CHECK(submitted.size() == 2);
  for (const auto& command : submitted)
    CHECK(command.type == T::Move && command.unitIndex == 1 && command.x == 37 && command.y == 45);

  add_unit_command(*data, T::Stop, 1);
  sink.submit_status = BWR_NO_COMMANDS;
  capture_log([&] { adapter.drain(*data, 8, true, target_for(sink)); });
  CHECK(adapter.submitted_unit_commands().empty());
}

// GameImpl::parseText: an official chat line becomes the game commands it
// stands for, with the official confirmation line; anything else is not a
// chat command and stays the client's SendText.
void chat_commands_parse_official_lines() {
  namespace C = BWAPIC::CommandType;
  auto data = std::make_unique<BWAPI::GameData>();
  data->hasGUI = true;
  ChatCommands chat;
  const auto line_of = [&](const BWAPIC::Command& command) {
    return std::string(data->strings[command.value1]);
  };
  CHECK(chat.parse("/leave", *data));
  CHECK(data->commandCount == 1 && data->commands[0].type == C::LeaveGame);
  CHECK(chat.parse("/speed 42", *data));
  CHECK(data->commands[1].type == C::SetLocalSpeed && data->commands[1].value1 == 42);
  CHECK(data->commands[2].type == C::Printf && line_of(data->commands[2]) == "Changed game speed");
  CHECK(chat.parse("/speed", *data));
  CHECK(data->commands[3].type == C::SetLocalSpeed && data->commands[3].value1 == -1);
  CHECK(data->commands[4].type == C::Printf && line_of(data->commands[4]) == "Changed game speed");
  CHECK(chat.parse("/fs 3", *data));
  CHECK(data->commands[5].type == C::SetFrameSkip && data->commands[5].value1 == 3);
  CHECK(data->commands[6].type == C::Printf && line_of(data->commands[6]) == "Altered frame skip");
  CHECK(chat.parse("/fs", *data));
  CHECK(data->commands[7].type == C::SetFrameSkip && data->commands[7].value1 == 1);
  CHECK(data->commands[8].type == C::Printf && line_of(data->commands[8]) == "Altered frame skip");
  CHECK(chat.parse("/restart", *data));
  CHECK(data->commands[9].type == C::RestartGame);
  CHECK(chat.parse("/nogui", *data));
  CHECK(data->commands[10].type == C::SetGui && data->commands[10].value1 == 0);
  CHECK(data->commands[11].type == C::Printf && line_of(data->commands[11]) == "GUI: disabled");
  CHECK(chat.parse("/grid", *data) && chat.grid);
  CHECK(data->commands[12].type == C::Printf &&
        line_of(data->commands[12]) == "Matrix grid enabled");
  CHECK(chat.parse("/grid", *data) && !chat.grid);
  CHECK(data->commands[13].type == C::Printf &&
        line_of(data->commands[13]) == "Matrix grid disabled");
  CHECK(chat.parse("/fps", *data) && chat.showfps);
  CHECK(data->commands[14].type == C::Printf &&
        line_of(data->commands[14]) == "FPS display enabled");
  const int before_cheats = data->commandCount;
  CHECK(chat.parse("/cheats", *data));
  CHECK(data->commandCount - before_cheats == 15);
  for (int i = before_cheats; i < data->commandCount; ++i)
    CHECK(data->commands[i].type == C::SendText);
  CHECK(line_of(data->commands[before_cheats]) == "power overwhelming");
  CHECK(line_of(data->commands[before_cheats + 7]) == "something for nothing");
  CHECK(line_of(data->commands[before_cheats + 10]) == "show me the money");
  for (const char* not_a_command : {"", "hello", "/x", "/wmode", "/speedX"})
    CHECK(!chat.parse(not_a_command, *data));
}

// A recognized line runs through the same drain as the client's commands, so
// it reaches the game without ever becoming a SendText event.
void recognized_chat_command_reaches_the_game() {
  auto data = std::make_unique<BWAPI::GameData>();
  data->hasGUI = true;
  ChatCommands chat;
  CHECK(chat.parse("/speed 42", *data));
  FakeSink sink;
  CommandAdapter adapter;
  adapter.drain(*data, 7, true, target_for(sink));
  CHECK(sink.sent.size() == 2);
  if (sink.sent.size() == 2) {
    CHECK(sink.sent[0].command.kind == BWR_COMMAND_SET_SPEED && sink.sent[0].command.x == 42);
    CHECK(sink.sent[1].command.kind == BWR_COMMAND_PRINT_TEXT);
    CHECK(std::string(sink.sent[1].command.text) == "Changed game speed");
  }
  CHECK(data->commandCount == 0);
}

// GameImpl::vSendTextEx: outside a network match a known cheat text becomes
// the game's own cheat command with the running cheat set; the one-shot
// cheats leave the set again, and everything else is sent as chat.
void single_player_cheat_text_becomes_game_cheat() {
  const auto send = [](BWAPI::GameData& data, const char* text) {
    std::memcpy(data.strings[data.stringCount], text, std::strlen(text) + 1);
    data.commands[data.commandCount++] =
        BWAPIC::Command(BWAPIC::CommandType::SendText, data.stringCount++, 0);
  };
  auto data = std::make_unique<BWAPI::GameData>();
  send(*data, "Power Overwhelming");  // case-insensitive, as BW::getCheatFlag is
  send(*data, "power overwhelming");  // a toggle: the running set empties again
  send(*data, "show me the money");   // one-shot: it carries the set every time
  send(*data, "show me the money");
  send(*data, "hello");
  FakeSink sink;
  CommandAdapter adapter;
  adapter.drain(*data, 7, true, target_for(sink));
  CHECK(sink.sent.size() == 5);
  if (sink.sent.size() == 5) {
    CHECK(sink.sent[0].command.kind == BWR_COMMAND_USE_CHEAT && sink.sent[0].command.x == 0x4);
    CHECK(sink.sent[1].command.kind == BWR_COMMAND_USE_CHEAT && sink.sent[1].command.x == 0);
    CHECK(sink.sent[2].command.kind == BWR_COMMAND_USE_CHEAT && sink.sent[2].command.x == 0x10);
    CHECK(sink.sent[3].command.kind == BWR_COMMAND_USE_CHEAT && sink.sent[3].command.x == 0x10);
    CHECK(sink.sent[4].command.kind == BWR_COMMAND_SEND_TEXT);
    CHECK(std::string(sink.sent[4].command.text) == "hello");
  }
  // A network match sends the line as chat, whatever it says.
  data = std::make_unique<BWAPI::GameData>();
  data->isMultiplayer = true;
  send(*data, "power overwhelming");
  adapter.drain(*data, 8, true, target_for(sink));
  CHECK(sink.sent.size() == 6 && sink.sent[5].command.kind == BWR_COMMAND_SEND_TEXT);
  // A new match starts with an empty cheat set.
  data = std::make_unique<BWAPI::GameData>();
  send(*data, "the gathering");
  adapter.drain(*data, 9, true, target_for(sink));
  CHECK(sink.sent.back().command.kind == BWR_COMMAND_USE_CHEAT && sink.sent.back().command.x == 0x800);
  adapter.start_match();
  data = std::make_unique<BWAPI::GameData>();
  send(*data, "the gathering");
  adapter.drain(*data, 10, true, target_for(sink));
  CHECK(sink.sent.back().command.kind == BWR_COMMAND_USE_CHEAT && sink.sent.back().command.x == 0x800);
}

// The /grid and /fps overlays draw the official shapes beside the client's,
// and nothing while the GUI is off.
void grid_and_fps_shapes_follow_official_form() {
  auto data = std::make_unique<BWAPI::GameData>();
  ChatCommands chat;
  CHECK(chat.parse("/grid", *data) && chat.grid);
  CHECK(chat.parse("/fps", *data) && chat.showfps);
  data->hasGUI = false;
  chat.append_overlay_shapes(*data);
  CHECK(data->shapeCount == 0);
  data->hasGUI = true;
  data->screenX = 64;
  data->screenY = 32;
  data->mouseX = 20;
  data->mouseY = 36;
  data->fps = 24;
  data->averageFPS = 23.75;
  chat.append_overlay_shapes(*data);
  CHECK(data->shapeCount > 2);
  // Every line is a grey map tick; the first sits on the first tile's edge.
  if (data->shapeCount > 0) {
    const BWAPIC::Shape& first = data->shapes[0];
    CHECK(first.type == BWAPIC::ShapeType::Line && first.ctype == BWAPI::CoordinateType::Map);
    CHECK(first.x1 == 96 && first.y1 == 32 && first.x2 == 96 && first.y2 == 34);
  }
  int texts = 0;
  const BWAPIC::Shape* tile = nullptr;
  const BWAPIC::Shape* fps = nullptr;
  for (int i = 0; i < data->shapeCount; ++i) {
    const BWAPIC::Shape& shape = data->shapes[i];
    if (shape.type != BWAPIC::ShapeType::Text) {
      CHECK(shape.type == BWAPIC::ShapeType::Line &&
            shape.ctype == BWAPI::CoordinateType::Map &&
            shape.color == BWAPI::Colors::Grey.getID());
      continue;
    }
    if (shape.x1 == 64 && shape.y1 == 288) tile = &shape;
    if (shape.x1 == 1 && shape.y1 == 1) fps = &shape;
    ++texts;
  }
  CHECK(texts == 2 && tile != nullptr && fps != nullptr);
  if (tile != nullptr) {
    CHECK(std::string(data->strings[tile->extra1]) == std::string("\x04(2, 2)"));
    CHECK(tile->extra2 == BWAPI::Text::Size::Small);
  }
  if (fps != nullptr)
    CHECK(std::string(data->strings[fps->extra1]) ==
          std::string("\x03" "Frame Rate (Logical Frames)\nLFPS: 24\nAvg LFPS:23.750"));
}

}  // namespace

int main() {
  local_settings_apply();
  flags_only_on_start();
  everything_else_is_not_executed();
  unit_commands_map_to_bwr_commands();
  batch_is_submitted_once();
  optimizer_groups_by_level();
  optimizer_groups_only_equal_commands();
  optimizer_groups_what_official_groups();
  optimizer_rewrites_commands_like_official();
  optimizer_splits_groups_by_unit_state();
  optimizer_level4_rewrites_unit_techs_to_positions();
  optimizer_level_persists_and_resets();
  grouped_results_are_polled();
  apm_actions_follow_submitted_groups();
  unmappable_commands_are_logged();
  results_are_polled_on_later_frames();
  game_commands_map_to_bwr_commands();
  gui_off_stops_drawing_and_screen_commands();
  failed_submit_is_logged();
  submitted_commands_keep_client_form();
  chat_commands_parse_official_lines();
  recognized_chat_command_reaches_the_game();
  single_player_cheat_text_becomes_game_cheat();
  grid_and_fps_shapes_follow_official_form();
  if (failures != 0) std::cerr << failures << " check(s) failed\n";
  return failures == 0 ? 0 : 1;
}
