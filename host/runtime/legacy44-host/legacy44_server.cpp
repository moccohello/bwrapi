#include "legacy44_server.h"

#include "legacy44_bwapi_rules.h"

#include <cstring>

namespace BWRAPI::Legacy44 {

BwapiServer::BwapiServer(DWORD game_pid, StatusWriter& status, DWORD io_timeout_ms)
    : game_pid_(game_pid), status_(status), io_timeout_ms_(io_timeout_ms) {}

bool BwapiServer::open(std::string& error) {
  if (!table_.open(error)) {
    error = "official GameTable setup failed: " + error;
    return false;
  }
  if (!transport_.open(table_, error)) {
    error = "official GameData setup failed: " + error;
    table_.release();
    return false;
  }
  transport_.data()->instanceID = table_.index();
  return true;
}

void BwapiServer::close() {
  transport_.close();
  table_.release();
}

void BwapiServer::reset_client() {
  events_.reset();
  projection_.reset();
  latcom_.reset();
  last_frame_ = {};
  current_ = {};
  have_frame_ = false;
  menu_frame_due_ = true;
  running_ = false;
  async_ = false;
  awaiting_ = false;
  save_known_ = false;
  if (draw_) draw_(nullptr);
  status_.write("AwaitingClient", false,
                "BWAPI::Client disconnected; waiting for reconnect", 0, {}, game_pid_);
}

void BwapiServer::drop_client() {
  transport_.disconnect_client();
  reset_client();
}

AcceptResult BwapiServer::accept(DWORD timeout_ms, CommandSink* sink, std::string& error) {
  BWAPI::GameData& data = *transport_.data();
  // Client::connect constructs a fresh GameImpl without processing events.
  // Keep it in the menu until its first update receives a MatchStart and the
  // current snapshot.
  data.isInGame = false;
  data.eventCount = 0;
  data.commandCount = 0;
  data.unitCommandCount = 0;
  data.shapeCount = 0;
  data.stringCount = 0;
  const AcceptResult accepted = transport_.accept(timeout_ms, error);
  if (accepted != AcceptResult::Connected) return accepted;
  // A reconnected client starts without callback history.
  events_.reset();
  projection_.reset();
  latcom_.reset();
  have_frame_ = false;
  menu_frame_due_ = true;
  async_ = false;
  if (!round(sink, error)) return AcceptResult::Failed;
  if (!transport_.connected()) return AcceptResult::Timeout;
  async_ = next_async_;
  status_.write("Connected", true,
                connected_once_ ? "BWAPI::Client reconnect handshake complete"
                                : "official GameTable/GameData/pipe handshake complete",
                0, {}, game_pid_);
  connected_once_ = true;
  return AcceptResult::Connected;
}

// Sends the GameData as it stands. False on failure; a client that went away
// is a disconnect (`alive` false), not a failure.
bool BwapiServer::deliver_ready(bool& alive, std::string& error) {
  alive = true;
  const ClientSendResult result = transport_.send_ready(io_timeout_ms_, error);
  if (result == ClientSendResult::Disconnected) {
    alive = false;
    reset_client();
    return true;
  }
  return result == ClientSendResult::Ready;
}

CommandTarget BwapiServer::command_target(CommandSink* sink) {
  return {sink, [this](int id) { return projection_.engine_unit_id(id); },
          [this](int id) { return projection_.player_slot(id); }};
}

void BwapiServer::queue_chat(bool sent, std::uint32_t player, const std::string& text) {
  // The client may be writing GameData now, so the command runs once its
  // update is in, appended to its commands.
  if (sent && ChatCommands::recognizes(text)) {
    chat_command_lines_.push_back(text);
    return;
  }
  events_.queue_chat(sent, player, text);
}

// Receives the client's update for the last ready and drains the commands it
// wrote. The client is waited for without limit, as the official server waits
// for its module.
bool BwapiServer::receive_completion(CommandSink* sink, bool& alive, std::string& error) {
  alive = true;
  int code = 0;
  const ClientReceiveResult result = transport_.receive_update(INFINITE, code, error);
  if (result == ClientReceiveResult::Disconnected) {
    alive = false;
    reset_client();
    return true;
  }
  if (result == ClientReceiveResult::Failed) return false;
  if (code != 1) {
    error = "unexpected BWAPI protocol code: " + std::to_string(code);
    return false;
  }
  for (const std::string& line : chat_command_lines_) chat_commands_.parse(line, *transport_.data());
  chat_command_lines_.clear();
  if (draw_) {
    // GameImpl::updateOverlays adds the /grid and /fps shapes beside the
    // client's before the frame is drawn.
    chat_commands_.append_overlay_shapes(*transport_.data());
    draw_(transport_.data());
  }
  commands_.drain(*transport_.data(), have_frame_ ? last_frame_.frame : 0,
                  have_frame_ && last_frame_.game_ready, command_target(sink));
  // The commands the game took stay applied until it acts on them.
  latcom_.record(commands_.submitted_unit_commands(), *transport_.data());
  // The commands handed to the game count toward the bot's APM.
  const auto& actions = commands_.last_actions();
  apm_.add(actions.selections, actions.commands);
  return true;
}

// One ready/update round for the staged events.
bool BwapiServer::round(CommandSink* sink, std::string& error) {
  bool alive = true;
  if (!deliver_ready(alive, error)) return false;
  if (!alive) return true;
  if (async_) {
    awaiting_ = true;
    return true;
  }
  return receive_completion(sink, alive, error);
}

bool BwapiServer::collect(CommandSink* sink, bool wait, bool& busy, std::string& error) {
  busy = false;
  if (!awaiting_) return true;
  if (!wait && !transport_.update_waiting()) {
    busy = true;
    return true;
  }
  awaiting_ = false;
  bool alive = true;
  return receive_completion(sink, alive, error);
}

// The old match ends with its own callbacks: MatchFrame and MatchEnd on the
// old GameData, then a MenuFrame. Both rounds run before any new match.
bool BwapiServer::end_match(CommandSink* sink, std::string& error) {
  if (!events_.match_active()) return true;
  bool busy = false;
  if (!collect(sink, true, busy, error)) return false;
  if (!transport_.connected()) return true;
  BWAPI::GameData& data = *transport_.data();
  if (!events_.stage_match_end(data, error)) return false;
  if (!round(sink, error)) return false;
  if (!collect(sink, true, busy, error)) return false;
  if (!transport_.connected()) return true;
  projection_.leave_match(data);
  bool advanced = false;
  if (!events_.advance_pending_lifecycle(data, advanced, error)) return false;
  if (!round(sink, error)) return false;
  latcom_.reset();
  last_frame_ = {};
  have_frame_ = false;
  menu_frame_due_ = false;
  running_ = false;
  return true;
}

bool BwapiServer::frame(const BwrFrame& frame, CommandSink* sink, std::string& error) {
  // Commands answered for an earlier match name its units, so they are not
  // executed in this one.
  const bool other_match = events_.match_active() && frame.match_id != events_.match_id();
  bool busy = false;
  if (!collect(other_match ? nullptr : sink, other_match, busy, error)) return false;
  if (busy || !transport_.connected()) return true;
  if (events_.match_active() && frame.match_id != events_.match_id()) {
    if (!end_match(sink, error)) {
      error = "event lifecycle adapter failed: " + error;
      return false;
    }
    if (!transport_.connected()) return true;
  }
  // The frame that stages the first MatchStart of a match (or of a
  // reconnected client) is the frame its onStart runs on.
  const bool match_start = !events_.match_active();
  BWAPI::GameData& data = *transport_.data();
  if (!projection_.project(frame, data, current_, error)) {
    error = "GameData projection failed: " + error;
    return false;
  }
  // GameImpl::updateUnits applies latency compensation right after the unit
  // data is complete.
  if (match_start) latcom_.reset();
  latcom_.apply(data, data.frameCount);
  if (input_) input_(data);
  data.eventCount = 0;
  observe_save(frame);
  if (!events_.append(current_, frame, data, error)) {
    error = "event-frame adapter failed: " + error;
    return false;
  }
  if (match_start) {
    commands_.start_match();
    apm_.reset();
    // GameImpl::initializeData turns the GUI on for each match, and the game
    // installs its own screen update cap when the match's game loop starts.
    data.hasGUI = true;
  }
  publish_apm(data);
  if (!running_) {
    status_.write("Running", true,
                  "validated projection delivered; unavailable_callbacks=none; event_source=frame-diff",
                  current_.frame, {}, game_pid_);
    running_ = true;
    heartbeat_frame_ = current_.frame;
  } else if (current_.frame - heartbeat_frame_ >= 240 && current_.frame >= heartbeat_frame_) {
    // A heartbeat of the delivered frame, about every ten seconds of game time.
    status_.write("Running", true, "frame delivered", current_.frame, {}, game_pid_);
    heartbeat_frame_ = current_.frame;
  }
  std::swap(last_frame_, current_);
  have_frame_ = true;
  menu_frame_due_ = false;
  return round(sink, error);
}

void BwapiServer::observe_save(const BwrFrame& frame) {
  if (frame.game == nullptr || frame.game->rows == 0 || !known(frame.game->save_tick[0])) return;
  const std::uint32_t tick = frame.game->save_tick[0];
  if (save_known_ && tick != save_tick_) {
    const BwrText32& name = frame.game->save_name[0];
    events_.queue_save(std::string(name.text, strnlen(name.text, sizeof(name.text))));
  }
  save_known_ = true;
  save_tick_ = tick;
}

bool BwapiServer::paused_frame(const BwrFrame& frame, CommandSink* sink, std::string& error) {
  bool busy = false;
  if (!collect(sink, false, busy, error)) return false;
  if (busy || !transport_.connected()) return true;
  BWAPI::GameData& data = *transport_.data();
  // The last frame the client saw may predate the pause. Input stays as the
  // window thread last read it: this thread has no key state of its own.
  data.isPaused = true;
  projection_.project_latency(frame, data);
  observe_save(frame);
  if (!events_.stage_match_frame(data, error)) return false;
  projection_.record_match_frame(data, data.frameCount);
  publish_apm(data);
  return round(sink, error);
}

void BwapiServer::publish_apm(BWAPI::GameData& data) {
  apm_.update(data.frameCount);
  data.botAPM_noselects = apm_.apm(false);
  data.botAPM_selects = apm_.apm(true);
}

bool BwapiServer::menu(CommandSink* sink, bool& idle, std::string& error) {
  idle = false;
  if (events_.match_active()) {
    if (end_match(sink, error)) return true;
    error = "event lifecycle adapter failed: " + error;
    return false;
  }
  bool busy = false;
  if (!collect(sink, false, busy, error)) return false;
  if (!menu_frame_due_ || busy || !transport_.connected()) {
    // No round runs here, so a client that exited would otherwise hold the
    // server until the next match; the client after it would miss its start.
    if (transport_.client_gone()) drop_client();
    idle = true;
    return true;
  }
  BWAPI::GameData& data = *transport_.data();
  projection_.leave_match(data);
  if (!events_.stage_menu_frame(data, error)) return false;
  menu_frame_due_ = false;
  return round(sink, error);
}

}  // namespace BWRAPI::Legacy44
