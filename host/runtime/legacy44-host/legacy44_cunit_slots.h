#pragma once

#include "legacy44_common.h"

#include <algorithm>
#include <array>
#include <cstdint>
#include <cstdio>
#include <string>
#include <vector>

namespace BWRAPI::Legacy44 {

// Eviction order when more than 1700 CUnit rows are live. Never is the local
// player's own units.
enum class CunitSlotTier : std::uint8_t {
  Never = 0,
  Visible = 1,
  HiddenEnemy = 2,
  NeutralDoodad = 3,
};

inline CunitSlotTier ClassifyCunitSlotTier(int owner, int type, int self, bool visible) {
  if (self >= 0 && owner == self) return CunitSlotTier::Never;
  const bool resource = type == 176 || type == 177 || type == 178 || type == 188;
  if (owner == static_cast<int>(kNeutralPlayerId) && !resource)
    return CunitSlotTier::NeutralDoodad;
  return visible ? CunitSlotTier::Visible : CunitSlotTier::HiddenEnemy;
}

// Maps SC:R CUnit records onto the classic slots (0-1699) of the unitArray
// every bot sees.
//
// A slot stays with its record while the same life (the engine unit id, which
// carries the record's uniqueness) is requested every frame. A vanished or
// reused record frees its slot and the next occupant sees a bumped 5-bit
// uniqueness, as in Brood War. The record count is the engine's pool size.
class CunitSlotAllocator {
 public:
  static constexpr std::uint32_t kSlots = static_cast<std::uint32_t>(kMaxUnitArray);

  struct Request {
    std::uint32_t ordinal = 0;
    CunitSlotTier tier = CunitSlotTier::Never;
    // Engine unit id of the record's life.
    std::uint64_t life = 0;
  };
  struct Slot {
    std::int32_t slot = -1;  // -1: no classic slot (over capacity).
    std::uint32_t uniqueness = 0;
  };

  // `records` is the engine pool's record count; requests stay bounded by it.
  // A count of 0 grows to what the frames ask for.
  void reset(std::uint32_t records) {
    ordinals_ = records;
    slot_of_ordinal_.assign(records, -1);
    life_.assign(records, 0);
    ordinal_of_slot_.fill(-1);
    uniqueness_.fill(0);
    request_of_ordinal_.assign(records, -1);
    keep_.clear();
    candidates_.clear();
    evicted_rows_ = 0;
    peak_rows_ = 0;
  }

  // Grows the record tables when the pool size was unknown at reset.
  void ensure(std::uint32_t records) {
    if (records <= ordinals_) return;
    ordinals_ = records;
    slot_of_ordinal_.resize(records, -1);
    life_.resize(records, 0);
    request_of_ordinal_.resize(records, -1);
  }

  // Assigns slots to the units of one frame; `slots` parallels `requests`.
  bool assign_public(const std::vector<Request>& requests, std::vector<Slot>& slots,
                     std::string& error) {
    if (!index_requests(requests, error)) return false;
    for (std::uint32_t ordinal = 0; ordinal < ordinals_; ++ordinal) {
      if (slot_of_ordinal_[ordinal] < 0) continue;
      const auto index = request_of_ordinal_[ordinal];
      if (index < 0 ||
          !same_life(ordinal, requests[static_cast<std::size_t>(index)]))
        release(ordinal);
    }

    keep_.assign(requests.size(), true);
    peak_rows_ = (std::max)(peak_rows_, static_cast<std::uint32_t>(requests.size()));
    if (requests.size() > kSlots) {
      // Candidates: every request that may be evicted (ref = request index).
      candidates_.clear();
      for (std::size_t index = 0; index < requests.size(); ++index) {
        const auto& request = requests[index];
        if (request.tier != CunitSlotTier::Never)
          candidates_.push_back({request.tier, slot_of_ordinal_[request.ordinal] >= 0,
                                request.ordinal, static_cast<std::int64_t>(index)});
      }
      const std::size_t excess = requests.size() - kSlots;
      if (candidates_.size() < excess) {
        error = "own units exceed the 1700 classic CUnit slots";
        return false;
      }
      std::sort(candidates_.begin(), candidates_.end(), eviction_first);
      for (std::size_t i = 0; i < excess; ++i) {
        keep_[static_cast<std::size_t>(candidates_[i].ref)] = false;
        release(candidates_[i].ordinal);
        ++evicted_rows_;
      }
    }

    if (!place(requests, error)) return false;
    fill(requests, slots);
    return true;
  }

