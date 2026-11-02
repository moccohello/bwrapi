#include "legacy44_transport.h"

#include <BWAPI/Client/GameTable.h>

#include <cstring>
#include <sstream>

namespace BWRAPI::Legacy44 {

namespace {

class ScopedEvent {
 public:
  ScopedEvent() : handle_(CreateEventA(nullptr, TRUE, FALSE, nullptr)) {}
  ~ScopedEvent() {
    if (handle_ != nullptr) CloseHandle(handle_);
  }

  ScopedEvent(const ScopedEvent&) = delete;
  ScopedEvent& operator=(const ScopedEvent&) = delete;

  HANDLE get() const { return handle_; }

 private:
  HANDLE handle_ = nullptr;
};

void cancel_and_drain(HANDLE pipe, OVERLAPPED& operation) {
  CancelIoEx(pipe, &operation);
  DWORD ignored = 0;
  GetOverlappedResult(pipe, &operation, &ignored, TRUE);
}

enum class ProcessState { dead, alive, unknown };

// Whether the server process that owns a GameTable row still runs.
ProcessState process_state(DWORD pid) {
  if (pid == 0) return ProcessState::dead;
  HANDLE process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | SYNCHRONIZE, FALSE, pid);
  if (process == nullptr)
    return GetLastError() == ERROR_INVALID_PARAMETER ? ProcessState::dead : ProcessState::unknown;
  const DWORD wait = WaitForSingleObject(process, 0);
  CloseHandle(process);
  if (wait == WAIT_OBJECT_0) return ProcessState::dead;
  return wait == WAIT_TIMEOUT ? ProcessState::alive : ProcessState::unknown;
}

// Waits up to 10 ms (less near a finite deadline) on `event`.
DWORD wait_step(HANDLE event, bool finite_timeout, ULONGLONG deadline) {
  DWORD wait_ms = 10;
  if (finite_timeout) {
    const ULONGLONG now = GetTickCount64();
    wait_ms = now >= deadline ? 0 : static_cast<DWORD>(std::min<ULONGLONG>(10, deadline - now));
  }
  return WaitForSingleObject(event, wait_ms);
}

}  // namespace

GameTableLease::~GameTableLease() { release(); }

bool GameTableLease::valid() const {
  return table_ != nullptr && table_index_ >= 0 && owns_row_ &&
         table_->gameInstances[table_index_].serverProcessID == owner_pid_;
}

void GameTableLease::set_connected(bool connected) {
  if (valid()) table_->gameInstances[table_index_].isConnected = connected;
}

void GameTableLease::keep_alive() {
  if (valid()) table_->gameInstances[table_index_].lastKeepAliveTime = GetTickCount();
}

bool GameTableLease::open(std::string& error) {
  release();
  owner_pid_ = GetCurrentProcessId();
  table_file_ = CreateFileMappingA(INVALID_HANDLE_VALUE, nullptr, PAGE_READWRITE, 0,
                                   static_cast<DWORD>(sizeof(BWAPI::GameTable)),
                                   "Local\\bwapi_shared_memory_game_list");
  if (table_file_ == nullptr) {
    error = win_error("CreateFileMapping GameTable");
    return false;
  }
  const DWORD table_error = GetLastError();
  table_ = static_cast<BWAPI::GameTable*>(MapViewOfFile(
      table_file_, FILE_MAP_READ | FILE_MAP_WRITE, 0, 0, sizeof(BWAPI::GameTable)));
  if (table_ == nullptr) {
    error = win_error("MapViewOfFile GameTable");
    release();
    return false;
  }
  if (table_error != ERROR_ALREADY_EXISTS) std::memset(table_, 0, sizeof(BWAPI::GameTable));

  // Official Client::connect selects the oldest unconnected row. A row with
  // an unobservable owner is not treated as stale, because doing so could make
  // a client attach to a different server than the one advertised by us.
  for (int index = 0; index < BWAPI::GameTable::MAX_GAME_INSTANCES; ++index) {
    const DWORD server_pid = table_->gameInstances[index].serverProcessID;
    if (server_pid == 0 || server_pid == owner_pid_) continue;
    const ProcessState state = process_state(server_pid);
    if (state == ProcessState::dead) {
      table_->gameInstances[index] = BWAPI::GameInstance();
      continue;
    }
    if (!table_->gameInstances[index].isConnected) {
      error = std::string(state == ProcessState::alive
                              ? "another BWAPI server is active"
                              : "another BWAPI server row cannot be validated") +
              "; exact client target is ambiguous (pid=" + std::to_string(server_pid) + ")";
      release();
      return false;
    }
  }

  for (int index = 0; index < BWAPI::GameTable::MAX_GAME_INSTANCES; ++index) {
    if (table_->gameInstances[index].serverProcessID == owner_pid_) {
      table_index_ = index;
      break;
    }
  }
  if (table_index_ < 0) {
    for (int index = 0; index < BWAPI::GameTable::MAX_GAME_INSTANCES; ++index) {
      if (table_->gameInstances[index].serverProcessID == 0) {
        table_index_ = index;
        break;
      }
    }
  }
  if (table_index_ < 0) {
    // Every row belongs to a live, connected server; a connected row is never
    // taken over.
    error = "official GameTable has no free instance row";
    release();
    return false;
  }
  table_->gameInstances[table_index_].serverProcessID = owner_pid_;
  table_->gameInstances[table_index_].isConnected = false;
  table_->gameInstances[table_index_].lastKeepAliveTime = GetTickCount();
  owns_row_ = true;
  return true;
}

