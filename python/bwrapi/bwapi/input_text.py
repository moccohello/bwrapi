"""BWAPI 4.4 input enums and text formatting controls."""

from enum import IntEnum
from operator import index as _index
from typing import SupportsIndex

_KEY_VALUES = {
    'K_LBUTTON': 1,
    'K_RBUTTON': 2,
    'K_CANCEL': 3,
    'K_MBUTTON': 4,
    'K_XBUTTON1': 5,
    'K_XBUTTON2': 6,
    '__UNDEFINED_7': 7,
    'K_BACK': 8,
    'K_TAB': 9,
    '__RESERVED_A': 10,
    '__RESERVED_B': 11,
    'K_CLEAR': 12,
    'K_RETURN': 13,
    '__UNDEFINED_E': 14,
    '__UNDEFINED_F': 15,
    'K_SHIFT': 16,
    'K_CONTROL': 17,
    'K_MENU': 18,
    'K_PAUSE': 19,
    'K_CAPITAL': 20,
    'K_KANA': 21,
    'K_UNDEFINED_16': 22,
    'K_JUNJA': 23,
    'K_FINAL': 24,
    'K_KANJI': 25,
    '__UNDEFINED_1A': 26,
    'K_ESCAPE': 27,
    'K_CONVERT': 28,
    'K_NONCONVERT': 29,
    'K_ACCEPT': 30,
    'K_MODECHANGE': 31,
    'K_SPACE': 32,
    'K_PRIOR': 33,
    'K_NEXT': 34,
    'K_END': 35,
    'K_HOME': 36,
    'K_LEFT': 37,
    'K_UP': 38,
    'K_RIGHT': 39,
    'K_DOWN': 40,
    'K_SELECT': 41,
    'K_PRINT': 42,
    'K_EXECUTE': 43,
    'K_SNAPSHOT': 44,
    'K_INSERT': 45,
    'K_DELETE': 46,
    'K_HELP': 47,
    'K_0': 48,
    'K_1': 49,
    'K_2': 50,
    'K_3': 51,
    'K_4': 52,
    'K_5': 53,
    'K_6': 54,
    'K_7': 55,
    'K_8': 56,
    'K_9': 57,
    '__UNDEFINED_3A': 58,
    '__UNDEFINED_3B': 59,
    '__UNDEFINED_3C': 60,
    '__UNDEFINED_3D': 61,
    '__UNDEFINED_3E': 62,
    '__UNDEFINED_3F': 63,
    '__UNDEFINED_40': 64,
    'K_A': 65,
    'K_B': 66,
    'K_C': 67,
    'K_D': 68,
    'K_E': 69,
    'K_F': 70,
    'K_G': 71,
    'K_H': 72,
    'K_I': 73,
    'K_J': 74,
    'K_K': 75,
    'K_L': 76,
    'K_M': 77,
    'K_N': 78,
    'K_O': 79,
    'K_P': 80,
    'K_Q': 81,
    'K_R': 82,
    'K_S': 83,
    'K_T': 84,
    'K_U': 85,
    'K_V': 86,
    'K_W': 87,
    'K_X': 88,
    'K_Y': 89,
    'K_Z': 90,
    'K_LWIN': 91,
    'K_RWIN': 92,
    'K_APPS': 93,
    '__RESERVED_5E': 94,
    'K_SLEEP': 95,
    'K_NUMPAD0': 96,
    'K_NUMPAD1': 97,
    'K_NUMPAD2': 98,
    'K_NUMPAD3': 99,
    'K_NUMPAD4': 100,
    'K_NUMPAD5': 101,
    'K_NUMPAD6': 102,
    'K_NUMPAD7': 103,
    'K_NUMPAD8': 104,
    'K_NUMPAD9': 105,
    'K_MULTIPLY': 106,
    'K_ADD': 107,
    'K_SEPARATOR': 108,
    'K_SUBTRACT': 109,
    'K_DECIMAL': 110,
    'K_DIVIDE': 111,
    'K_F1': 112,
    'K_F2': 113,
    'K_F3': 114,
    'K_F4': 115,
    'K_F5': 116,
    'K_F6': 117,
    'K_F7': 118,
    'K_F8': 119,
    'K_F9': 120,
    'K_F10': 121,
    'K_F11': 122,
    'K_F12': 123,
    'K_F13': 124,
    'K_F14': 125,
    'K_F15': 126,
    'K_F16': 127,
    'K_F17': 128,
    'K_F18': 129,
    'K_F19': 130,
    'K_F20': 131,
    'K_F21': 132,
    'K_F22': 133,
    'K_F23': 134,
    'K_F24': 135,
    '__UNASSIGNED_88': 136,
    '__UNASSIGNED_89': 137,
    '__UNASSIGNED_8A': 138,
    '__UNASSIGNED_8B': 139,
    '__UNASSIGNED_8C': 140,
    '__UNASSIGNED_8D': 141,
    '__UNASSIGNED_8E': 142,
    '__UNASSIGNED_8F': 143,
    'K_NUMLOCK': 144,
    'K_SCROLL': 145,
    'K_OEM_NEC_EQUAL': 146,
    'K_OEM_FJ_JISHO': 147,
    'K_OEM_FJ_MASSHOU': 148,
    'K_OEM_FJ_TOUROKU': 149,
    'K_OEM_FJ_LOYA': 150,
    '__UNASSIGNED_97': 151,
    '__UNASSIGNED_98': 152,
    '__UNASSIGNED_99': 153,
    '__UNASSIGNED_9A': 154,
    '__UNASSIGNED_9B': 155,
    '__UNASSIGNED_9C': 156,
    '__UNASSIGNED_9D': 157,
    '__UNASSIGNED_9E': 158,
    '__UNASSIGNED_9F': 159,
    'K_LSHIFT': 160,
    'K_RSHIFT': 161,
    'K_LCONTROL': 162,
    'K_RCONTROL': 163,
    'K_LMENU': 164,
    'K_RMENU': 165,
    'K_BROWSER_BACK': 166,
    'K_BROWSER_FORWARD': 167,
    'K_BROWSER_REFRESH': 168,
    'K_BROWSER_STOP': 169,
    'K_BROWSER_SEARCH': 170,
    'K_BROWSER_FAVORITES': 171,
    'K_BROWSER_HOME': 172,
    'K_VOLUME_MUTE': 173,
    'K_VOLUME_DOWN': 174,
    'K_VOLUME_UP': 175,
    'K_MEDIA_NEXT_TRACK': 176,
    'K_MEDIA_PREV_TRACK': 177,
    'K_MEDIA_STOP': 178,
    'K_MEDIA_PLAY_PAUSE': 179,
    'K_LAUNCH_MAIL': 180,
    'K_LAUNCH_MEDIA_SELECT': 181,
    'K_LAUNCH_APP1': 182,
    'K_LAUNCH_APP2': 183,
    '__RESERVED_B8': 184,
    '__RESERVED_B9': 185,
    'K_OEM_1': 186,
    'K_OEM_PLUS': 187,
    'K_OEM_COMMA': 188,
    'K_OEM_MINUS': 189,
    'K_OEM_PERIOD': 190,
    'K_OEM_2': 191,
    'K_OEM_3': 192,
    'K_OEM_4': 219,
    'K_OEM_5': 220,
    'K_OEM_6': 221,
    'K_OEM_7': 222,
    'K_OEM_8': 223,
    '__RESERVED_E0': 224,
    'K_OEM_AX': 225,
    'K_OEM_102': 226,
    'K_ICO_HELP': 227,
    'K_ICO_00': 228,
    'K_PROCESSKEY': 229,
    'K_ICO_CLEAR': 230,
    'K_PACKET': 231,
    '__UNASSIGNED_E8': 232,
    'K_OEM_RESET': 233,
    'K_OEM_JUMP': 234,
    'K_OEM_PA1': 235,
    'K_OEM_PA2': 236,
    'K_OEM_PA3': 237,
    'K_OEM_WSCTRL': 238,
    'K_OEM_CUSEL': 239,
    'K_OEM_ATTN': 240,
    'K_OEM_FINISH': 241,
    'K_OEM_COPY': 242,
    'K_OEM_AUTO': 243,
    'K_OEM_ENLW': 244,
    'K_OEM_BACKTAB': 245,
    'K_ATTN': 246,
    'K_CRSEL': 247,
    'K_EXSEL': 248,
    'K_EREOF': 249,
    'K_PLAY': 250,
    'K_ZOOM': 251,
    'K_NONAME': 252,
    'K_PA1': 253,
    'K_OEM_CLEAR': 254,
    'K_MAX': 255,
}

