// bwrapi_bwapi.dll: the BWAPI 4.4 server inside the game. Injected beside
// bwrapi.dll, it fills GameData on the game thread at every frame boundary
// and waits there for the client, as the original BWAPI server does, so the
// game runs at the client's pace. Without a client the frame is not read and
// nothing waits.
//
// Two threads drive one BwapiServer under one lock:
// - the game thread's frame callback serves the frames of a match: acquire,
//   projection, ready, the client's update, its commands, release;
// - the server thread serves everything the game has no frame for: opening
//   the server, accepting a client, the menu, the end of a match and the
//   repeated frames of a paused game. Only paused rounds submit commands
//   from it: the frame stands still, so its read names the frame the game
//   thread runs them on.
// The server thread holds the lock only for one step and sleeps without it,
// so the game thread waits on it only while the client answers a round.
// Each round hands the client's draw shapes to bwrapi.dll, which draws them
// into every screen update the game draws until the next round.
//
// A client is served synchronously unless bwrapi_bwapi.mode beside this DLL
// reads `async` when it connects: then the game does not wait for it, and each
// frame after its update executes its commands and hands it that frame.

#include "legacy44_common.h"
#include "legacy44_draw.h"
#include "legacy44_server.h"
#include "legacy44_session.h"

#include <windows.h>

#include <exception>
#include <fstream>
#include <memory>
#include <mutex>
#include <string>
#include <thread>
#include <type_traits>

namespace BWRAPI::Legacy44 {

namespace {

// The server thread's step between two looks at the game.
constexpr DWORD kServeMs = 10;
// A paused game repeats its frame; BWAPI calls onFrame on those loops too.
constexpr ULONGLONG kPausedRoundMs = 20;
constexpr ULONGLONG kOpenRetryMs = 1000;
constexpr DWORD kIoTimeoutMs = 5000;

// The ABI of the bwrapi.dll already in the game. Linking it would let the
// loader map a second copy from this DLL's directory with its own frame hook.
bool resolve_abi(AbiFunctions& api) {
  const HMODULE bwrapi = GetModuleHandleW(L"bwrapi.dll");
  if (bwrapi == nullptr) return false;
  bool found = true;
  const auto entry = [&](auto& slot, const char* name) {
    slot = reinterpret_cast<std::remove_reference_t<decltype(slot)>>(GetProcAddress(bwrapi, name));
    found = found && slot != nullptr;
  };
  entry(api.open_self, "bwr_open_self");
  entry(api.close, "bwr_close");
  entry(api.subscribe, "bwr_subscribe");
  entry(api.step, "bwr_step");
  entry(api.acquire, "bwr_acquire");
  entry(api.release, "bwr_release");
  entry(api.last_error, "bwr_last_error");
  entry(api.set_frame_callback, "bwr_set_frame_callback");
  entry(api.submit, "bwr_submit");
  entry(api.poll_result, "bwr_poll_result");
  entry(api.capture_chat, "bwr_capture_chat");
  entry(api.next_chat, "bwr_next_chat");
  entry(api.draw, "bwr_draw");
  return found;
}

// Directory of this DLL with a trailing separator; the log and status files
// live beside it because the game has no console.
std::string module_directory(HMODULE module) {
  char path[MAX_PATH]{};
  const DWORD length = GetModuleFileNameA(module, path, MAX_PATH);
  std::string directory(path, length);
  const auto slash = directory.find_last_of("/\\");
  return slash == std::string::npos ? std::string() : directory.substr(0, slash + 1);
}

class GameServer {
 public:
  explicit GameServer(const std::string& directory)
      : directory_(directory),
        log_(directory + "bwrapi_bwapi.log", std::ios::app),
        status_(directory + "bwrapi_bwapi.status.jsonl") {
    if (log_) std::cerr.rdbuf(log_.rdbuf());
  }

  // On the game thread, from the first window message.
  bool start(std::string& error) {
    if (!resolve_abi(api_)) {
      error = "bwrapi.dll is not loaded in the game or lacks the ABI 1.0 exports";
      return false;
    }
    if (!session_.open_self(kHostSources, error)) return false;
    // Reads only the game row: it tells the server thread whether a match
    // runs and whether it is paused, and names a paused frame for commands.
    if (!presence_.open_self(BWR_SOURCE_GAME, error)) return false;
    server_ = std::make_unique<BwapiServer>(pid_, status_, kIoTimeoutMs);
    server_->set_input(&read_input);
    // Rounds of both threads run under the lock, so the frame callback's
    // session serves them one at a time.
    server_->set_draw([this](const BWAPI::GameData* data) { session_.draw(draw_.build(data)); });
    if (session_.set_frame_callback(&GameServer::frame_callback, this) != BWR_OK) {
      error = "bwr_set_frame_callback failed: " + session_.last_error();
      return false;
    }
    std::thread([this] { serve(); }).detach();
    return true;
  }

 private:
  static void frame_callback(void* user) { static_cast<GameServer*>(user)->on_frame(); }

  void on_frame() {
    std::lock_guard<std::mutex> lock(mutex_);
    try {
      if (!opened_ || !server_->connected() || server_->client_busy()) return;
      AbiFrame frame;
      if (session_.acquire(frame) != BWR_OK) return;
      take_chat(session_);
      std::string error;
      if (!server_->frame(*frame, session_.sink(), error)) fail(error);
      last_round_ = GetTickCount64();
    } catch (const std::exception& exception) {
      fail(std::string("frame callback failed: ") + exception.what());
    } catch (...) {
      fail("frame callback failed");
    }
  }

