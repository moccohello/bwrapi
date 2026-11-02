#pragma once

#include <BWAPI/Client/GameData.h>
#include <BWAPI/Client/UnitCommand.h>

#include <cstddef>
#include <cstdint>
#include <vector>

namespace BWRAPI::Legacy44 {

// BWAPI 4.4 latency compensation, server half. The client shows a unit
// command's effect only on the frame it issues it; the game carries it out
// after the remaining latency frames, and until then each projected frame
// still shows the unit as before. The official server therefore buffers every
// command it accepts by the frame the game will act on it
// (GameImpl::addToCommandBuffer) and re-applies the effects still in flight to
// each frame's GameData (applyLatencyCompensation, right after
// augmentUnitData), with the effects of CommandTemp.h: most commands apply
// until the remaining latency frames run out; cancels, Halt_Construction,
// Morph and Build_Addon add later order and finish steps; Train, Morph and
// Build_Addon also hold their price or supply on the frame the game takes it.
// Nothing applies while the client has latency compensation off or to a
// queued command.
class LatencyCompensation {
 public:
  // Buffers the unit commands the server handed to the game after the
  // client's update. `data` is the GameData of the last applied frame, whose
  // remaining latency frames place each command.
  void record(const std::vector<BWAPIC::UnitCommand>& commands, const BWAPI::GameData& data);
  // Moves the buffer on to game frame `frame` and applies the commands still
  // in flight to that frame's freshly projected GameData.
  void apply(BWAPI::GameData& data, int frame);
  // Forgets every buffered command (a match boundary or a new client).
  void reset();

 private:
  // CommandTemp's event types: the step of a command a buffer entry applies.
  enum class Step : std::uint8_t { Resource, Order, Finish };
  struct Entry {
    BWAPIC::UnitCommand command;
    Step step;
    // The BWAPI player that pays for the command: the unit's owner when it
    // was accepted.
    int player;
  };

  // Puts `entry` where the game acts on it `frames` frames from now; a
  // command the game takes this frame needs no buffer.
  void insert(const Entry& entry, int frames);
  std::vector<Entry>& slot(std::size_t offset) { return slots_[(head_ + offset) % slots_.size()]; }
  // Grows the buffer to `size` frames, keeping each entry's offset.
  void grow(std::size_t size);
  void execute(Entry& entry, bool current_frame, BWAPI::GameData& data) const;

  // A ring of frames; offset 0 is the next frame, whose entries the game acts
  // on then (the front of the official buffer). A frame's entries
  // keep their capacity when it is reused, so steady state allocates nothing.
  std::vector<std::vector<Entry>> slots_;
  std::size_t head_ = 0;
  std::size_t pending_ = 0;
  bool have_frame_ = false;
  int frame_ = 0;
};

}  // namespace BWRAPI::Legacy44
