#pragma once

namespace BWRAPI::Legacy44 {

// The bot's actions per minute as BWAPI 4.4's APMCounter keeps them: every
// game command the server issues counts once without selects and once with
// them, every unit selection counts with selects only. Both counters decay
// exponentially in game time (42 ms per frame) and are divided by the
// match-length factor of the same formula.
class ApmCounter {
 public:
  // GameImpl::initializeData, run at every match boundary: both counters
  // restart from zero.
  void reset() noexcept;
  // GameImpl::updateStatistics, at the start of each update before the
  // server writes GameData; `frame` is the GameData frameCount.
  void update(int frame) noexcept;
  int apm(bool selects) const noexcept { return selects ? apm_selects_ : apm_no_selects_; }
  // The selections and game commands of one client update
  // (CommandAdapter::last_actions).
  void add(int selections, int commands) noexcept {
    selects_ += selections;
    no_selects_ += commands;
    selects_ += commands;
  }

 private:
  long double no_selects_ = 0.0L;
  long double selects_ = 0.0L;
  int apm_no_selects_ = 0;
  int apm_selects_ = 0;
  int last_update_frame_ = 0;
};

}  // namespace BWRAPI::Legacy44
