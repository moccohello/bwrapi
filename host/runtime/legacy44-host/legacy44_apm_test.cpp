#include "legacy44_apm.h"

#include <iostream>

namespace {

int failures = 0;

void check(bool condition, const char* message) {
  if (condition) return;
  std::cerr << "check failed: " << message << '\n';
  ++failures;
}

}  // namespace

int main() {
  using BWRAPI::Legacy44::ApmCounter;

  // A hundred commands in the first minute of game time (1428 frames of
  // 42 ms) read 56 APM; another minute without commands decays them to 14.
  ApmCounter apm;
  apm.update(0);
  check(apm.apm(false) == 0 && apm.apm(true) == 0, "frame 0 has no APM");
  apm.add(0, 100);
  apm.update(1428);
  check(apm.apm(false) == 56, "100 actions in one minute");
  check(apm.apm(true) == 56, "an action without a select counts for both");
  apm.update(2856);
  check(apm.apm(false) == 14, "a quiet minute decays the counter");

  // A selection counts only with selects: 100 commands with their 100
  // selections read 56 and 112.
  ApmCounter selected;
  selected.update(0);
  selected.add(100, 100);
  selected.update(1428);
  check(selected.apm(false) == 56 && selected.apm(true) == 112, "selections count with selects only");
  selected.reset();
  selected.update(1428);
  check(selected.apm(false) == 0 && selected.apm(true) == 0, "a new match restarts from zero");
  return failures == 0 ? 0 : 1;
}
