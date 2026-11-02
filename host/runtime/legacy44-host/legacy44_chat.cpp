#include "legacy44_chat.h"

#include <BWAPI/Client/Command.h>
#include <BWAPI/Client/Shape.h>
#include <BWAPI/Color.h>
#include <BWAPI/CoordinateType.h>
#include <BWAPI/Position.h>

#include <cstdio>
#include <cstring>
#include <sstream>

namespace BWRAPI::Legacy44 {

namespace {

// The game screen is 480 pixels high and as wide as the window's aspect;
// 1280 covers the widest window with room, and drawing clips what falls
// outside the screen anyway.
constexpr int kOverlayScreenWidth = 1280;
constexpr int kOverlayScreenHeight = 480;

// The cheat strings parseText's /cheats sends, in the official order.
constexpr const char* kCheatTexts[] = {
    "power overwhelming", "operation cwal",       "the gathering",
    "medieval man",       "black sheep wall",     "food for thought",
    "modify the phase variance", "something for nothing", "something for nothing",
    "something for nothing",     "show me the money",    "show me the money",
    "show me the money",         "show me the money",    "show me the money",
};

// A command or string is dropped when the client filled its buffer; the line
// still counts as recognized, as a line the official event buffers drop does.
bool add_command(BWAPI::GameData& data, BWAPIC::CommandType::Enum type, int value1 = 0,
                 int value2 = 0) {
  if (data.commandCount >= BWAPI::GameData::MAX_COMMANDS) return false;
  data.commands[data.commandCount++] = BWAPIC::Command(type, value1, value2);
  return true;
}

int add_string(BWAPI::GameData& data, const std::string& text) {
  if (data.stringCount >= BWAPI::GameData::MAX_STRINGS) return -1;
  const int index = data.stringCount++;
  std::memcpy(data.strings[index], text.c_str(), text.size() + 1);
  return index;
}

void print(BWAPI::GameData& data, const std::string& text) {
  const int index = add_string(data, text);
  if (index >= 0) add_command(data, BWAPIC::CommandType::Printf, index);
}

void send_text(BWAPI::GameData& data, const char* text) {
  const int index = add_string(data, text);
  if (index >= 0) add_command(data, BWAPIC::CommandType::SendText, index);
}

void add_shape(BWAPI::GameData& data, const BWAPIC::Shape& shape) {
  if (data.shapeCount >= BWAPI::GameData::MAX_SHAPES) return;
  data.shapes[data.shapeCount++] = shape;
}

void add_text_shape(BWAPI::GameData& data, const char* text, int x, int y) {
  const int index = add_string(data, text);
  if (index < 0) return;
  add_shape(data, BWAPIC::Shape(BWAPIC::ShapeType::Text, BWAPI::CoordinateType::Screen, x, y, 0,
                                0, index, BWAPI::Text::Size::Small, 0, false));
}

}  // namespace

bool ChatCommands::parse(const std::string& text, BWAPI::GameData& data) {
  namespace C = BWAPIC::CommandType;
  std::istringstream line(text);
  std::string cmd;
  line >> cmd;
  if (cmd == "/leave") {
    add_command(data, C::LeaveGame);
  } else if (cmd == "/speed") {
    // As in parseText, a number that fails to parse reads as 0 and a missing
    // one keeps the default.
    int speed = -1;
    line >> speed;
    add_command(data, C::SetLocalSpeed, speed);
    print(data, "Changed game speed");
  } else if (cmd == "/fs") {
    int skip = 1;
    line >> skip;
    add_command(data, C::SetFrameSkip, skip);
    print(data, "Altered frame skip");
  } else if (cmd == "/cheats") {
    for (const char* cheat : kCheatTexts) send_text(data, cheat);
  } else if (cmd == "/restart") {
    add_command(data, C::RestartGame);
  } else if (cmd == "/nogui") {
    // The line the official setGUI leaves in hasGUI names the new state.
    const bool enabled = !data.hasGUI;
    add_command(data, C::SetGui, enabled ? 1 : 0);
    print(data, enabled ? "GUI: enabled" : "GUI: disabled");
  } else if (cmd == "/grid") {
    grid = !grid;
    print(data, grid ? "Matrix grid enabled" : "Matrix grid disabled");
  } else if (cmd == "/fps") {
    showfps = !showfps;
    print(data, showfps ? "FPS display enabled" : "FPS display disabled");
  } else {
    return false;
  }
  return true;
}

bool ChatCommands::recognizes(const std::string& text) {
  std::istringstream line(text);
  std::string cmd;
  line >> cmd;
  for (const char* known : {"/leave", "/speed", "/fs", "/cheats", "/restart", "/nogui", "/grid", "/fps"})
    if (cmd == known) return true;
  return false;
}

void ChatCommands::append_overlay_shapes(BWAPI::GameData& data) {
  // updateOverlays draws nothing while the GUI is off.
  if (!data.hasGUI) return;
  if (grid) {
    // The official matrix grid: along the right and bottom edge of every
    // 32 pixel tile in view, pairs of 2 pixel ticks 4 pixels apart.
    const int grey = BWAPI::Colors::Grey.getID();
    const int first_x = data.screenX / 32;
    const int last_x = (data.screenX + kOverlayScreenWidth) / 32 + 1;
    const int first_y = data.screenY / 32;
    const int last_y = (data.screenY + kOverlayScreenHeight) / 32 + 1;
    for (int ty = first_y; ty <= last_y; ++ty) {
      for (int tx = first_x; tx <= last_x; ++tx) {
        for (int i = 0; i < 32; i += 4) {
          add_shape(data, BWAPIC::Shape(BWAPIC::ShapeType::Line, BWAPI::CoordinateType::Map,
                                        tx * 32 + 32, ty * 32 + i, tx * 32 + 32, ty * 32 + i + 2,
                                        0, 0, grey, false));
          add_shape(data, BWAPIC::Shape(BWAPIC::ShapeType::Line, BWAPI::CoordinateType::Map,
                                        tx * 32 + i, ty * 32 + 32, tx * 32 + i + 2, ty * 32 + 32,
                                        0, 0, grey, false));
        }
      }
    }
    if (data.mouseX != BWAPI::Positions::Unknown.x) {
      char tile[64];
      std::snprintf(tile, sizeof tile, "%c(%d, %d)", static_cast<int>(BWAPI::Text::White),
                    (data.screenX + data.mouseX) / 32, (data.screenY + data.mouseY) / 32);
      add_text_shape(data, tile, 64, 288);
    }
  }
  if (showfps) {
    char text[96];
    std::snprintf(text, sizeof text, "%cFrame Rate (Logical Frames)\nLFPS: %d\nAvg LFPS:%.3f",
                  static_cast<int>(BWAPI::Text::Yellow), data.fps, data.averageFPS);
    add_text_shape(data, text, 1, 1);
  }
}

}  // namespace BWRAPI::Legacy44
