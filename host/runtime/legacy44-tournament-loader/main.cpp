#include "tournament_policy.h"

#include <windows.h>
#include <BWAPI/Client/GameImpl.h>
#include <svnrev.h>

#include <chrono>
#include <cstdint>
#include <iostream>
#include <sstream>
#include <string>
#include <thread>

namespace {

class TournamentGame final : public BWAPI::GameImpl {
 public:
  explicit TournamentGame(BWAPI::GameData* data) : BWAPI::GameImpl(data), data_(data) {}
  int getLastEventTime() const override { return last_event_time_; }
  void set_last_event_time(DWORD elapsed) {
    last_event_time_ = static_cast<std::int32_t>(elapsed);
  }
  void attach_controller(BWAPI::TournamentModule* controller) {
    controller_ = controller;
  }
  void begin_regular_callback() {
    immediate_actions_.clear();
    policy_error_.clear();
  }
  const BWRAPI::Legacy44Tournament::ImmediateActionTrace& immediate_actions() const {
    return immediate_actions_;
  }
  bool take_policy_error(std::string& error) const {
    if (policy_error_.empty()) return false;
    error = policy_error_;
    return true;
  }
  bool tournament_call() const { return tournament_call_; }
  void set_tournament_call(bool value) { tournament_call_ = value; }

  void setTextSize(BWAPI::Text::Size::Enum size) override {
    if (size < BWAPI::Text::Size::Small) size = BWAPI::Text::Size::Small;
    if (size > BWAPI::Text::Size::Huge) size = BWAPI::Text::Size::Huge;
    if (!authorize(BWAPI::Tournament::SetTextSize, &size)) return;
    BWAPI::GameImpl::setTextSize(size);
  }

  void setGUI(bool enabled) override {
    if (!authorize(BWAPI::Tournament::SetGUI, &enabled)) return;
    const int command_begin = data_->commandCount;
    BWAPI::GameImpl::setGUI(enabled);
    remember_appended_commands(command_begin);
    // Pinned GameDrawing.cpp calls this after an allowed SetGUI action.  It is
    // a separate onAction(SetFrameSkip) decision, including -1 when GUI opens.
    setFrameSkip(enabled ? -1 : 9999999);
  }

  void setFrameSkip(int frame_skip) override {
    BWAPI::GameImpl::setLastError(BWAPI::Errors::None);
    if (!authorize(BWAPI::Tournament::SetFrameSkip, &frame_skip)) {
      return;
    }
    const int command_begin = data_->commandCount;
    BWAPI::GameImpl::setFrameSkip(frame_skip);
    remember_appended_commands(command_begin);
  }

 private:
  bool authorize(BWAPI::Tournament::ActionID action, void* parameter) {
    if (controller_ == nullptr || tournament_call_) return true;
    bool allowed = false;
    const bool was_tournament_call = tournament_call_;
    tournament_call_ = true;
    const bool invoked = BWRAPI::Legacy44Tournament::authorize_immediate_action(
        *data_, *controller_, action, parameter, immediate_actions_, allowed, policy_error_);
    tournament_call_ = was_tournament_call;
    return invoked && allowed;
  }

  void remember_appended_commands(int command_begin) {
    for (int index = command_begin; index < data_->commandCount; ++index)
      immediate_actions_.remember_command(index);
  }

  BWAPI::GameData* data_ = nullptr;
  BWAPI::TournamentModule* controller_ = nullptr;
  BWRAPI::Legacy44Tournament::ImmediateActionTrace immediate_actions_;
  std::string policy_error_;
  bool tournament_call_ = false;
  int last_event_time_ = 0;
};

class TournamentCallScope {
 public:
  explicit TournamentCallScope(TournamentGame& game)
      : game_(game), previous_(game.tournament_call()) {
    game_.set_tournament_call(true);
  }
  ~TournamentCallScope() { game_.set_tournament_call(previous_); }

