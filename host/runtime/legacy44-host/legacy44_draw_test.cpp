// The client's shapes become the bwr_draw list: palette colors as RGBA, text
// shapes with their string as UTF-8 in the list's text, and the game's text
// colors.
#include "legacy44_draw.h"

#include <BWAPI/Client/Shape.h>
#include <BWAPI/Color.h>
#include <BWAPI/CoordinateType.h>

#include <windows.h>

#include <cstring>
#include <iostream>
#include <memory>
#include <string>

using namespace BWRAPI::Legacy44;

namespace {

int failures = 0;

#define CHECK(condition)                                                \
  do {                                                                  \
    if (!(condition)) {                                                 \
      std::cerr << __FILE__ << ':' << __LINE__ << ": check failed: "    \
                << #condition << '\n';                                  \
      ++failures;                                                       \
    }                                                                   \
  } while (false)

void add_text(BWAPI::GameData& data, int x, int y, const char* text, int size) {
  std::strcpy(data.strings[data.stringCount], text);
  data.shapes[data.shapeCount++] = BWAPIC::Shape(BWAPIC::ShapeType::Text, BWAPI::CoordinateType::Map, x, y, 0, 0,
                                                 data.stringCount++, size, 0, false);
}

void shapes_become_the_draw_list() {
  auto data = std::make_unique<BWAPI::GameData>();
  data->shapes[data->shapeCount++] = BWAPIC::Shape(BWAPIC::ShapeType::Circle, BWAPI::CoordinateType::Mouse, 1, 2,
                                                   0, 0, 12, 0, BWAPI::Colors::Red, true);
  add_text(*data, 5, 6, "\x03" "frame", 2);
  // A string the client never wrote and a size BWAPI has not are left out.
  data->shapes[data->shapeCount++] =
      BWAPIC::Shape(BWAPIC::ShapeType::Text, BWAPI::CoordinateType::Screen, 0, 0, 0, 0, 7, 1, 0, false);
  add_text(*data, 0, 0, "x", 4);
  // A computer player name in the system code page arrives as UTF-8.
  add_text(*data, 7, 8, "\xc7\xd1", 0);

  DrawList draw;
  const BwrDrawList* list = draw.build(data.get());
  CHECK(list != nullptr && list->size == sizeof(BwrDrawList) && list->count == 3);
  if (list == nullptr || list->count != 3) return;
  const BwrShape& circle = list->shapes[0];
  CHECK(circle.kind == BWR_SHAPE_CIRCLE && circle.coordinates == BWR_COORDINATES_MOUSE && circle.solid == 1);
  CHECK(circle.x1 == 1 && circle.y1 == 2 && circle.extra1 == 12);
  const BWAPI::Color red = BWAPI::Colors::Red;
  CHECK(circle.rgba == (0xff000000u | red.blue() << 16 | red.green() << 8 | red.red()));
  const std::string text(list->text, list->text_bytes);
  const BwrShape& frame = list->shapes[1];
  CHECK(frame.kind == BWR_SHAPE_TEXT && frame.coordinates == BWR_COORDINATES_MAP && frame.text_size == 2);
  CHECK(text.substr(frame.text_offset, frame.text_len) == "\x03" "frame");
  const BwrShape& name = list->shapes[2];
  const std::string utf8 = text.substr(name.text_offset, name.text_len);
  CHECK(GetACP() != 949 || utf8 == "\xed\x95\x9c");
  CHECK(!utf8.empty());
  // Text starts in the game's default blue.
  CHECK(list->text_colors[2] == 0xffe8b8b8u);
  CHECK(draw.build(nullptr) == nullptr);
}

}  // namespace

int main() {
  shapes_become_the_draw_list();
  if (failures != 0) {
    std::cerr << failures << " check(s) failed\n";
    return 1;
  }
  std::cout << "legacy44 draw tests passed\n";
  return 0;
}
