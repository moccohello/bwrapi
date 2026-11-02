#include "legacy44_common.h"

#include <cstdio>
#include <sstream>
#include <stdexcept>

namespace BWRAPI::Legacy44 {

namespace {

std::string json_escape(const std::string& value) {
  std::string escaped;
  escaped.reserve(value.size() + 8);
  for (unsigned char c : value) {
    switch (c) {
      case '"': escaped += "\\\""; break;
      case '\\': escaped += "\\\\"; break;
      case '\b': escaped += "\\b"; break;
      case '\f': escaped += "\\f"; break;
      case '\n': escaped += "\\n"; break;
      case '\r': escaped += "\\r"; break;
      case '\t': escaped += "\\t"; break;
      default:
        if (c < 0x20) {
          char buffer[7]{};
          std::snprintf(buffer, sizeof(buffer), "\\u%04x", c);
          escaped += buffer;
        } else {
          escaped.push_back(static_cast<char>(c));
        }
        break;
    }
  }
  return escaped;
}

std::optional<std::uint64_t> process_start_ticks(HANDLE process) {
  if (process == nullptr) return std::nullopt;
  FILETIME creation{}, exit_time{}, kernel{}, user{};
  if (!GetProcessTimes(process, &creation, &exit_time, &kernel, &user)) {
    return std::nullopt;
  }
  return (static_cast<std::uint64_t>(creation.dwHighDateTime) << 32) |
         static_cast<std::uint64_t>(creation.dwLowDateTime);
}

}  // namespace

std::string win_error(const char* operation) {
  const DWORD error = GetLastError();
  std::ostringstream stream;
  stream << operation << " failed (" << error << ")";
  return stream.str();
}

int points_from_raw(std::int64_t value) {
  if (value < 0) return 0;
  return clamp_i64<int>((value + 255) / 256);
}

StatusWriter::StatusWriter(const std::string& path) {
  if (path.empty()) return;
  std::filesystem::path status_path(path);
  if (!status_path.parent_path().empty()) {
    std::error_code error;
    std::filesystem::create_directories(status_path.parent_path(), error);
    if (error) throw std::runtime_error("status directory: " + error.message());
  }
  stream_.open(status_path, std::ios::out | std::ios::app);
  if (!stream_) throw std::runtime_error("cannot open status file: " + path);
  pid_ = GetCurrentProcessId();
  start_ = process_start_ticks(GetCurrentProcess());
}

StatusWriter::~StatusWriter() = default;

void StatusWriter::write(const char* phase, bool connected, const std::string& reason,
                         std::uint32_t frame, const std::string& session,
                         DWORD target_pid) {
  if (!stream_) return;
  stream_ << "{\"phase\":\"" << json_escape(phase)
          << "\",\"pid\":" << pid_
          << ",\"process_start_time\":" << start_.value_or(0)
          << ",\"connected\":" << (connected ? "true" : "false")
          << ",\"target_pid\":" << target_pid
          << ",\"frame\":" << frame
          << ",\"session_epoch\":\"" << json_escape(session)
          << "\",\"reason\":\"" << json_escape(reason) << "\"}\n";
  stream_.flush();
}

}  // namespace BWRAPI::Legacy44
