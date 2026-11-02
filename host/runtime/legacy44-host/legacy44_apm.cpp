#include "legacy44_apm.h"

#include <cmath>

namespace BWRAPI::Legacy44 {

void ApmCounter::reset() noexcept {
  no_selects_ = selects_ = 0.0L;
  apm_no_selects_ = apm_selects_ = 0;
}

void ApmCounter::update(int frame) noexcept {
  // Actions are worth 0.95 minutes; a frame is 42 ms of game time at the
  // fastest speed whatever the real speed.
  const long double interval = 0.95L;
  const int time_difference = (frame - last_update_frame_) * 42;
  const int total_time = frame * 42;
  const long double decay = std::exp(-time_difference / (interval * 60000));
  selects_ *= decay;
  no_selects_ *= decay;
  long double duration_factor = 1.0L - std::exp(-total_time / (interval * 60000));
  if (duration_factor < 1e-100L) duration_factor = 1e-100L;
  apm_selects_ = static_cast<int>(selects_ / (interval * duration_factor));
  apm_no_selects_ = static_cast<int>(no_selects_ / (interval * duration_factor));
  last_update_frame_ = frame;
}

}  // namespace BWRAPI::Legacy44