  // One line per match when rows were dropped for capacity; clears the counts.
  void report(std::uint64_t generation) {
    if (evicted_rows_ != 0) {
      std::fprintf(stderr,
                   "[cunit-slot-capacity] generation=%llu evicted_rows=%llu peak_rows=%u "
                   "classic_slots=%u order=neutral-doodads,hidden-enemies,others own=never\n",
                   static_cast<unsigned long long>(generation),
                   static_cast<unsigned long long>(evicted_rows_), peak_rows_, kSlots);
      std::fflush(stderr);
    }
    evicted_rows_ = 0;
    peak_rows_ = 0;
  }

 private:
  struct Order {
    CunitSlotTier tier;
    bool held;
    std::uint32_t ordinal;
    std::int64_t ref;
  };

  // Highest tier first; within a tier, rows without a slot before established
  // ones, then higher records first.
  static bool eviction_first(const Order& a, const Order& b) {
    if (a.tier != b.tier) return a.tier > b.tier;
    if (a.held != b.held) return !a.held;
    return a.ordinal > b.ordinal;
  }

  bool index_requests(const std::vector<Request>& requests, std::string& error) {
    for (const auto& request : requests)
      ensure(request.ordinal >= ordinals_ ? request.ordinal + 1 : ordinals_);
    std::fill(request_of_ordinal_.begin(), request_of_ordinal_.end(), -1);
    for (std::size_t index = 0; index < requests.size(); ++index) {
      const auto& request = requests[index];
      if (request.ordinal >= ordinals_ ||
          request_of_ordinal_[request.ordinal] >= 0) {
        error = "CUnit slot request has a duplicate or out-of-range record";
        return false;
      }
      request_of_ordinal_[request.ordinal] = static_cast<std::int32_t>(index);
    }
    return true;
  }

  bool same_life(std::uint32_t ordinal, const Request& request) const {
    return life_[ordinal] == request.life;
  }

  // Prefer slot == record for low records so the common case keeps the
  // classic layout; everything else takes the lowest free slot.
  bool place(const std::vector<Request>& requests, std::string& error) {
    for (std::size_t index = 0; index < requests.size(); ++index) {
      const auto ordinal = requests[index].ordinal;
      if (keep_[index] && slot_of_ordinal_[ordinal] < 0 && ordinal < kSlots &&
          ordinal_of_slot_[ordinal] < 0)
        occupy(ordinal, requests[index]);
    }
    std::uint32_t next_free = 0;
    for (std::size_t index = 0; index < requests.size(); ++index) {
      if (!keep_[index] || slot_of_ordinal_[requests[index].ordinal] >= 0) continue;
      while (next_free < kSlots && ordinal_of_slot_[next_free] >= 0) ++next_free;
      if (next_free == kSlots) {
        error = "CUnit slot allocator ran out of classic slots";
        return false;
      }
      occupy(next_free, requests[index]);
    }
    return true;
  }

  void fill(const std::vector<Request>& requests, std::vector<Slot>& slots) const {
    slots.assign(requests.size(), Slot{});
    for (std::size_t index = 0; index < requests.size(); ++index) {
      if (!keep_[index]) continue;
      const auto slot = slot_of_ordinal_[requests[index].ordinal];
      slots[index] = {slot, uniqueness_[static_cast<std::size_t>(slot)]};
    }
  }

  void occupy(std::uint32_t slot, const Request& request) {
    ordinal_of_slot_[slot] = static_cast<std::int16_t>(request.ordinal);
    slot_of_ordinal_[request.ordinal] = static_cast<std::int16_t>(slot);
    life_[request.ordinal] = request.life;
  }

  void release(std::uint32_t ordinal) {
    if (ordinal >= ordinals_ || slot_of_ordinal_[ordinal] < 0) return;
    const auto slot = static_cast<std::size_t>(slot_of_ordinal_[ordinal]);
    ordinal_of_slot_[slot] = -1;
    uniqueness_[slot] = static_cast<std::uint8_t>((uniqueness_[slot] + 1u) & 31u);
    slot_of_ordinal_[ordinal] = -1;
  }

  std::uint32_t ordinals_ = 0;
  std::vector<std::int16_t> slot_of_ordinal_;
  std::vector<std::uint64_t> life_;
  std::array<std::int16_t, kSlots> ordinal_of_slot_{};
  std::array<std::uint8_t, kSlots> uniqueness_{};
  // Frame scratch, reused so a frame costs no allocation.
  std::vector<std::int32_t> request_of_ordinal_;
  std::vector<bool> keep_;
  std::vector<Order> candidates_;
  std::uint64_t evicted_rows_ = 0;
  std::uint32_t peak_rows_ = 0;
};

}  // namespace BWRAPI::Legacy44
