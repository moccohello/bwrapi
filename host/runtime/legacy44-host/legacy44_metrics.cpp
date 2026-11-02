#include "legacy44_metrics.h"

#include <chrono>
#include <cmath>

namespace BWRAPI::Legacy44 {

void Legacy44FrameMetrics::reset() noexcept {
  initialized_ = false;
  match_active_ = false;
  last_update_frame_ = 0;
  fps_ = 0.0L;
  average_fps_ = 0.0L;
  last_update_time_ = {};
}

void Legacy44FrameMetrics::on_frame(int frame, Clock::time_point observed_at,
                                    bool force_match_reset) noexcept {
  if (!initialized_ || !match_active_ || force_match_reset) {
    initialize(frame, observed_at);
    match_active_ = true;
    return;
  }
  update(frame, observed_at);
}

void Legacy44FrameMetrics::leave_match() noexcept {
  match_active_ = false;
}

int Legacy44FrameMetrics::fps() const noexcept {
  return static_cast<int>(std::round(fps_));
}

double Legacy44FrameMetrics::average_fps() const noexcept {
  return static_cast<double>(average_fps_);
}

void Legacy44FrameMetrics::initialize(int frame,
                                      Clock::time_point observed_at) noexcept {
  last_update_time_ = observed_at;
  last_update_frame_ = frame;
  fps_ = 0.0L;
  average_fps_ = 0.0L;
  initialized_ = true;
}

void Legacy44FrameMetrics::update(int frame,
                                  Clock::time_point observed_at) noexcept {
  const auto elapsed = observed_at - last_update_time_;
  if (elapsed < std::chrono::seconds(1) && fps_ != 0.0L) return;

  const auto one_second =
      std::chrono::duration_cast<Clock::duration>(std::chrono::seconds(1)).count();
  const auto elapsed_count = elapsed.count();
  // steady_clock can have a coarser tick than one call interval. Avoid
  // publishing an infinite first sample; the next frame will retry from the
  // same timestamp, matching the pinned FPSCounter's fps==0 update trigger.
  if (one_second <= 0 || elapsed_count <= 0) return;

  const int frame_difference = frame - last_update_frame_;
  const long double elapsed_seconds =
      static_cast<long double>(elapsed_count) / one_second;
  fps_ = frame_difference / elapsed_seconds;
  average_fps_ = average_fps_ * 0.7 + fps_ * 0.3;
  last_update_frame_ = frame;
  last_update_time_ = observed_at;
}

}  // namespace BWRAPI::Legacy44