_MOUSE_BUTTON_VALUES = {
    'M_LEFT': 0,
    'M_RIGHT': 1,
    'M_MIDDLE': 2,
    'M_MAX': 3,
}

_TEXT_VALUES = {
    'Previous': 1,
    'Default': 2,
    'Yellow': 3,
    'White': 4,
    'Grey': 5,
    'Red': 6,
    'Green': 7,
    'BrightRed': 8,
    'Invisible': 11,
    'Blue': 14,
    'Teal': 15,
    'Purple': 16,
    'Orange': 17,
    'Align_Right': 18,
    'Align_Center': 19,
    'Invisible2': 20,
    'Brown': 21,
    'PlayerWhite': 22,
    'PlayerYellow': 23,
    'DarkGreen': 24,
    'LightYellow': 25,
    'Cyan': 26,
    'Tan': 27,
    'GreyBlue': 28,
    'GreyGreen': 29,
    'GreyCyan': 30,
    'Turquoise': 31,
}

_TEXT_SIZE_VALUES = {
    'Small': 0,
    'Default': 1,
    'Large': 2,
    'Huge': 3,
}


Key = IntEnum("Key", _KEY_VALUES, module=__name__)
MouseButton = IntEnum("MouseButton", _MOUSE_BUTTON_VALUES, module=__name__)
_TextEnum = IntEnum("Enum", _TEXT_VALUES, module=__name__, qualname="Text.Enum")
_TextSizeEnum = IntEnum("Enum", _TEXT_SIZE_VALUES, module=__name__, qualname="Text.Size.Enum")