 private:
  TournamentGame& game_;
  bool previous_;
};

using GameInitFn = void (__cdecl*)(BWAPI::Game*);
using NewAIModuleFn = BWAPI::AIModule* (__cdecl*)();
using NewTournamentModuleFn = BWAPI::TournamentModule* (__cdecl*)();

int report_seh(const char* operation, EXCEPTION_POINTERS* pointers) {
  // Let C++ exceptions reach dispatch_event's catch so SourceUnavailable
  // retains its getter/reason instead of becoming an opaque SEH code.
  if (pointers->ExceptionRecord->ExceptionCode == 0xe06d7363)
    return EXCEPTION_CONTINUE_SEARCH;
  std::cerr << "tournament loader: SEH exception in " << operation
            << " code=0x" << std::hex
            << pointers->ExceptionRecord->ExceptionCode << std::dec << '\n';
  return EXCEPTION_EXECUTE_HANDLER;
}

const char* event_name(BWAPI::EventType::Enum type) {
  switch (type) {
    case BWAPI::EventType::MatchStart: return "onStart";
    case BWAPI::EventType::MatchEnd: return "onEnd";
    case BWAPI::EventType::MatchFrame: return "onFrame";
    case BWAPI::EventType::MenuFrame: return "MenuFrame";
    case BWAPI::EventType::SendText: return "onSendText";
    case BWAPI::EventType::ReceiveText: return "onReceiveText";
    case BWAPI::EventType::PlayerLeft: return "onPlayerLeft";
    case BWAPI::EventType::NukeDetect: return "onNukeDetect";
    case BWAPI::EventType::UnitDiscover: return "onUnitDiscover";
    case BWAPI::EventType::UnitEvade: return "onUnitEvade";
    case BWAPI::EventType::UnitShow: return "onUnitShow";
    case BWAPI::EventType::UnitHide: return "onUnitHide";
    case BWAPI::EventType::UnitCreate: return "onUnitCreate";
    case BWAPI::EventType::UnitDestroy: return "onUnitDestroy";
    case BWAPI::EventType::UnitMorph: return "onUnitMorph";
    case BWAPI::EventType::UnitRenegade: return "onUnitRenegade";
    case BWAPI::EventType::SaveGame: return "onSaveGame";
    case BWAPI::EventType::UnitComplete: return "onUnitComplete";
    default: return "unknown event";
  }
}

void dispatch_event_unchecked(BWAPI::AIModule& module, const BWAPI::Event& event) {
  switch (event.getType()) {
    case BWAPI::EventType::MatchStart:
      module.onStart();
      break;
    case BWAPI::EventType::MatchEnd:
      module.onEnd(event.isWinner());
      break;
    case BWAPI::EventType::MatchFrame:
      module.onFrame();
      break;
    case BWAPI::EventType::MenuFrame:
      break;
    case BWAPI::EventType::SendText:
      module.onSendText(event.getText());
      break;
    case BWAPI::EventType::ReceiveText:
      module.onReceiveText(event.getPlayer(), event.getText());
      break;
    case BWAPI::EventType::PlayerLeft:
      module.onPlayerLeft(event.getPlayer());
      break;
    case BWAPI::EventType::NukeDetect:
      module.onNukeDetect(event.getPosition());
      break;
    case BWAPI::EventType::UnitDiscover:
      module.onUnitDiscover(event.getUnit());
      break;
    case BWAPI::EventType::UnitEvade:
      module.onUnitEvade(event.getUnit());
      break;
    case BWAPI::EventType::UnitShow:
      module.onUnitShow(event.getUnit());
      break;
    case BWAPI::EventType::UnitHide:
      module.onUnitHide(event.getUnit());
      break;
    case BWAPI::EventType::UnitCreate:
      module.onUnitCreate(event.getUnit());
      break;
    case BWAPI::EventType::UnitDestroy:
      module.onUnitDestroy(event.getUnit());
      break;
    case BWAPI::EventType::UnitMorph:
      module.onUnitMorph(event.getUnit());
      break;
    case BWAPI::EventType::UnitRenegade:
      module.onUnitRenegade(event.getUnit());
      break;
    case BWAPI::EventType::SaveGame:
      module.onSaveGame(event.getText());
      break;
    case BWAPI::EventType::UnitComplete:
      module.onUnitComplete(event.getUnit());
      break;
    default:
      break;
  }
}

bool dispatch_event_seh(BWAPI::AIModule& module, const BWAPI::Event& event,
                        const char* owner) {
  __try {
    dispatch_event_unchecked(module, event);
    return true;
  } __except (report_seh(owner, GetExceptionInformation())) {
    return false;
  }
}

bool dispatch_event(BWAPI::AIModule& module, const BWAPI::Event& event,
                    const char* owner, std::string& error, DWORD* elapsed = nullptr) {
  try {
    const DWORD started = GetTickCount();
    if (dispatch_event_seh(module, event, owner)) {
      if (elapsed != nullptr) *elapsed = GetTickCount() - started;
      if (event.getType() == BWAPI::EventType::MatchStart ||
          event.getType() == BWAPI::EventType::MatchEnd) {
        std::cerr << "tournament loader: callback returned owner=" << owner
                  << " callback=" << event_name(event.getType());
        if (event.getType() == BWAPI::EventType::MatchEnd)
          std::cerr << " winner=" << (event.isWinner() ? "true" : "false");
        std::cerr << '\n';
      }
      return true;
    }
    error = std::string("SEH exception in ") + owner + " " +
            event_name(event.getType());
  } catch (const std::exception& exception) {
    error = std::string("C++ exception in ") + owner + " " +
            event_name(event.getType()) + ": " + exception.what() +
            "; retry=false command_resend=false";
  } catch (...) {
    error = std::string("unknown exception in ") + owner + " " +
            event_name(event.getType());
  }
  return false;
}

bool dispatch_first_advertisement_seh(BWAPI::TournamentModule& controller) {
  __try {
    controller.onFirstAdvertisement();
    return true;
  } __except (report_seh("TournamentModule::onFirstAdvertisement",
                        GetExceptionInformation())) {
    return false;
  }
}

bool dispatch_first_advertisement(BWAPI::TournamentModule& controller,
                                  std::string& error) {
  try {
    if (dispatch_first_advertisement_seh(controller)) return true;
    error = "SEH exception in TournamentModule::onFirstAdvertisement";
  } catch (const std::exception& exception) {
    error = std::string("C++ exception in TournamentModule::onFirstAdvertisement: ") +
            exception.what();
  } catch (...) {
    error = "unknown exception in TournamentModule::onFirstAdvertisement";
  }
  return false;
}

const std::string& tournament_advertisement() {
  static const std::string message = [] {
    std::ostringstream stream;
    stream << "BWAPI " BWAPI_VER "." << SVN_REV << " Tournament Mode Engaged!";
    return stream.str();
  }();
  return message;
}

class MatchModules {
 public:
  ~MatchModules() { close(); }
  MatchModules(const MatchModules&) = delete;
  MatchModules& operator=(const MatchModules&) = delete;
  MatchModules() = default;

