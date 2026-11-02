#pragma once

#include "legacy44_apm.h"
#include "legacy44_chat.h"
#include "legacy44_command.h"
#include "legacy44_common.h"
#include "legacy44_events.h"
#include "legacy44_latcom.h"
#include "legacy44_projection.h"
#include "legacy44_transport.h"

#include <cstdint>
#include <functional>
#include <string>

namespace BWRAPI::Legacy44 {

// The BWAPI 4.4 server: the official GameTable row, GameData mapping and
// pipe, and the rounds that hand GameData to the client and take back its
// commands. A synchronous round sends one ready and waits for the client's
// update, as the original in-game server does, so the caller of a round waits
// for the client. An asynchronous round only sends the ready: frames that come
// while the client works pass without a round, and the first frame after its
// update executes its commands and hands it that frame.
//
// The in-game server DLL drives match frames from the game thread's frame
// callback and everything else from its own thread. Calls must not overlap;
// the driver serializes them. Any BWAPI client may connect, leave and
// reconnect.
class BwapiServer {
 public:
  // `game_pid` is the game process, which status lines name.
  BwapiServer(DWORD game_pid, StatusWriter& status, DWORD io_timeout_ms);
  BwapiServer(const BwapiServer&) = delete;
  BwapiServer& operator=(const BwapiServer&) = delete;

  bool open(std::string& error);
  // The pipe and mapping go before the GameTable row they are listed under.
  void close();

  // Waits up to `timeout_ms` for a client and runs its first round, which
  // waits for the client in both modes.
  AcceptResult accept(DWORD timeout_ms, CommandSink* sink, std::string& error);
  // The mode of the next accepted client.
  void set_async(bool async) { next_async_ = async; }
  // An asynchronous client still works on the last ready.
  bool client_busy() const { return awaiting_ && !transport_.update_waiting(); }
  bool connected() const { return transport_.connected(); }
  void keep_alive() { transport_.keep_alive(); }
  // Fills the user input fields of each frame handed to the client; only a
  // server on the game's window thread can read them.
  void set_input(std::function<void(BWAPI::GameData&)> input) { input_ = std::move(input); }
  // Receives each client update's GameData before its shapes are cleared,
  // and null when the client leaves; only a server in the game can show them.
  void set_draw(std::function<void(const BWAPI::GameData*)> draw) { draw_ = std::move(draw); }
  // Disconnects the client after a failure; it reconnects to a fresh state.
  void drop_client();

  // One game frame of a match, then its round; commands go to `sink`. A
  // frame of another match ends the previous one first.
  bool frame(const BwrFrame& frame, CommandSink* sink, std::string& error);
  // BWAPI calls onFrame on every loop of a paused game: an extra MatchFrame.
  // `frame` is the paused game's game row.
  bool paused_frame(const BwrFrame& frame, CommandSink* sink, std::string& error);
  // The game is outside a match: ends the match that ran, or tells the client
  // once that it is in the menu. `idle` is set when neither was due.
  bool menu(CommandSink* sink, bool& idle, std::string& error);

  // A chat line the game took. GameImpl::onSendText runs parseText first: a
  // line the official server would run itself waits for the client's next
  // update and runs with its commands, never reaching the client; the rest
  // become the events of the next round. `player` is the game player number
  // a received line came from.
  void queue_chat(bool sent, std::uint32_t player, const std::string& text);

  bool match_active() const { return events_.match_active(); }

 private:
  void reset_client();
  bool deliver_ready(bool& alive, std::string& error);
  bool receive_completion(CommandSink* sink, bool& alive, std::string& error);
  bool round(CommandSink* sink, std::string& error);
  // Takes the update of an asynchronous round still out, waiting for it when
  // `wait`; `busy` is set when it has not come and was not waited for.
  bool collect(CommandSink* sink, bool wait, bool& busy, std::string& error);
  bool end_match(CommandSink* sink, std::string& error);
  // GameImpl::updateStatistics' APM half for the GameData about to be sent.
  void publish_apm(BWAPI::GameData& data);
  // Queues SaveGame when the game row names a save newer than the last one
  // seen since the client connected.
  void observe_save(const BwrFrame& frame);
  // The target a round's or a chat command's drain takes.
  CommandTarget command_target(CommandSink* sink);

  DWORD game_pid_;
  StatusWriter& status_;
  DWORD io_timeout_ms_;
  // The transport refers to the row, so it is declared after it and goes first.
  GameTableLease table_;
  GameDataTransport transport_;
  GameDataProjection projection_;
  EventFrameAdapter events_;
  CommandAdapter commands_;
  ChatCommands chat_commands_;
  // Chat command lines waiting for the client's next update.
  std::vector<std::string> chat_command_lines_;
  LatencyCompensation latcom_;
  ApmCounter apm_;
  std::function<void(BWAPI::GameData&)> input_;
  std::function<void(const BWAPI::GameData*)> draw_;
  // The two samples swap each frame, so a frame costs no allocation.
  FrameSample current_;
  FrameSample last_frame_;
  bool have_frame_ = false;
  // The client has not been told yet that it is in the menu.
  bool menu_frame_due_ = true;
  bool running_ = false;
  bool connected_once_ = false;
  bool next_async_ = false;
  bool async_ = false;
  // An asynchronous ready whose update has not been received.
  bool awaiting_ = false;
  std::uint32_t heartbeat_frame_ = 0;
  // Tick count of the last save seen; none yet after a connect.
  bool save_known_ = false;
  std::uint32_t save_tick_ = 0;
};

}  // namespace BWRAPI::Legacy44
