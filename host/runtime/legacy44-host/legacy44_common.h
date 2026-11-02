#pragma once

#define NOMINMAX

#include <BWAPI/Client/GameData.h>

#include <windows.h>

#include <algorithm>
#include <cstddef>
#include <cstdint>
#include <filesystem>
#include <fstream>
#include <iterator>
#include <limits>
#include <optional>
#include <string>

namespace BWRAPI::Legacy44 {

constexpr std::uint32_t kClientVersion = 10003;
// Pinned BWAPI 4.4's include/svnrev.h.  GameData owns this Client-facing
// identity; it is not a runtime observation.
constexpr int kBwapiRevision = 5030;
constexpr std::size_t kMaxUnits = 10000;
constexpr std::size_t kMaxUnitArray = 1700;
constexpr std::size_t kMaxUnitSearch = 1700 * 2;
constexpr std::size_t kMaxPlayers = 12;
constexpr std::size_t kPlayablePlayerCount = 8;
constexpr std::size_t kNeutralPlayerId = 11;
constexpr std::size_t kMaxBullets = 100;
constexpr std::size_t kMaxMapTiles = 256;
constexpr std::size_t kMaxWalkTiles = 1024;
constexpr std::size_t kMaxRegions = 5000;
constexpr std::size_t kMaxStartLocations = 8;

std::string win_error(const char* operation);

template <typename T>
T clamp_i64(std::int64_t value) {
  const auto minimum = static_cast<std::int64_t>(std::numeric_limits<T>::min());
  const auto maximum = static_cast<std::int64_t>(std::numeric_limits<T>::max());
  return static_cast<T>(std::clamp(value, minimum, maximum));
}

int points_from_raw(std::int64_t value);

class StatusWriter {
 public:
  explicit StatusWriter(const std::string& path);
  ~StatusWriter();
  StatusWriter(const StatusWriter&) = delete;
  StatusWriter& operator=(const StatusWriter&) = delete;

  void write(const char* phase, bool connected, const std::string& reason,
             std::uint32_t frame = 0, const std::string& session = {},
             DWORD target_pid = 0);

 private:
  std::ofstream stream_;
  DWORD pid_ = 0;
  std::optional<std::uint64_t> start_;
};

}  // namespace BWRAPI::Legacy44