  bool open(const std::string& ai_path, const std::string& tournament_path,
            BWAPI::Game* game, std::string& error) {
    close();
    tournament_handle_ = LoadLibraryExA(
        tournament_path.c_str(), nullptr, LOAD_WITH_ALTERED_SEARCH_PATH);
    if (tournament_handle_ == nullptr) {
      error = "failed to load tournament module; GetLastError=" +
              std::to_string(GetLastError());
      return false;
    }
    tournament_game_init_ = reinterpret_cast<GameInitFn>(
        GetProcAddress(tournament_handle_, "gameInit"));
    new_tournament_ai_ = reinterpret_cast<NewAIModuleFn>(
        GetProcAddress(tournament_handle_, "newTournamentAI"));
    new_tournament_module_ = reinterpret_cast<NewTournamentModuleFn>(
        GetProcAddress(tournament_handle_, "newTournamentModule"));
    if (tournament_game_init_ == nullptr || new_tournament_ai_ == nullptr ||
        new_tournament_module_ == nullptr) {
      error = "tournament DLL must export gameInit, newTournamentAI, and "
              "newTournamentModule";
      return false;
    }

    ai_handle_ = LoadLibraryExA(
        ai_path.c_str(), nullptr, LOAD_WITH_ALTERED_SEARCH_PATH);
    if (ai_handle_ == nullptr) {
      error = "failed to load AIModule; GetLastError=" +
              std::to_string(GetLastError());
      return false;
    }
    ai_game_init_ = reinterpret_cast<GameInitFn>(
        GetProcAddress(ai_handle_, "gameInit"));
    new_ai_module_ = reinterpret_cast<NewAIModuleFn>(
        GetProcAddress(ai_handle_, "newAIModule"));
    if (ai_game_init_ == nullptr || new_ai_module_ == nullptr) {
      error = "AIModule DLL must export gameInit and newAIModule";
      return false;
    }

    try {
      if (!create_tournament_seh(game)) {
        error = "SEH exception while initializing module exports";
        return false;
      }
      if (tournament_ai_ == nullptr || controller_ == nullptr ||
          BWAPI::BWAPIClient.data == nullptr) {
        error = "tournament factory or GameData mapping is unavailable";
        return false;
      }
      auto& tournament_game = *static_cast<TournamentGame*>(game);
      tournament_game.attach_controller(controller_);
      BWRAPI::Legacy44Tournament::CallbackBoundary initialization_boundary;
      BWRAPI::Legacy44Tournament::capture_boundary(
          *BWAPI::BWAPIClient.data, initialization_boundary);
      tournament_game.begin_regular_callback();
      if (!create_ai_seh(game)) {
        error = "SEH exception while initializing AIModule exports";
        return false;
      }
      if (tournament_game.take_policy_error(error)) return false;
      {
        TournamentCallScope tournament_call(tournament_game);
        if (!BWRAPI::Legacy44Tournament::gate_regular_output(
                *BWAPI::BWAPIClient.data, initialization_boundary, *controller_,
                tournament_game.immediate_actions(), error)) return false;
      }
    } catch (const std::exception& exception) {
      error = std::string("C++ exception while initializing module exports: ") +
              exception.what();
      return false;
    } catch (...) {
      error = "unknown exception while initializing module exports";
      return false;
    }
    if (ai_ == nullptr || tournament_ai_ == nullptr || controller_ == nullptr) {
      error = "a module factory returned null";
      return false;
    }
    return true;
  }