void GameTableLease::release() {
  if (table_ != nullptr && table_index_ >= 0 && owns_row_ &&
      table_->gameInstances[table_index_].serverProcessID == owner_pid_) {
    table_->gameInstances[table_index_] = BWAPI::GameInstance();
  }
  owns_row_ = false;
  table_index_ = -1;
  if (table_ != nullptr) UnmapViewOfFile(table_);
  if (table_file_ != nullptr && table_file_ != INVALID_HANDLE_VALUE)
    CloseHandle(table_file_);
  table_ = nullptr;
  table_file_ = nullptr;
  owner_pid_ = 0;
}

GameDataTransport::~GameDataTransport() { close(); }

bool GameDataTransport::open(GameTableLease& table_lease, std::string& error) {
  close();
  if (!table_lease.valid()) {
    error = "GameTable lease is not active";
    return false;
  }
  table_lease_ = &table_lease;
  std::ostringstream mapping_name;
  mapping_name << "Local\\bwapi_shared_memory_" << GetCurrentProcessId();
  data_file_ = CreateFileMappingA(INVALID_HANDLE_VALUE, nullptr, PAGE_READWRITE, 0,
                                  static_cast<DWORD>(sizeof(BWAPI::GameData)),
                                  mapping_name.str().c_str());
  if (data_file_ == nullptr) {
    error = win_error("CreateFileMapping GameData");
    close();
    return false;
  }
  // Always initialize the mapping, including ERROR_ALREADY_EXISTS, so a
  // recycled PID cannot expose old data.
  data_ = static_cast<BWAPI::GameData*>(MapViewOfFile(
      data_file_, FILE_MAP_READ | FILE_MAP_WRITE, 0, 0, sizeof(BWAPI::GameData)));
  if (data_ == nullptr) {
    error = win_error("MapViewOfFile GameData");
    close();
    return false;
  }
  std::memset(data_, 0, sizeof(BWAPI::GameData));
  data_->client_version = static_cast<int>(kClientVersion);
  data_->hasLatCom = true;
  data_->hasGUI = true;

  io_event_ = CreateEventA(nullptr, TRUE, FALSE, nullptr);
  if (io_event_ == nullptr) {
    error = win_error("CreateEvent protocol I/O");
    close();
    return false;
  }
  if (!create_pipe(error)) {
    close();
    return false;
  }
  return true;
}

bool GameDataTransport::create_pipe(std::string& error) {
  if (pipe_ != nullptr) return true;
  std::ostringstream pipe_name;
  pipe_name << "\\\\.\\pipe\\bwapi_pipe_" << GetCurrentProcessId();
  pipe_ = CreateNamedPipeA(pipe_name.str().c_str(),
                           PIPE_ACCESS_DUPLEX | FILE_FLAG_OVERLAPPED,
                           PIPE_TYPE_MESSAGE | PIPE_READMODE_MESSAGE | PIPE_NOWAIT,
                           1, 4096, 4096, 0, nullptr);
  if (pipe_ == INVALID_HANDLE_VALUE) {
    error = win_error("CreateNamedPipe");
    pipe_ = nullptr;
    return false;
  }
  return true;
}

