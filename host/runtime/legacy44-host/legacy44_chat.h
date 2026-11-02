#pragma once

#include "legacy44_common.h"

#include <string>

namespace BWRAPI::Legacy44 {

// GameImpl::parseText: the official chat commands the server runs itself. A
// recognized line becomes game commands in GameData (the caller drains them
// through CommandAdapter like the client's own) and never reaches the client
// as SendText. /wmode and the _DEBUG-only commands are not part of the set.
class ChatCommands {
 public:
  // Runs `text` when it is one of the official commands; the commands it
  // stands for are appended after any the client wrote. Returns whether the
  // line was recognized.
  bool parse(const std::string& text, BWAPI::GameData& data);
  // Whether parse would take `text`.
  static bool recognizes(const std::string& text);
  // The /grid and /fps overlays.
  bool grid = false;
  bool showfps = false;
  // GameImpl::updateOverlays: the shapes of the toggles that are on, appended
  // to the GameData about to be shown (nothing while the GUI is off).
  void append_overlay_shapes(BWAPI::GameData& data);
};

}  // namespace BWRAPI::Legacy44
