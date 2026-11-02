#pragma once

#include "legacy44_common.h"

namespace BWAPI {
struct GameTable;
}

namespace BWRAPI::Legacy44 {

enum class ClientReceiveResult { Update, Disconnected, Failed };
enum class ClientSendResult { Ready, Disconnected, Failed };
// Timeout: no client connected within the wait.
enum class AcceptResult { Connected, Timeout, Failed };

class GameTableLease {
 public:
  GameTableLease() = default;
  ~GameTableLease();
  GameTableLease(const GameTableLease&) = delete;
  GameTableLease& operator=(const GameTableLease&) = delete;

  bool open(std::string& error);
  void release();
  bool valid() const;
  int index() const { return table_index_; }
  void set_connected(bool connected);
  void keep_alive();

 private:
  HANDLE table_file_ = nullptr;
  BWAPI::GameTable* table_ = nullptr;
  int table_index_ = -1;
  DWORD owner_pid_ = 0;
  bool owns_row_ = false;
};

// The official GameData mapping and pipe of this process, which any BWAPI
// client may connect to, leave and reconnect to.
class GameDataTransport {
 public:
  GameDataTransport() = default;
  ~GameDataTransport();
  GameDataTransport(const GameDataTransport&) = delete;
  GameDataTransport& operator=(const GameDataTransport&) = delete;

  bool open(GameTableLease& table_lease, std::string& error);
  AcceptResult accept(DWORD timeout_ms, std::string& error);
  ClientReceiveResult receive_update(DWORD timeout_ms, int& code, std::string& error);
  // The client's update is in the pipe (or the pipe broke), so receive_update
  // returns without waiting.
  bool update_waiting() const;
  // The connected client's end of the pipe closed.
  bool client_gone() const;
  ClientSendResult send_ready(DWORD timeout_ms, std::string& error);
  void keep_alive();
  void disconnect_client();
  void close();

  BWAPI::GameData* data() const { return data_; }
  bool connected() const { return connected_; }

 private:
  bool create_pipe(std::string& error);

  GameTableLease* table_lease_ = nullptr;
  HANDLE data_file_ = nullptr;
  HANDLE pipe_ = nullptr;
  // Event of the protocol reads and writes, which never overlap; one per
  // transport so a round creates no kernel object.
  HANDLE io_event_ = nullptr;
  BWAPI::GameData* data_ = nullptr;
  bool connected_ = false;
};

}  // namespace BWRAPI::Legacy44
