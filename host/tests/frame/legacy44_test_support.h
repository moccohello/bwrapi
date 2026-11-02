#pragma once

// Shared pieces of the host projection tests: check macros, a GameData with its
// projection, and builders for engine unit ids and common rows.
#include "legacy44_frame_fixture.h"
#include "../../runtime/legacy44-host/legacy44_projection.h"

#include <cstdint>
#include <iostream>
#include <memory>
#include <string>

namespace BWRAPI::Legacy44::Test {

inline int& failures() {
  static int count = 0;
  return count;
}

#define CHECK(condition)                                                      \
  do {                                                                        \
    if (!(condition)) {                                                       \
      std::cerr << __FILE__ << ':' << __LINE__ << ": check failed: "          \
                << #condition << '\n';                                        \
      ++::BWRAPI::Legacy44::Test::failures();                                 \
    }                                                                         \
  } while (false)

inline int finish() {
  if (failures() != 0) std::cerr << failures() << " check(s) failed\n";
  return failures() == 0 ? 0 : 1;
}

// Engine unit ids are (slot + 1) | uniqueness << 16.
inline std::uint64_t unit_id(unsigned slot, unsigned uniqueness = 1) {
  return (static_cast<std::uint64_t>(slot) + 1) | static_cast<std::uint64_t>(uniqueness) << 16;
}

// A projection that writes into one GameData, like the host does.
struct Env {
  std::unique_ptr<BWAPI::GameData> data = std::make_unique<BWAPI::GameData>();
  GameDataProjection projection;
  Fixture::FrameData fx;
  FrameSample sample;
  std::string error;

  bool project() {
    error.clear();
    return projection.project(fx.build(), *data, sample, error);
  }
  const BWAPI::UnitData& unit(int id) const { return data->units[id]; }
};

inline Fixture::UnitsRows::Row unit_row(unsigned slot, int type, int owner, int x, int y,
                                        unsigned uniqueness = 1) {
  Fixture::UnitsRows::Row row;
  row.id = unit_id(slot, uniqueness);
  row.unit_type = static_cast<std::uint16_t>(type);
  row.owner = static_cast<std::uint8_t>(owner);
  row.x = static_cast<std::int16_t>(x);
  row.y = static_cast<std::int16_t>(y);
  row.hit_points = 40 * 256;
  row.order = 3;  // Stop
  row.completed = 1;
  return row;
}

inline Fixture::PlayersRows::Row player_row(int id, int race) {
  Fixture::PlayersRows::Row row;
  row.id = static_cast<std::uint8_t>(id);
  row.race = static_cast<std::uint8_t>(race);
  return row;
}

inline void set_text(BwrText32& text, const char* value) {
  text = {};
  for (std::size_t i = 0; value[i] != '\0' && i < sizeof(text.text) - 1; ++i) text.text[i] = value[i];
}

inline void set_text(BwrText256& text, const char* value) {
  text = {};
  for (std::size_t i = 0; value[i] != '\0' && i < sizeof(text.text) - 1; ++i) text.text[i] = value[i];
}

}  // namespace BWRAPI::Legacy44::Test
