#include "legacy44_metrics.h"

#include <chrono>
#include <cmath>
#include <iostream>

namespace {

bool require(bool condition, const char* message) {
  if (condition) return true;
  std::cerr << message << '\n';
  return false;
}

bool near(double value, double expected) {
  return std::abs(value - expected) < 1e-9;
}

}  // namespace

int main() {
  using BWRAPI::Legacy44::Legacy44FrameMetrics;
  using namespace std::chrono_literals;

  Legacy44FrameMetrics metrics;
  const auto start = Legacy44FrameMetrics::Clock::time_point{};
  metrics.on_frame(100, start);
  if (!require(metrics.fps() == 0 && near(metrics.average_fps(), 0.0),
               "match start must publish the pinned zero baseline")) {
    return 1;
  }

  // Two elapsed frames in half a second produce 4 FPS; the pinned 0.3
  // weighting gives an average of 1.2. The next one-second window checks the
  // frame delta and the 0.7/0.3 filter together.
  metrics.on_frame(102, start + 500ms);
  metrics.on_frame(106, start + 1500ms);
  if (!require(metrics.fps() == 4 && near(metrics.average_fps(), 2.04),
               "two frame windows must match pinned FPS and average semantics")) {
    return 1;
  }

  metrics.leave_match();
  if (!require(metrics.fps() == 4 && near(metrics.average_fps(), 2.04),
               "leaving a match must retain the last published counters")) {
    return 1;
  }
  metrics.on_frame(900, start + 2500ms);
  if (!require(metrics.fps() == 0 && near(metrics.average_fps(), 0.0),
               "the first frame after a match boundary must reset counters")) {
    return 1;
  }
  metrics.on_frame(901, start + 2600ms);
  if (!require(metrics.fps() == 10 && near(metrics.average_fps(), 3.0),
               "the new match must measure from its own frame and clock origin")) {
    return 1;
  }
  return 0;
}
