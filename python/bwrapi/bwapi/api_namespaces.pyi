from enum import IntEnum
from .position import Point as Point, Position as Position, TilePosition as TilePosition, WalkPosition as WalkPosition

class CoordinateType:
    class Enum(IntEnum):
        None_ = 0
        Screen = 1
        Map = 2
        Mouse = 3
    None_: Enum
    Screen: Enum
    Map: Enum
    Mouse: Enum

class EventType:
    class Enum(IntEnum):
        MatchStart = 0
        MatchEnd = 1
        MatchFrame = 2
        MenuFrame = 3
        SendText = 4
        ReceiveText = 5
        PlayerLeft = 6
        NukeDetect = 7
        UnitDiscover = 8
        UnitEvade = 9
        UnitShow = 10
        UnitHide = 11
        UnitCreate = 12
        UnitDestroy = 13
        UnitMorph = 14
        UnitRenegade = 15
        SaveGame = 16
        UnitComplete = 17
        None_ = 18
    MatchStart: Enum
    MatchEnd: Enum
    MatchFrame: Enum
    MenuFrame: Enum
    SendText: Enum
    ReceiveText: Enum
    PlayerLeft: Enum
    NukeDetect: Enum
    UnitDiscover: Enum
    UnitEvade: Enum
    UnitShow: Enum
    UnitHide: Enum
    UnitCreate: Enum
    UnitDestroy: Enum
    UnitMorph: Enum
    UnitRenegade: Enum
    SaveGame: Enum
    UnitComplete: Enum
    None_: Enum

class Flag:
    class Enum(IntEnum):
        CompleteMapInformation = 0
        UserInput = 1
        Max = 2
    CompleteMapInformation: Enum
    UserInput: Enum
    Max: Enum

class Latency:
    class Enum(IntEnum):
        SinglePlayer = 2
        LanLow = 5
        LanMedium = 7
        LanHigh = 9
        BattlenetLow = 14
        BattlenetMedium = 19
        BattlenetHigh = 24
    SinglePlayer: Enum
    LanLow: Enum
    LanMedium: Enum
    LanHigh: Enum
    BattlenetLow: Enum
    BattlenetMedium: Enum
    BattlenetHigh: Enum

class Positions:
    Origin: Position
    Invalid: Position
    None_: Position
    Unknown: Position

class TilePositions:
    Origin: TilePosition
    Invalid: TilePosition
    None_: TilePosition
    Unknown: TilePosition

class WalkPositions:
    Origin: WalkPosition
    Invalid: WalkPosition
    None_: WalkPosition
    Unknown: WalkPosition

TILE_SIZE: int
POSITION_SCALE: int
WALKPOSITION_SCALE: int
TILEPOSITION_SCALE: int
CLIENT_VERSION: int
