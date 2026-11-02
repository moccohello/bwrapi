#include "legacy44_draw.h"

#include <BWAPI/Color.h>

#include <windows.h>

#include <algorithm>
#include <cstring>

namespace BWRAPI::Legacy44 {

namespace {

constexpr std::uint32_t rgba(int red, int green, int blue) {
  return static_cast<std::uint32_t>(red) | static_cast<std::uint32_t>(green) << 8 |
         static_cast<std::uint32_t>(blue) << 16 | 0xff000000u;
}

// The game's text color of each BWAPI Text code (2 Default .. 31 Turquoise);
// codes that are no color are white.
constexpr std::uint32_t kWhite = rgba(0xff, 0xff, 0xff);
constexpr std::uint32_t kTextColors[32] = {
    kWhite,                   kWhite,                   rgba(0xb8, 0xb8, 0xe8), rgba(0xdc, 0xdc, 0x3c),
    rgba(0xff, 0xff, 0xff),   rgba(0x84, 0x74, 0x74),   rgba(0xc8, 0x18, 0x18), rgba(0x10, 0xfc, 0x18),
    rgba(0xf4, 0x04, 0x04),   kWhite,                   kWhite,                 kWhite,
    kWhite,                   kWhite,                   rgba(0x0c, 0x48, 0xcc), rgba(0x2c, 0xb4, 0x94),
    rgba(0x88, 0x40, 0x9c),   rgba(0xf8, 0x8c, 0x14),   kWhite,                 kWhite,
    kWhite,                   rgba(0x70, 0x30, 0x14),   rgba(0xcc, 0xe0, 0xd0), rgba(0xfc, 0xfc, 0x38),
    rgba(0x08, 0x80, 0x08),   rgba(0xfc, 0xfc, 0x7c),   rgba(0xb8, 0xb8, 0xe8), rgba(0xec, 0xc4, 0xb0),
    rgba(0x40, 0x68, 0xd4),   rgba(0x74, 0xa4, 0x7c),   rgba(0x90, 0x90, 0xb8), rgba(0x00, 0xe4, 0xfc),
};

std::uint32_t palette_rgba(int color) {
  const BWAPI::Color palette(BWAPI::Color(color).getID());
  return rgba(palette.red(), palette.green(), palette.blue());
}

}  // namespace

const BwrDrawList* DrawList::build(const BWAPI::GameData* data) {
  shapes_.clear();
  text_.clear();
  if (data == nullptr) return nullptr;
  const int count = std::clamp(data->shapeCount, 0, int{BWAPI::GameData::MAX_SHAPES});
  const int strings = std::clamp(data->stringCount, 0, int{BWAPI::GameData::MAX_STRINGS});
  for (int index = 0; index < count; ++index) {
    const BWAPIC::Shape& shape = data->shapes[index];
    BwrShape out{};
    out.kind = static_cast<std::uint8_t>(shape.type);
    out.coordinates = static_cast<std::uint8_t>(shape.ctype);
    out.solid = shape.isSolid ? 1 : 0;
    out.x1 = shape.x1;
    out.y1 = shape.y1;
    out.x2 = shape.x2;
    out.y2 = shape.y2;
    out.extra1 = shape.extra1;
    out.extra2 = shape.extra2;
    out.rgba = palette_rgba(shape.color);
    if (shape.type == BWAPIC::ShapeType::Text) {
      if (shape.extra1 < 0 || shape.extra1 >= strings || shape.extra2 < 0 || shape.extra2 > 3) continue;
      const char* text = data->strings[shape.extra1];
      const int length = static_cast<int>(strnlen(text, sizeof(data->strings[0])));
      out.text_size = static_cast<std::uint8_t>(shape.extra2);
      out.text_offset = static_cast<std::uint32_t>(text_.size());
      // Bots echo game strings such as computer player names, which are in
      // the system code page rather than UTF-8.
      if (length == 0 || MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, text, length, nullptr, 0) > 0) {
        text_.append(text, length);
      } else {
        const int wide = MultiByteToWideChar(CP_ACP, 0, text, length, nullptr, 0);
        wide_.resize(wide);
        MultiByteToWideChar(CP_ACP, 0, text, length, wide_.data(), wide);
        const int bytes = WideCharToMultiByte(CP_UTF8, 0, wide_.data(), wide, nullptr, 0, nullptr, nullptr);
        const std::size_t at = text_.size();
        text_.resize(at + bytes);
        WideCharToMultiByte(CP_UTF8, 0, wide_.data(), wide, text_.data() + at, bytes, nullptr, nullptr);
      }
      out.text_len = static_cast<std::uint32_t>(text_.size()) - out.text_offset;
    }
    shapes_.push_back(out);
  }
  list_.size = sizeof(BwrDrawList);
  list_.count = static_cast<std::uint32_t>(shapes_.size());
  list_.shapes = shapes_.data();
  list_.text = text_.data();
  list_.text_bytes = static_cast<std::uint32_t>(text_.size());
  std::memcpy(list_.text_colors, kTextColors, sizeof(list_.text_colors));
  return &list_;
}

}  // namespace BWRAPI::Legacy44