  void serve() {
    for (;;) {
      {
        std::lock_guard<std::mutex> lock(mutex_);
        try {
          serve_step();
        } catch (const std::exception& exception) {
          fail(std::string("server thread failed: ") + exception.what());
        } catch (...) {
          fail("server thread failed");
        }
      }
      Sleep(kServeMs);
    }
  }

  void serve_step() {
    std::string error;
    const ULONGLONG now = GetTickCount64();
    // The official server intercepts the local player's chat only while a
    // client is connected; then the typed line goes to the client instead.
    const bool capture = opened_ && server_->connected();
    if (capture != capturing_ && presence_.capture_chat(capture) == BWR_OK) capturing_ = capture;
    if (!opened_) {
      if (now < next_open_) return;
      if (!server_->open(error)) {
        if (error != open_error_) {
          status_.write("Failed", false, error, 0, {}, pid_);
          std::cerr << "[legacy44] BWAPI server open failed: " << error << '\n';
          open_error_ = error;
        }
        next_open_ = now + kOpenRetryMs;
        return;
      }
      opened_ = true;
      status_.write("AwaitingClient", false, "in-game BWAPI server open; waiting for a BWAPI client",
                    0, {}, pid_);
    }
    if (!server_->connected()) {
      // One look at the pipe; a waiting client is accepted on a later step.
      const bool async = async_mode();
      server_->set_async(async);
      const AcceptResult accepted = server_->accept(0, nullptr, error);
      if (accepted == AcceptResult::Failed) fail(error);
      if (accepted == AcceptResult::Connected)
        std::cerr << "[legacy44] client connected mode=" << (async ? "async" : "sync") << '\n';
      return;
    }
    server_->keep_alive();
    if (presence_.step(0) == BWR_NOT_IN_GAME) {
      bool idle = false;
      if (!server_->menu(nullptr, idle, error)) fail(error);
      return;
    }
    if (server_->match_active() && now - last_round_ >= kPausedRoundMs) {
      // A paused game has no frame callback, so this thread reads the pause
      // itself. The read names the paused frame, so commands (a resume among
      // them) queue against it; the game thread runs them while it stands.
      AbiFrame frame;
      if (presence_.acquire(frame) != BWR_OK || !paused(*frame)) return;
      take_chat(presence_);
      if (!server_->paused_frame(*frame, presence_.sink(), error)) fail(error);
      last_round_ = GetTickCount64();
    }
  }

  // GameImpl's key and mouse button state: the window thread's own key
  // state, only while the client enabled user input.
  static void read_input(BWAPI::GameData& data) {
    const bool enabled = data.flags[BWAPI::Flag::UserInput];
    for (int key = 0; key < BWAPI::K_MAX; ++key) data.keyState[key] = enabled && (GetKeyState(key) & 0x80) != 0;
    const int buttons[BWAPI::M_MAX] = {VK_LBUTTON, VK_RBUTTON, VK_MBUTTON};
    for (int button = 0; button < BWAPI::M_MAX; ++button)
      data.mouseState[button] = enabled && (GetKeyState(buttons[button]) & 0x80) != 0;
  }

  // Hands the chat lines the game took since the last round to the server; a
  // line of the local player may be a command it runs itself.
  void take_chat(AbiSession& session) {
    BwrChat line{};
    while (session.next_chat(line) == BWR_OK && line.kind != 0)
      server_->queue_chat(line.kind == BWR_CHAT_SENT, line.player, line.text);
  }

  static bool paused(const BwrFrame& frame) {
    return frame.game != nullptr && frame.game->rows > 0 && frame.game->paused[0] == 1;
  }

  bool async_mode() const {
    std::ifstream file(directory_ + "bwrapi_bwapi.mode");
    std::string mode;
    file >> mode;
    return mode == "async";
  }

  // A failed round ends the client's connection; the server keeps serving.
  void fail(const std::string& reason) {
    status_.write("Failed", false, reason, 0, {}, pid_);
    std::cerr << "[legacy44] " << reason << "; client disconnected\n";
    if (server_ != nullptr && server_->connected()) server_->drop_client();
  }

  std::string directory_;
  std::ofstream log_;
  StatusWriter status_;
  AbiFunctions api_;
  // The frame callback's session: frames and commands of the match.
  AbiSession session_{api_};
  // The server thread's session; a session serves one thread.
  AbiSession presence_{api_};
  const DWORD pid_ = GetCurrentProcessId();
  DrawList draw_;
  std::unique_ptr<BwapiServer> server_;
  std::mutex mutex_;
  bool opened_ = false;
  bool capturing_ = false;
  ULONGLONG next_open_ = 0;
  ULONGLONG last_round_ = 0;
  std::string open_error_;
};

void install() {
  // The injector removes its window hook right after this message, but the
  // frame callback keeps calling into this DLL, so it must never unload.
  HMODULE module = nullptr;
  if (!GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_PIN,
                          reinterpret_cast<LPCWSTR>(&install), &module))
    return;
  auto* server = new GameServer(module_directory(module));
  std::string error;
  if (!server->start(error)) {
    std::cerr << "[legacy44] in-game BWAPI server did not start: " << error << '\n';
    // A registered callback would still point at it, so it is never freed.
  }
}

}  // namespace

}  // namespace BWRAPI::Legacy44

// WH_CALLWNDPROC entry the injector loads this DLL through; the first message
// installs the server from the game thread. legacy44_game_server.def exports it.
extern "C" LRESULT CALLBACK bwrapi_bwapi_callwndproc(int code, WPARAM wparam, LPARAM lparam) {
  static std::once_flag installed;
  if (code >= 0 && lparam != 0) {
    std::call_once(installed, [] {
      try {
        BWRAPI::Legacy44::install();
      } catch (...) {
      }
    });
  }
  return CallNextHookEx(nullptr, code, wparam, lparam);
}
