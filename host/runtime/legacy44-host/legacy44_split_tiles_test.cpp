#include "legacy44_split_tiles.h"

#include <iostream>

namespace {
bool require(bool condition, const char* message) {
  if (condition) return true;
  std::cerr << message << '\n';
  return false;
}

std::uint16_t region_at(const BWRAPI::Legacy44::SplitTileTable& table,
                        std::uint16_t encoded, unsigned minitile) {
  if (!(encoded & 0x2000)) return encoded;
  const auto& row = table.entries().at(encoded & 0x1fff);
  return (row.mask & (1u << minitile)) ? row.secondary : row.primary;
}
}  // namespace

int main() {
  using BWRAPI::Legacy44::SplitTileTable;
  SplitTileTable repeats(5000);
  std::uint16_t encoded = 0;
  for (unsigned tile = 0; tile < 65536; ++tile) {
    if (!require(repeats.encode(12, 34, 0xa55a, encoded) && encoded == 0x2000,
                 "repeated patterns must share one deterministic split index")) return 1;
    for (unsigned mini = 0; mini < 16; ++mini)
      if (!require(region_at(repeats, encoded, mini) ==
                   ((0xa55a & (1u << mini)) ? 34 : 12),
                   "dedup must preserve every minitile region")) return 1;
  }
  if (!require(repeats.encode(34, 12, 0x5aa5, encoded) && encoded == 0x2000
               && repeats.entries().size() == 1,
               "swapped regions with complementary masks are the same pattern")) return 1;
  for (const auto mask : {std::uint16_t(0), std::uint16_t(0xffff), std::uint16_t(0x8421)}) {
    const auto primary = std::uint16_t(12);
    const auto secondary = mask == 0x8421 ? 12 : 34;
    const auto expected = mask == 0xffff ? 34 : 12;
    if (!require(repeats.encode(primary, secondary, mask, encoded),
                 "uniform regions must encode directly")) return 1;
    for (unsigned mini = 0; mini < 16; ++mini)
      if (!require(region_at(repeats, encoded, mini) == expected,
                   "uniform normalization must preserve all minitiles")) return 1;
  }
  if (!require(repeats.entries().size() == 1,
               "uniform tiles must not consume split-table capacity")) return 1;
  SplitTileTable capacity(5000);
  for (unsigned mask = 1; mask <= 5000; ++mask)
    if (!require(capacity.encode(1, 2, static_cast<std::uint16_t>(mask), encoded)
                 && encoded == (0x2000u | (mask - 1)),
                 "all 5000 official entries must remain usable")) return 1;
  if (!require(!capacity.encode(1, 2, 5001, encoded) && capacity.entries().size() == 5000,
               "5001 genuinely distinct patterns must fail without truncation")) return 1;
  if (!require(capacity.encode(1, 2, 1, encoded) && encoded == 0x2000,
               "an existing pattern must remain usable at full capacity")) return 1;
  return 0;
}
