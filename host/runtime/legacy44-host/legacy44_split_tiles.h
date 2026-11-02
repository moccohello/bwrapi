#pragma once

#include <cstddef>
#include <cstdint>
#include <unordered_map>
#include <utility>
#include <vector>

namespace BWRAPI::Legacy44 {

// mapTileRegionId names a split pattern with a 13-bit index; GameData holds
// the first 5000 patterns.
constexpr std::size_t kSplitTileIndexCapacity = 0x2000;

struct SplitTileEntry {
  std::uint16_t mask;
  std::uint16_t primary;
  std::uint16_t secondary;
};

// Official GameData indexes split patterns, rather than tile occurrences.
// Uniform tiles and repeated patterns need no additional split-table row.
class SplitTileTable {
 public:
  explicit SplitTileTable(std::size_t capacity) : capacity_(capacity) {}

  bool encode(std::uint16_t primary, std::int32_t secondary,
              std::uint16_t mask, std::uint16_t& encoded) {
    if (secondary < 0 || mask == 0 || primary == secondary) {
      encoded = primary;
      return true;
    }
    if (mask == 0xffff) {
      encoded = static_cast<std::uint16_t>(secondary);
      return true;
    }
    auto second = static_cast<std::uint16_t>(secondary);
    if (primary > second) {
      std::swap(primary, second);
      mask = static_cast<std::uint16_t>(~mask);
    }
    const std::uint64_t key = std::uint64_t(mask)
        | (std::uint64_t(primary) << 16) | (std::uint64_t(second) << 32);
    auto found = indices_.find(key);
    if (found != indices_.end()) {
      encoded = static_cast<std::uint16_t>(0x2000u | found->second);
      return true;
    }
    if (entries_.size() >= capacity_ || entries_.size() >= 0x2000) return false;
    const auto index = static_cast<std::uint16_t>(entries_.size());
    indices_.emplace(key, index);
    entries_.push_back({mask, primary, second});
    encoded = static_cast<std::uint16_t>(0x2000u | index);
    return true;
  }

  const std::vector<SplitTileEntry>& entries() const { return entries_; }

 private:
  std::size_t capacity_;
  std::unordered_map<std::uint64_t, std::uint16_t> indices_;
  std::vector<SplitTileEntry> entries_;
};

}  // namespace BWRAPI::Legacy44