AcceptResult GameDataTransport::accept(DWORD timeout_ms, std::string& error) {
  if (table_lease_ == nullptr || !table_lease_->valid() || data_ == nullptr) {
    error = "GameData transport is not open";
    return AcceptResult::Failed;
  }
  if (connected_) {
    error = "BWAPI::Client is already connected";
    return AcceptResult::Failed;
  }
  if (!create_pipe(error)) return AcceptResult::Failed;
  ScopedEvent connect_event;
  if (connect_event.get() == nullptr) {
    error = win_error("CreateEvent ConnectNamedPipe");
    return AcceptResult::Failed;
  }
  const bool finite_timeout = timeout_ms != INFINITE;
  const ULONGLONG deadline = finite_timeout ? GetTickCount64() + timeout_ms : 0;
  for (;;) {
    table_lease_->keep_alive();
    ResetEvent(connect_event.get());
    OVERLAPPED connect_operation{};
    connect_operation.hEvent = connect_event.get();
    const BOOL connected_now = ConnectNamedPipe(pipe_, &connect_operation);
    DWORD connect_error = connected_now ? ERROR_SUCCESS : GetLastError();
    bool connected = connected_now || connect_error == ERROR_PIPE_CONNECTED;
    if (!connected && connect_error == ERROR_IO_PENDING) {
      for (;;) {
        const DWORD wait_result = wait_step(connect_event.get(), finite_timeout, deadline);
        if (wait_result == WAIT_OBJECT_0) {
          DWORD ignored = 0;
          if (GetOverlappedResult(pipe_, &connect_operation, &ignored, FALSE)) {
            connected = true;
            connect_error = ERROR_SUCCESS;
          } else {
            connect_error = GetLastError();
            connected = connect_error == ERROR_PIPE_CONNECTED;
          }
          break;
        }
        if (wait_result == WAIT_FAILED) {
          const DWORD wait_error = GetLastError();
          cancel_and_drain(pipe_, connect_operation);
          error = "WaitForSingleObject ConnectNamedPipe failed (" +
                  std::to_string(wait_error) + ")";
          return AcceptResult::Failed;
        }
        table_lease_->keep_alive();
        if (finite_timeout && GetTickCount64() >= deadline) {
          cancel_and_drain(pipe_, connect_operation);
          error = "timeout waiting for BWAPI::Client pipe connection";
          return AcceptResult::Timeout;
        }
      }
    }
    if (connected) {
      DWORD mode = PIPE_READMODE_MESSAGE | PIPE_WAIT;
      if (!SetNamedPipeHandleState(pipe_, &mode, nullptr, nullptr)) {
        error = win_error("SetNamedPipeHandleState");
        disconnect_client();
        return AcceptResult::Failed;
      }
      data_->stringCount = 0;
      data_->shapeCount = 0;
      data_->commandCount = 0;
      data_->unitCommandCount = 0;
      table_lease_->set_connected(true);
      connected_ = true;
      return AcceptResult::Connected;
    }
    if (connect_error != ERROR_PIPE_LISTENING && connect_error != ERROR_NO_DATA) {
      error = "ConnectNamedPipe failed (" + std::to_string(connect_error) + ")";
      return AcceptResult::Failed;
    }
    // A client that connected and closed before the accept leaves the pipe
    // unusable until it is disconnected; a fresh pipe listens again.
    if (connect_error == ERROR_NO_DATA) {
      disconnect_client();
      if (!create_pipe(error)) return AcceptResult::Failed;
    }
    if (finite_timeout && GetTickCount64() >= deadline) {
      error = "timeout waiting for BWAPI::Client pipe connection";
      return AcceptResult::Timeout;
    }
    Sleep(10);
  }
}

bool GameDataTransport::update_waiting() const {
  if (pipe_ == nullptr || !connected_) return false;
  DWORD available = 0;
  // A pipe that cannot be peeked is broken; the read that follows reports it.
  if (!PeekNamedPipe(pipe_, nullptr, 0, nullptr, &available, nullptr)) return true;
  return available >= sizeof(int);
}

bool GameDataTransport::client_gone() const {
  DWORD available = 0;
  return pipe_ != nullptr && connected_ && !PeekNamedPipe(pipe_, nullptr, 0, nullptr, &available, nullptr);
}