  BWAPI::AIModule& ai() const { return *ai_; }
  BWAPI::AIModule& tournament_ai() const { return *tournament_ai_; }
  BWAPI::TournamentModule& controller() const { return *controller_; }

  void observe_advertisement(const BWAPI::Event& event) {
    if (!advertisement_seen_ && event.getType() == BWAPI::EventType::ReceiveText &&
        event.getText() == tournament_advertisement()) {
      advertisement_seen_ = true;
    }
  }

  bool emit_first_advertisement(TournamentGame& game, std::string& error) {
    if (advertisement_seen_) return true;
    const BWAPI::Player player = game.self();
    if (player == nullptr) return true;
    const int player_id = player->getID();
    if (player_id < 0 || game.getFrameCount() <= player_id * 8) return true;

    advertisement_seen_ = true;
    TournamentCallScope tournament_call(game);
    const std::string& message = tournament_advertisement();
    game.sendText("%s", message.c_str());
    return dispatch_first_advertisement(*controller_, error);
  }

 private:
  bool create_tournament_seh(BWAPI::Game* game) {
    __try {
      // Pinned GameImpl initializes the tournament DLL before the regular AI.
      tournament_game_init_(game);
      tournament_ai_ = new_tournament_ai_();
      controller_ = new_tournament_module_();
      return true;
    } __except (report_seh("tournament initialization", GetExceptionInformation())) {
      return false;
    }
  }

  bool create_ai_seh(BWAPI::Game* game) {
    __try {
      ai_game_init_(game);
      ai_ = new_ai_module_();
      return true;
    } __except (report_seh("module initialization", GetExceptionInformation())) {
      return false;
    }
  }

  void close() {
    delete ai_;
    delete tournament_ai_;
    delete controller_;
    ai_ = nullptr;
    tournament_ai_ = nullptr;
    controller_ = nullptr;
    if (ai_handle_ != nullptr) FreeLibrary(ai_handle_);
    if (tournament_handle_ != nullptr) FreeLibrary(tournament_handle_);
    ai_handle_ = nullptr;
    tournament_handle_ = nullptr;
    ai_game_init_ = nullptr;
    new_ai_module_ = nullptr;
    tournament_game_init_ = nullptr;
    new_tournament_ai_ = nullptr;
    new_tournament_module_ = nullptr;
    advertisement_seen_ = false;
  }

