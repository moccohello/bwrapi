#include "tournament_policy.h"

#include <algorithm>
#include <cstring>
#include <iostream>
#include <vector>

namespace BWRAPI::Legacy44Tournament {

namespace {

const char* action_name(BWAPI::Tournament::ActionID action) {
  switch (action) {
    case BWAPI::Tournament::EnableFlag: return "EnableFlag";
    case BWAPI::Tournament::PauseGame: return "PauseGame";
    case BWAPI::Tournament::ResumeGame: return "ResumeGame";
    case BWAPI::Tournament::LeaveGame: return "LeaveGame";
    case BWAPI::Tournament::SetLocalSpeed: return "SetLocalSpeed";
    case BWAPI::Tournament::SetTextSize: return "SetTextSize";
    case BWAPI::Tournament::SetLatCom: return "SetLatCom";
    case BWAPI::Tournament::SetGUI: return "SetGUI";
    case BWAPI::Tournament::SetMap: return "SetMap";
    case BWAPI::Tournament::SetFrameSkip: return "SetFrameSkip";
    case BWAPI::Tournament::Printf: return "Printf";
    case BWAPI::Tournament::SendText: return "SendText";
    case BWAPI::Tournament::SetCommandOptimizationLevel:
      return "SetCommandOptimizationLevel";
    default: return "Unknown";
  }
}

bool mapped_action(BWAPIC::CommandType::Enum command,
                   BWAPI::Tournament::ActionID& action) {
  switch (command) {
    case BWAPIC::CommandType::EnableFlag:
      action = BWAPI::Tournament::EnableFlag;
      return true;
    case BWAPIC::CommandType::Printf:
      action = BWAPI::Tournament::Printf;
      return true;
    case BWAPIC::CommandType::SendText:
      action = BWAPI::Tournament::SendText;
      return true;
    case BWAPIC::CommandType::PauseGame:
      action = BWAPI::Tournament::PauseGame;
      return true;
    case BWAPIC::CommandType::ResumeGame:
      action = BWAPI::Tournament::ResumeGame;
      return true;
    case BWAPIC::CommandType::LeaveGame:
      action = BWAPI::Tournament::LeaveGame;
      return true;
    case BWAPIC::CommandType::SetLocalSpeed:
      action = BWAPI::Tournament::SetLocalSpeed;
      return true;
    case BWAPIC::CommandType::SetLatCom:
      action = BWAPI::Tournament::SetLatCom;
      return true;
    case BWAPIC::CommandType::SetGui:
      action = BWAPI::Tournament::SetGUI;
      return true;
    case BWAPIC::CommandType::SetFrameSkip:
      action = BWAPI::Tournament::SetFrameSkip;
      return true;
    case BWAPIC::CommandType::SetMap:
      action = BWAPI::Tournament::SetMap;
      return true;
    case BWAPIC::CommandType::SetCommandOptimizerLevel:
      action = BWAPI::Tournament::SetCommandOptimizationLevel;
      return true;
    default:
      return false;
  }
}

int report_seh(const char* operation, EXCEPTION_POINTERS* pointers) {
  std::cerr << "tournament loader: SEH exception in " << operation
            << " code=0x" << std::hex
            << pointers->ExceptionRecord->ExceptionCode << std::dec << '\n';
  return EXCEPTION_EXECUTE_HANDLER;
}

bool call_on_action_seh(BWAPI::TournamentModule& controller,
                        BWAPI::Tournament::ActionID action, void* parameter,
                        bool& allowed) {
  __try {
    allowed = controller.onAction(action, parameter);
    return true;
  } __except (report_seh("TournamentModule::onAction", GetExceptionInformation())) {
    return false;
  }
}

bool call_on_action(BWAPI::TournamentModule& controller,
                    BWAPI::Tournament::ActionID action, void* parameter,
                    bool& allowed, std::string& error) {
  try {
    if (call_on_action_seh(controller, action, parameter, allowed)) return true;
    error = std::string("SEH exception in TournamentModule::onAction(") +
            action_name(action) + ")";
  } catch (const std::exception& exception) {
    error = std::string("C++ exception in TournamentModule::onAction(") +
            action_name(action) + "): " + exception.what();
  } catch (...) {
    error = std::string("unknown exception in TournamentModule::onAction(") +
            action_name(action) + ")";
  }
  return false;
}

const char* checked_string(BWAPI::GameData& data, int index, int string_limit) {
  if (index < 0 || index >= string_limit) return nullptr;
  char* value = data.strings[index];
  return std::memchr(value, '\0', sizeof(data.strings[index])) == nullptr
             ? nullptr
             : value;
}

void replay_local_action(BWAPI::GameData& data, const BWAPIC::Command& command) {
  if (command.type == BWAPIC::CommandType::SetLatCom)
    data.hasLatCom = command.value1 == 1;
  else if (command.type == BWAPIC::CommandType::SetGui)
    data.hasGUI = command.value1 == 1;
}

}  // namespace

void capture_boundary(const BWAPI::GameData& data, CallbackBoundary& boundary) {
  boundary.command_count = data.commandCount;
  boundary.shape_count = data.shapeCount;
  boundary.string_count = data.stringCount;
  boundary.unit_command_count = data.unitCommandCount;
  boundary.has_lat_com = data.hasLatCom;
  boundary.has_gui = data.hasGUI;
}

void ImmediateActionTrace::clear() {
  command_indices_.clear();
}

void ImmediateActionTrace::remember_command(int index) {
  if (index >= 0) command_indices_.push_back(index);
}

bool ImmediateActionTrace::contains_command(int index) const {
  return std::find(command_indices_.begin(), command_indices_.end(), index) !=
         command_indices_.end();
}

bool authorize_immediate_action(BWAPI::GameData& data,
                                BWAPI::TournamentModule& controller,
                                BWAPI::Tournament::ActionID action,
                                void* parameter,
                                ImmediateActionTrace& trace,
                                bool& allowed,
                                std::string& error) {
  const int controller_command_begin = data.commandCount;
  if (!call_on_action(controller, action, parameter, allowed, error)) return false;
  for (int index = controller_command_begin; index < data.commandCount; ++index)
    trace.remember_command(index);
  return true;
}

bool gate_regular_output(BWAPI::GameData& data, const CallbackBoundary& boundary,
                         BWAPI::TournamentModule& controller,
                         const ImmediateActionTrace& immediate_actions,
                         std::string& error) {
  const int regular_command_end = data.commandCount;
  const int regular_string_end = data.stringCount;
  std::vector<BWAPIC::Command> regular_commands(
      data.commands + boundary.command_count,
      data.commands + regular_command_end);

  // BWAPIClient writes these two shared flags before queuing their commands.
  // Rebuild them from the pre-callback state so a denied local action is undone.
  data.hasLatCom = boundary.has_lat_com;
  data.hasGUI = boundary.has_gui;

  std::vector<BWAPIC::Command> allowed_commands;
  allowed_commands.reserve(regular_commands.size());
  for (std::size_t offset = 0; offset < regular_commands.size(); ++offset) {
    const BWAPIC::Command& command = regular_commands[offset];
    const int command_index = boundary.command_count + static_cast<int>(offset);
    if (immediate_actions.contains_command(command_index)) {
      allowed_commands.push_back(command);
      replay_local_action(data, command);
      continue;
    }
    BWAPI::Tournament::ActionID action = BWAPI::Tournament::EnableFlag;
    if (!mapped_action(command.type, action)) {
      allowed_commands.push_back(command);
      continue;
    }

    int integer_parameter = command.value1;
    bool bool_parameter = command.value1 == 1;
    void* parameter = nullptr;
    bool valid_parameter = true;
    switch (command.type) {
      case BWAPIC::CommandType::EnableFlag:
      case BWAPIC::CommandType::SetLocalSpeed:
      case BWAPIC::CommandType::SetFrameSkip:
        parameter = &integer_parameter;
        break;
      case BWAPIC::CommandType::SetCommandOptimizerLevel:
        integer_parameter = std::max(0, std::min(4, integer_parameter));
        parameter = &integer_parameter;
        break;
      case BWAPIC::CommandType::SetLatCom:
      case BWAPIC::CommandType::SetGui:
        parameter = &bool_parameter;
        break;
      case BWAPIC::CommandType::Printf:
      case BWAPIC::CommandType::SendText:
      case BWAPIC::CommandType::SetMap:
        parameter = const_cast<char*>(
            checked_string(data, command.value1, regular_string_end));
        valid_parameter = parameter != nullptr;
        break;
      default:
        break;
    }

    const int bypass_begin = data.commandCount;
    bool allowed = false;
    if (valid_parameter &&
        !call_on_action(controller, action, parameter, allowed, error)) {
      return false;
    }
    // controller가 호출 도중 발행한 명령은 허용된 원래 명령보다 먼저 실행된다.
    for (int index = bypass_begin; index < data.commandCount; ++index) {
      allowed_commands.push_back(data.commands[index]);
      replay_local_action(data, data.commands[index]);
    }
    if (!valid_parameter) {
      std::cerr << "tournament policy: denied malformed " << action_name(action)
                << " command parameter\n";
    } else if (!allowed) {
      std::cerr << "tournament policy: denied " << action_name(action) << '\n';
    } else {
      BWAPIC::Command accepted = command;
      if (parameter == &integer_parameter) accepted.value1 = integer_parameter;
      else if (parameter == &bool_parameter) accepted.value1 = bool_parameter ? 1 : 0;
      allowed_commands.push_back(accepted);
      replay_local_action(data, accepted);
    }
  }

  // controller 명령은 재귀 심사 없이 각 호출 위치에 보존한다.
  int write_index = boundary.command_count;
  for (const BWAPIC::Command& command : allowed_commands)
    data.commands[write_index++] = command;
  data.commandCount = write_index;
  return true;
}

}  // namespace BWRAPI::Legacy44Tournament
