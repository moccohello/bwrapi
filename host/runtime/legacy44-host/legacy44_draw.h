#pragma once

#include <BWAPI/Client/GameData.h>

#include <bwrapi.h>

#include <cstdint>
#include <string>
#include <vector>

namespace BWRAPI::Legacy44 {

// The client's draw shapes as the bwr_draw list that draws them into the game
// screen: BWAPI palette colors as RGBA, text as UTF-8 and the game's text
// colors for BWAPI's Text codes. The buffers keep their capacity, so a round
// costs no allocation once they have grown to the client's usual drawing.
class DrawList {
 public:
  // The shapes of a client update, before they are cleared; null draws
  // nothing.
  const BwrDrawList* build(const BWAPI::GameData* data);

 private:
  std::vector<BwrShape> shapes_;
  std::string text_;
  std::wstring wide_;
  BwrDrawList list_{};
};

}  // namespace BWRAPI::Legacy44
