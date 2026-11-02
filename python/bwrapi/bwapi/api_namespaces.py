"""BWAPI 4.4 enums, owned Point values and read-only constants."""

from enum import IntEnum

from .position import Point, Position, TilePosition, WalkPosition, _readonly_copy
from .profile import BWAPI_CLIENT_VERSION
from .type_ids import CoordinateTypeId, EventTypeId, FlagId, LatencyId


_CoordinateEnum = IntEnum(
    "Enum",
    {
        "None": int(CoordinateTypeId.NONE),
        "Screen": int(CoordinateTypeId.SCREEN),
        "Map": int(CoordinateTypeId.MAP),
        "Mouse": int(CoordinateTypeId.MOUSE),
    },
    module=__name__,
    qualname="CoordinateType.Enum",
)

_EventEnum = IntEnum(
    "Enum",
    {
        "MatchStart": int(EventTypeId.MATCH_START),
        "MatchEnd": int(EventTypeId.MATCH_END),
        "MatchFrame": int(EventTypeId.MATCH_FRAME),
        "MenuFrame": int(EventTypeId.MENU_FRAME),
        "SendText": int(EventTypeId.SEND_TEXT),
        "ReceiveText": int(EventTypeId.RECEIVE_TEXT),
        "PlayerLeft": int(EventTypeId.PLAYER_LEFT),
        "NukeDetect": int(EventTypeId.NUKE_DETECT),
        "UnitDiscover": int(EventTypeId.UNIT_DISCOVER),
        "UnitEvade": int(EventTypeId.UNIT_EVADE),
        "UnitShow": int(EventTypeId.UNIT_SHOW),
        "UnitHide": int(EventTypeId.UNIT_HIDE),
        "UnitCreate": int(EventTypeId.UNIT_CREATE),
        "UnitDestroy": int(EventTypeId.UNIT_DESTROY),
        "UnitMorph": int(EventTypeId.UNIT_MORPH),
        "UnitRenegade": int(EventTypeId.UNIT_RENEGADE),
        "SaveGame": int(EventTypeId.SAVE_GAME),
        "UnitComplete": int(EventTypeId.UNIT_COMPLETE),
        "None": int(EventTypeId.NONE),
    },
    module=__name__,
    qualname="EventType.Enum",
)

_FlagEnum = IntEnum(
    "Enum",
    {
        "CompleteMapInformation": int(FlagId.COMPLETE_MAP_INFORMATION),
        "UserInput": int(FlagId.USER_INPUT),
        "Max": int(FlagId.MAX),
    },
    module=__name__,
    qualname="Flag.Enum",
)

_LatencyEnum = IntEnum(
    "Enum",
    {
        "SinglePlayer": int(LatencyId.SINGLE_PLAYER),
        "LanLow": int(LatencyId.LAN_LOW),
        "LanMedium": int(LatencyId.LAN_MEDIUM),
        "LanHigh": int(LatencyId.LAN_HIGH),
        "BattlenetLow": int(LatencyId.BATTLENET_LOW),
        "BattlenetMedium": int(LatencyId.BATTLENET_MEDIUM),
        "BattlenetHigh": int(LatencyId.BATTLENET_HIGH),
    },
    module=__name__,
    qualname="Latency.Enum",
)


class CoordinateType:
    """Namespace for BWAPI relative drawing coordinates."""

    Enum = _CoordinateEnum
    Screen = Enum.Screen
    Map = Enum.Map
    Mouse = Enum.Mouse


class EventType:
    """Namespace for BWAPI callback event kinds."""

    Enum = _EventEnum
    MatchStart = Enum.MatchStart
    MatchEnd = Enum.MatchEnd
    MatchFrame = Enum.MatchFrame
    MenuFrame = Enum.MenuFrame
    SendText = Enum.SendText
    ReceiveText = Enum.ReceiveText
    PlayerLeft = Enum.PlayerLeft
    NukeDetect = Enum.NukeDetect
    UnitDiscover = Enum.UnitDiscover
    UnitEvade = Enum.UnitEvade
    UnitShow = Enum.UnitShow
    UnitHide = Enum.UnitHide
    UnitCreate = Enum.UnitCreate
    UnitDestroy = Enum.UnitDestroy
    UnitMorph = Enum.UnitMorph
    UnitRenegade = Enum.UnitRenegade
    SaveGame = Enum.SaveGame
    UnitComplete = Enum.UnitComplete


class Flag:
    """Namespace for BWAPI game flags."""

    Enum = _FlagEnum
    CompleteMapInformation = Enum.CompleteMapInformation
    UserInput = Enum.UserInput
    Max = Enum.Max


class Latency:
    """Namespace for BWAPI latency values."""

    Enum = _LatencyEnum
    SinglePlayer = Enum.SinglePlayer
    LanLow = Enum.LanLow
    LanMedium = Enum.LanMedium
    LanHigh = Enum.LanHigh
    BattlenetLow = Enum.BattlenetLow
    BattlenetMedium = Enum.BattlenetMedium
    BattlenetHigh = Enum.BattlenetHigh


def _add_none_alias(enum_type: type[IntEnum], namespace: type) -> None:
    """Expose C++'s `None`, a reserved word in Python, as `None_` like the other BWAPI types."""

    member = enum_type.__members__["None"]
    setattr(enum_type, "None_", member)
    setattr(namespace, "None", member)
    setattr(namespace, "None_", member)


_add_none_alias(_CoordinateEnum, CoordinateType)
_add_none_alias(_EventEnum, EventType)


class Positions:
    """Special pixel-coordinate values from BWAPI Position.h."""

    Invalid = _readonly_copy(Position(32000, 32000))
    None_ = _readonly_copy(Position(32000, 32032))
    Unknown = _readonly_copy(Position(32000, 32064))
    Origin = _readonly_copy(Position(0, 0))


class WalkPositions:
    """Special walk-grid values from BWAPI Position.h."""

    Invalid = _readonly_copy(WalkPosition(4000, 4000))
    None_ = _readonly_copy(WalkPosition(4000, 4004))
    Unknown = _readonly_copy(WalkPosition(4000, 4008))
    Origin = _readonly_copy(WalkPosition(0, 0))


class TilePositions:
    """Special tile-coordinate values from BWAPI Position.h."""

    Invalid = _readonly_copy(TilePosition(1000, 1000))
    None_ = _readonly_copy(TilePosition(1000, 1001))
    Unknown = _readonly_copy(TilePosition(1000, 1002))
    Origin = _readonly_copy(TilePosition(0, 0))


for _position_namespace in (Positions, WalkPositions, TilePositions):
    setattr(_position_namespace, "None", _position_namespace.None_)


TILE_SIZE = 32
POSITION_SCALE = 1
WALKPOSITION_SCALE = 8
TILEPOSITION_SCALE = 32
CLIENT_VERSION = BWAPI_CLIENT_VERSION


__all__ = [
    "CLIENT_VERSION",
    "CoordinateType",
    "EventType",
    "Flag",
    "Latency",
    "Point",
    "Position",
    "POSITION_SCALE",
    "Positions",
    "TILE_SIZE",
    "TILEPOSITION_SCALE",
    "TilePosition",
    "TilePositions",
    "WalkPosition",
    "WALKPOSITION_SCALE",
    "WalkPositions",
]