  HMODULE ai_handle_ = nullptr;
  HMODULE tournament_handle_ = nullptr;
  GameInitFn ai_game_init_ = nullptr;
  NewAIModuleFn new_ai_module_ = nullptr;
  GameInitFn tournament_game_init_ = nullptr;
  NewAIModuleFn new_tournament_ai_ = nullptr;
  NewTournamentModuleFn new_tournament_module_ = nullptr;
  BWAPI::AIModule* ai_ = nullptr;
  BWAPI::AIModule* tournament_ai_ = nullptr;
  BWAPI::TournamentModule* controller_ = nullptr;
  bool advertisement_seen_ = false;
};

bool process_event(MatchModules& modules, const BWAPI::Event& event,
                   std::string& error) {
  if (BWAPI::BWAPIClient.data == nullptr) {
    error = "BWAPIClient GameData mapping is unavailable";
    return false;
  }

  BWRAPI::Legacy44Tournament::CallbackBoundary regular_boundary;
  BWRAPI::Legacy44Tournament::capture_boundary(
      *BWAPI::BWAPIClient.data, regular_boundary);
  auto& game = *static_cast<TournamentGame*>(BWAPI::BroodwarPtr);
  game.begin_regular_callback();
  game.set_last_event_time(0);
  DWORD elapsed = 0;
  if (!dispatch_event(modules.ai(), event, "AIModule", error, &elapsed)) return false;
  if (game.take_policy_error(error)) return false;
  {
    TournamentCallScope tournament_call(game);
    if (!BWRAPI::Legacy44Tournament::gate_regular_output(
            *BWAPI::BWAPIClient.data, regular_boundary, modules.controller(),
            game.immediate_actions(), error)) {
      return false;
    }
  }

  // 출력을 심사하는 시간은 AI 콜백 경과 시간에 포함하지 않는다.
  game.set_last_event_time(elapsed);
  return dispatch_event(modules.tournament_ai(), event, "TournamentAI", error);
}

void reconnect() {
  while (!BWAPI::BWAPIClient.connect())
    std::this_thread::sleep_for(std::chrono::milliseconds(1000));
  // 첫 update 전에 교체한다. Client는 이후에도 GameImpl 수명/상태를 관리한다.
  auto* game = new TournamentGame(BWAPI::BWAPIClient.data);
  delete static_cast<BWAPI::GameImpl*>(BWAPI::BroodwarPtr);
  BWAPI::BroodwarPtr = game;
}

void print_usage() {
  std::cerr << "usage: bwrapi-legacy44-tournament-loader.exe "
               "<aimodule.dll> <tournament.dll>\n";
}

}  // namespace

int main(int argc, char** argv) {
  try {
  static_assert(sizeof(void*) == 4,
                "bwrapi-legacy44-tournament-loader must be built as x86");
  if (argc != 3) {
    print_usage();
    return 2;
  }

  const std::string ai_path = argv[1];
  const std::string tournament_path = argv[2];
  reconnect();
  for (;;) {
    while (BWAPI::BWAPIClient.isConnected() &&
           (BWAPI::BroodwarPtr == nullptr || !BWAPI::Broodwar->isInGame())) {
      BWAPI::BWAPIClient.update();
    }
    if (!BWAPI::BWAPIClient.isConnected()) {
      reconnect();
      continue;
    }
    if (BWAPI::BroodwarPtr == nullptr) {
      std::cerr << "tournament loader: connected without a Game interface\n";
      return 3;
    }

    MatchModules modules;
    std::string error;
    if (!modules.open(ai_path, tournament_path, BWAPI::BroodwarPtr, error)) {
      std::cerr << "tournament loader: " << error << '\n';
      BWAPI::BWAPIClient.disconnect();
      return error.find("SourceUnavailable") != std::string::npos ? 24 : 4;
    }
    std::cerr << "tournament loader: entered match\n";

    while (BWAPI::BWAPIClient.isConnected() && BWAPI::BroodwarPtr != nullptr &&
           BWAPI::Broodwar->isInGame()) {
      const auto& event_list = BWAPI::Broodwar->getEvents();
      for (const BWAPI::Event& event : event_list)
        modules.observe_advertisement(event);
      auto& game = *static_cast<TournamentGame*>(BWAPI::BroodwarPtr);
      if (!modules.emit_first_advertisement(game, error)) {
        std::cerr << "tournament loader: fail-closed: " << error << '\n';
        BWAPI::BWAPIClient.disconnect();
        return error.find("SourceUnavailable") != std::string::npos ? 24 : 5;
      }
      for (const BWAPI::Event& event : event_list) {
        if (!process_event(modules, event, error)) {
          std::cerr << "tournament loader: fail-closed: " << error << '\n';
          BWAPI::BWAPIClient.disconnect();
          return error.find("SourceUnavailable") != std::string::npos ? 24 : 5;
        }
      }
      BWAPI::BWAPIClient.update();
    }
    std::cerr << "tournament loader: game ended\n";
  }
  } catch (const std::exception& error) {
    std::cerr << "[loader-update-failed] reason=" << error.what()
              << " retry=false command_resend=false\n";
    BWAPI::BWAPIClient.disconnect();
    return 24;
  } catch (...) {
    std::cerr << "[loader-update-failed] reason=unknown exception retry=false command_resend=false\n";
    BWAPI::BWAPIClient.disconnect();
    return 24;
  }
}
