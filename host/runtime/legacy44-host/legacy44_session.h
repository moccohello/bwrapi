#pragma once

#include "legacy44_command.h"
#include "legacy44_common.h"

#include <bwrapi.h>

#include <cstdint>
#include <string>

namespace BWRAPI::Legacy44 {

// Sources the host maps into GameData. ORDERS (queued orders) are not read.
constexpr std::uint64_t kHostSources =
    BWR_SOURCE_PLAYERS | BWR_SOURCE_UNITS | BWR_SOURCE_UPGRADES | BWR_SOURCE_TECHS |
    BWR_SOURCE_UNIT_COUNTS | BWR_SOURCE_BULLETS | BWR_SOURCE_VISION | BWR_SOURCE_VIEWPORT |
    BWR_SOURCE_SELECTION | BWR_SOURCE_RENDER | BWR_SOURCE_GAME | BWR_SOURCE_FORCES |
    BWR_SOURCE_START_LOCATIONS | BWR_SOURCE_UNIT_AVAILABILITY | BWR_SOURCE_TERRAIN |
    BWR_SOURCE_WALKABILITY | BWR_SOURCE_REGIONS | BWR_SOURCE_REGION_NEIGHBORS;

// The bwrapi.dll entry points the in-game server calls. It resolves them from
// the bwrapi.dll already in the game, so the game keeps one copy of the frame
// hook.
struct AbiFunctions {
  decltype(&bwr_open_self) open_self = nullptr;
  decltype(&bwr_close) close = nullptr;
  decltype(&bwr_subscribe) subscribe = nullptr;
  decltype(&bwr_step) step = nullptr;
  decltype(&bwr_acquire) acquire = nullptr;
  decltype(&bwr_release) release = nullptr;
  decltype(&bwr_last_error) last_error = nullptr;
  decltype(&bwr_set_frame_callback) set_frame_callback = nullptr;
  decltype(&bwr_submit) submit = nullptr;
  decltype(&bwr_poll_result) poll_result = nullptr;
  decltype(&bwr_capture_chat) capture_chat = nullptr;
  decltype(&bwr_next_chat) next_chat = nullptr;
  decltype(&bwr_draw) draw = nullptr;
};

// A frame held from bwr_acquire until it is released.
class AbiFrame {
 public:
  AbiFrame() = default;
  AbiFrame(const AbiFunctions* api, BwrSession* session, const BwrFrame* frame)
      : api_(api), session_(session), frame_(frame) {}
  ~AbiFrame() { reset(); }
  AbiFrame(const AbiFrame&) = delete;
  AbiFrame& operator=(const AbiFrame&) = delete;
  AbiFrame(AbiFrame&& other) noexcept { swap(other); }
  AbiFrame& operator=(AbiFrame&& other) noexcept {
    if (this != &other) {
      reset();
      swap(other);
    }
    return *this;
  }

  const BwrFrame& operator*() const { return *frame_; }
  void reset() {
    if (frame_ != nullptr) api_->release(session_, frame_);
    frame_ = nullptr;
  }

 private:
  void swap(AbiFrame& other) noexcept {
    std::swap(api_, other.api_);
    std::swap(session_, other.session_);
    std::swap(frame_, other.frame_);
  }
  const AbiFunctions* api_ = nullptr;
  BwrSession* session_ = nullptr;
  const BwrFrame* frame_ = nullptr;
};

// One bwrapi.dll session on the game process itself.
class AbiSession : public CommandSink {
 public:
  explicit AbiSession(const AbiFunctions& api) : api_(&api) {}
  ~AbiSession() override { close(); }
  AbiSession(const AbiSession&) = delete;
  AbiSession& operator=(const AbiSession&) = delete;

  // Opens the calling game process; its commands go to the in-game ring and,
  // submitted inside a frame callback, run at that frame boundary.
  bool open_self(std::uint64_t sources, std::string& error) {
    close();
    if (api_->open_self(&session_) != BWR_OK) {
      const char* message = api_->last_error(nullptr);
      error = std::string("bwr_open_self failed: ") + (message != nullptr ? message : "");
      session_ = nullptr;
      return false;
    }
    api_->subscribe(session_, sources);
    return true;
  }
  void close() {
    if (session_ != nullptr) api_->close(session_);
    session_ = nullptr;
  }

  // Waits until the game frame passes the last acquired one.
  BwrStatus step(DWORD timeout_ms) { return api_->step(session_, timeout_ms); }
  BwrStatus acquire(AbiFrame& frame) {
    const BwrFrame* raw = nullptr;
    const BwrStatus status = api_->acquire(session_, &raw);
    frame = status == BWR_OK ? AbiFrame(api_, session_, raw) : AbiFrame();
    return status;
  }
  BwrStatus set_frame_callback(void (*callback)(void*), void* user) {
    return api_->set_frame_callback(session_, callback, user);
  }
  BwrStatus capture_chat(bool enable) { return api_->capture_chat(session_, enable ? 1 : 0); }
  BwrStatus next_chat(BwrChat& line) { return api_->next_chat(session_, &line); }
  // Draws `list` into every screen update until the next call; null clears.
  BwrStatus draw(const BwrDrawList* list) { return api_->draw(session_, list); }
  std::string last_error() const {
    const char* message = api_->last_error(session_);
    return message != nullptr ? message : "";
  }

  // The sink for unit commands.
  CommandSink* sink() { return this; }
  BwrStatus submit(const BwrCommand* commands, std::uint32_t count, std::uint64_t* ids) override {
    return api_->submit(session_, commands, count, ids);
  }
  BwrStatus poll(std::uint64_t id, BwrResult* result) override {
    return api_->poll_result(session_, id, result);
  }

 private:
  const AbiFunctions* api_;
  BwrSession* session_ = nullptr;
};

}  // namespace BWRAPI::Legacy44