class Text:
    """BWAPI text formatting namespace."""

    Enum = _TextEnum
    Previous = Enum.Previous
    Default = Enum.Default
    Yellow = Enum.Yellow
    White = Enum.White
    Grey = Enum.Grey
    Red = Enum.Red
    Green = Enum.Green
    BrightRed = Enum.BrightRed
    Invisible = Enum.Invisible
    Blue = Enum.Blue
    Teal = Enum.Teal
    Purple = Enum.Purple
    Orange = Enum.Orange
    Align_Right = Enum.Align_Right
    Align_Center = Enum.Align_Center
    Invisible2 = Enum.Invisible2
    Brown = Enum.Brown
    PlayerWhite = Enum.PlayerWhite
    PlayerYellow = Enum.PlayerYellow
    DarkGreen = Enum.DarkGreen
    LightYellow = Enum.LightYellow
    Cyan = Enum.Cyan
    Tan = Enum.Tan
    GreyBlue = Enum.GreyBlue
    GreyGreen = Enum.GreyGreen
    GreyCyan = Enum.GreyCyan
    Turquoise = Enum.Turquoise

    class Size:
        """BWAPI text size namespace."""

        Enum = _TextSizeEnum
        Small = Enum.Small
        Default = Enum.Default
        Large = Enum.Large
        Huge = Enum.Huge

    @staticmethod
    def isColor(value: SupportsIndex) -> bool:
        """Return whether an integer is a BWAPI text color control code."""
        code = _index(value)
        return 2 <= code <= 8 or 14 <= code <= 17 or 21 <= code <= 31


# BWAPI exposes these enum names as global constants as well as enum members.
for _name, _member in Key.__members__.items():
    if _name.startswith("K_"):
        globals()[_name] = _member
for _name, _member in MouseButton.__members__.items():
    if _name.startswith("M_"):
        globals()[_name] = _member

__all__ = [
    "Key",
    "MouseButton",
    "Text",
    *[name for name in Key.__members__ if name.startswith("K_")],
    *[name for name in MouseButton.__members__ if name.startswith("M_")],
]
