#pragma once

#include <chrono>

namespace BWRAPI::Legacy44 {

// Host-side counterpart of the pinned BWAPI 4.4 FPSCounter. Samples are taken
// from accepted MatchFrame publications using a monotonic clock; engine UI FPS
// and polling-loop rate are not inputs.
class Legacy44FrameMetrics {
 public:
  using Clock = std::chrono::steady_clock;

  void reset() noexcept;
  void on_frame(int frame, Clock::time_point observed_at,
                bool force_match_reset = false) noexcept;
  void leave_match() noexcept;

  int fps() const noexcept;
  double average_fps() const noexcept;

 private:
  void initialize(int frame, Clock::time_point observed_at) noexcept;
  void update(int frame, Clock::time_point observed_at) noexcept;

  bool initialized_ = false;
  bool match_active_ = false;
  int last_update_frame_ = 0;
  long double fps_ = 0.0L;
  long double average_fps_ = 0.0L;
  Clock::time_point last_update_time_{};
};

}  // namespace BWRAPI::Legacy44