ClientReceiveResult GameDataTransport::receive_update(DWORD timeout_ms, int& code,
                                                      std::string& error) {
  if (pipe_ == nullptr || !connected_) {
    error = "BWAPI client pipe is not connected";
    return ClientReceiveResult::Failed;
  }
  const bool finite_timeout = timeout_ms != INFINITE;
  const ULONGLONG deadline = finite_timeout ? GetTickCount64() + timeout_ms : 0;
  ResetEvent(io_event_);
  DWORD read = 0;
  int candidate = 0;
  OVERLAPPED read_operation{};
  read_operation.hEvent = io_event_;
  const BOOL read_now = ReadFile(pipe_, &candidate, sizeof(candidate), &read, &read_operation);
  DWORD last_error = read_now ? ERROR_SUCCESS : GetLastError();
  bool read_complete = false;
  if (read_now) {
    read_complete = GetOverlappedResult(pipe_, &read_operation, &read, FALSE);
    if (!read_complete) last_error = GetLastError();
  } else if (last_error == ERROR_IO_PENDING) {
    for (;;) {
      const DWORD wait_result = wait_step(io_event_, finite_timeout, deadline);
      if (wait_result == WAIT_OBJECT_0) {
        read_complete = GetOverlappedResult(pipe_, &read_operation, &read, FALSE);
        if (!read_complete) last_error = GetLastError();
        break;
      }
      if (wait_result == WAIT_FAILED) {
        const DWORD wait_error = GetLastError();
        cancel_and_drain(pipe_, read_operation);
        error = "WaitForSingleObject protocol read failed (" + std::to_string(wait_error) + ")";
        return ClientReceiveResult::Failed;
      }
      if (finite_timeout && GetTickCount64() >= deadline) {
        cancel_and_drain(pipe_, read_operation);
        error = "timeout waiting for BWAPI::Client update";
        return ClientReceiveResult::Failed;
      }
    }
  }
  if (last_error == ERROR_BROKEN_PIPE || last_error == ERROR_PIPE_NOT_CONNECTED) {
    error = "BWAPI::Client disconnected";
    disconnect_client();
    return ClientReceiveResult::Disconnected;
  }
  if (!read_complete) {
    error = "ReadFile protocol code failed (" + std::to_string(last_error) + ")";
    return ClientReceiveResult::Failed;
  }
  if (read != sizeof(candidate)) {
    error = "named pipe returned a partial protocol code";
    return ClientReceiveResult::Failed;
  }
  code = candidate;
  return ClientReceiveResult::Update;
}

ClientSendResult GameDataTransport::send_ready(DWORD timeout_ms, std::string& error) {
  int code = 2;
  if (pipe_ == nullptr || !connected_) {
    error = "BWAPI client pipe is not connected";
    return ClientSendResult::Failed;
  }
  ResetEvent(io_event_);
  DWORD written = 0;
  OVERLAPPED write_operation{};
  write_operation.hEvent = io_event_;
  BOOL write_ok = WriteFile(pipe_, &code, sizeof(code), &written, &write_operation);
  DWORD write_error = write_ok ? ERROR_SUCCESS : GetLastError();
  if (!write_ok && write_error == ERROR_IO_PENDING) {
    const bool finite_timeout = timeout_ms != INFINITE;
    const ULONGLONG deadline = finite_timeout ? GetTickCount64() + timeout_ms : 0;
    for (;;) {
      const DWORD wait_result = wait_step(io_event_, finite_timeout, deadline);
      if (wait_result == WAIT_OBJECT_0) {
        write_ok = GetOverlappedResult(pipe_, &write_operation, &written, FALSE);
        if (!write_ok) write_error = GetLastError();
        break;
      }
      if (wait_result == WAIT_FAILED) {
        write_error = GetLastError();
        cancel_and_drain(pipe_, write_operation);
        break;
      }
      if (finite_timeout && GetTickCount64() >= deadline) {
        cancel_and_drain(pipe_, write_operation);
        error = "timeout waiting for BWAPI::Client ready delivery";
        return ClientSendResult::Failed;
      }
    }
  } else if (write_ok) {
    write_ok = GetOverlappedResult(pipe_, &write_operation, &written, FALSE);
    if (!write_ok) write_error = GetLastError();
  }
  if (write_ok && written != sizeof(code)) write_error = ERROR_WRITE_FAULT;
  if (!write_ok || written != sizeof(code)) {
    error = "WriteFile protocol code failed (" + std::to_string(write_error) + ")";
    if (write_error == ERROR_BROKEN_PIPE || write_error == ERROR_PIPE_NOT_CONNECTED ||
        write_error == ERROR_NO_DATA) {
      disconnect_client();
      return ClientSendResult::Disconnected;
    }
    return ClientSendResult::Failed;
  }
  return ClientSendResult::Ready;
}

void GameDataTransport::keep_alive() {
  if (table_lease_ != nullptr) table_lease_->keep_alive();
}

void GameDataTransport::disconnect_client() {
  if (table_lease_ != nullptr) table_lease_->set_connected(false);
  if (pipe_ != nullptr && pipe_ != INVALID_HANDLE_VALUE) {
    DisconnectNamedPipe(pipe_);
    CloseHandle(pipe_);
  }
  pipe_ = nullptr;
  connected_ = false;
}

void GameDataTransport::close() {
  disconnect_client();
  if (data_ != nullptr) UnmapViewOfFile(data_);
  if (data_file_ != nullptr && data_file_ != INVALID_HANDLE_VALUE)
    CloseHandle(data_file_);
  data_ = nullptr;
  data_file_ = nullptr;
  if (io_event_ != nullptr) CloseHandle(io_event_);
  io_event_ = nullptr;
  table_lease_ = nullptr;
}

}  // namespace BWRAPI::Legacy44
