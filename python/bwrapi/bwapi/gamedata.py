"""ctypes definitions of the BWAPI 4.4 shared memory layout (`BWAPI/Client/*Data.h`).

The server (x64 and x86 MSVC) and the client use the same byte layout. Fields are
only int, bool, double, unsigned short and arrays of them, so natural alignment is
the same on x64 and x86 MSVC and on Linux x86-64. The tests compile the official
headers and compare every field.
"""

from __future__ import annotations

import sys
from ctypes import Structure, c_bool, c_char, c_double, c_int32, c_uint16, c_uint32

UNIT_TYPE_MAX = 234
TECH_TYPE_MAX = 47
UPGRADE_TYPE_MAX = 63
FLAG_MAX = 2
MOUSE_MAX = 3
KEY_MAX = 255

MAX_FORCES = 5
MAX_PLAYERS = 12
MAX_UNITS = 10000
MAX_UNIT_INDEX = 1700
MAX_BULLETS = 100
MAX_NUKE_DOTS = 200
MAX_REGIONS = 5000
MAX_EVENTS = 10000
MAX_EVENT_STRINGS = 1000
MAX_STRINGS = 20000
MAX_SHAPES = 20000
MAX_COMMANDS = 20000
MAX_UNIT_COMMANDS = 20000

GAME_TABLE_NAME = "Local\\bwapi_shared_memory_game_list"
MAX_GAME_INSTANCES = 8


def game_data_name(server_pid: int) -> str:
    return f"Local\\bwapi_shared_memory_{server_pid}"


def decode_text(raw: bytes) -> str:
    """Text from the game: UTF-8, or the system code page (a Korean client's computer names are CP949)."""
    try:
        return raw.decode("utf-8")
    except UnicodeDecodeError:
        return raw.decode("mbcs" if sys.platform == "win32" else "latin-1", "replace")


def pipe_name(server_pid: int) -> str:
    return f"\\\\.\\pipe\\bwapi_pipe_{server_pid}"


class GameInstance(Structure):
    _fields_ = [
        ("serverProcessID", c_uint32),
        ("isConnected", c_bool),
        ("lastKeepAliveTime", c_uint32),
    ]


class GameTable(Structure):
    _fields_ = [("gameInstances", GameInstance * MAX_GAME_INSTANCES)]


class ForceData(Structure):
    _fields_ = [("name", c_char * 32)]


class PlayerData(Structure):
    _fields_ = [
        ("name", c_char * 25),
        ("race", c_int32),
        ("type", c_int32),
        ("force", c_int32),
        ("isAlly", c_bool * 12),
        ("isEnemy", c_bool * 12),
        ("isNeutral", c_bool),
        ("startLocationX", c_int32),
        ("startLocationY", c_int32),
        ("isVictorious", c_bool),
        ("isDefeated", c_bool),
        ("leftGame", c_bool),
        ("isParticipating", c_bool),
        ("minerals", c_int32),
        ("gas", c_int32),
        ("gatheredMinerals", c_int32),
        ("gatheredGas", c_int32),
        ("repairedMinerals", c_int32),
        ("repairedGas", c_int32),
        ("refundedMinerals", c_int32),
        ("refundedGas", c_int32),
        ("supplyTotal", c_int32 * 3),
        ("supplyUsed", c_int32 * 3),
        ("allUnitCount", c_int32 * UNIT_TYPE_MAX),
        ("visibleUnitCount", c_int32 * UNIT_TYPE_MAX),
        ("completedUnitCount", c_int32 * UNIT_TYPE_MAX),
        ("deadUnitCount", c_int32 * UNIT_TYPE_MAX),
        ("killedUnitCount", c_int32 * UNIT_TYPE_MAX),
        ("upgradeLevel", c_int32 * UPGRADE_TYPE_MAX),
        ("hasResearched", c_bool * TECH_TYPE_MAX),
        ("isResearching", c_bool * TECH_TYPE_MAX),
        ("isUpgrading", c_bool * UPGRADE_TYPE_MAX),
        ("color", c_int32),
        ("totalUnitScore", c_int32),
        ("totalKillScore", c_int32),
        ("totalBuildingScore", c_int32),
        ("totalRazingScore", c_int32),
        ("customScore", c_int32),
        ("maxUpgradeLevel", c_int32 * UPGRADE_TYPE_MAX),
        ("isResearchAvailable", c_bool * TECH_TYPE_MAX),
        ("isUnitAvailable", c_bool * UNIT_TYPE_MAX),
    ]


_UNIT_INTS = (
    "clearanceLevel id player type positionX positionY"
).split()
_UNIT_INTS_2 = (
    "hitPoints lastHitPoints shields energy resources resourceGroup "
    "killCount acidSporeCount scarabCount interceptorCount spiderMineCount "
    "groundWeaponCooldown airWeaponCooldown spellCooldown defenseMatrixPoints "
    "defenseMatrixTimer ensnareTimer irradiateTimer lockdownTimer maelstromTimer "
    "orderTimer plagueTimer removeTimer stasisTimer stimTimer "
    "buildType trainingQueueCount"
).split()
_UNIT_INTS_3 = (
    "tech upgrade remainingBuildTime remainingTrainTime remainingResearchTime "
    "remainingUpgradeTime buildUnit target targetPositionX targetPositionY order "
    "orderTarget orderTargetPositionX orderTargetPositionY secondaryOrder "
    "rallyPositionX rallyPositionY rallyUnit addon nydusExit powerUp "
    "transport carrier hatchery"
).split()
_UNIT_BOOLS = "exists hasNuke isAccelerating isAttacking isAttackFrame isBeingGathered isBlind isBraking isBurrowed".split()
_UNIT_BOOLS_2 = (
    "isCloaked isCompleted isConstructing isDetected isGathering isHallucination "
    "isIdle isInterruptible isInvincible isLifted isMorphing isMoving isParasited "
    "isSelected isStartingAttack isStuck isTraining isUnderStorm isUnderDarkSwarm "
    "isUnderDWeb isPowered"
).split()


class UnitData(Structure):
    _fields_ = [
        *((name, c_int32) for name in _UNIT_INTS),
        ("angle", c_double),
        ("velocityX", c_double),
        ("velocityY", c_double),
        *((name, c_int32) for name in _UNIT_INTS_2),
        ("trainingQueue", c_int32 * 5),
        *((name, c_int32) for name in _UNIT_INTS_3),
        *((name, c_bool) for name in _UNIT_BOOLS),
        ("carryResourceType", c_int32),
        *((name, c_bool) for name in _UNIT_BOOLS_2),
        ("isVisible", c_bool * 9),
        ("buttonset", c_int32),
        ("lastAttackerPlayer", c_int32),
        ("recentlyAttacked", c_bool),
        ("replayID", c_int32),
    ]


class BulletData(Structure):
    _fields_ = [
        ("id", c_int32),
        ("player", c_int32),
        ("type", c_int32),
        ("source", c_int32),
        ("positionX", c_int32),
        ("positionY", c_int32),
        ("angle", c_double),
        ("velocityX", c_double),
        ("velocityY", c_double),
        ("target", c_int32),
        ("targetPositionX", c_int32),
        ("targetPositionY", c_int32),
        ("removeTimer", c_int32),
        ("exists", c_bool),
        ("isVisible", c_bool * 9),
    ]


class RegionData(Structure):
    _fields_ = [
        ("id", c_int32),
        ("islandID", c_int32),
        ("center_x", c_int32),
        ("center_y", c_int32),
        ("priority", c_int32),
        ("leftMost", c_int32),
        ("rightMost", c_int32),
        ("topMost", c_int32),
        ("bottomMost", c_int32),
        ("neighborCount", c_int32),
        ("neighbors", c_int32 * 256),
        ("isAccessible", c_bool),
        ("isHigherGround", c_bool),
    ]


class Point(Structure):
    _fields_ = [("x", c_int32), ("y", c_int32)]


class Event(Structure):
    _fields_ = [("type", c_int32), ("v1", c_int32), ("v2", c_int32)]


class Shape(Structure):
    _fields_ = [
        ("type", c_int32),
        ("ctype", c_int32),
        ("x1", c_int32),
        ("y1", c_int32),
        ("x2", c_int32),
        ("y2", c_int32),
        ("extra1", c_int32),
        ("extra2", c_int32),
        ("color", c_int32),
        ("isSolid", c_bool),
    ]


class Command(Structure):
    _fields_ = [("type", c_int32), ("value1", c_int32), ("value2", c_int32)]


class UnitCommand(Structure):
    _fields_ = [
        ("type", c_int32),
        ("unitIndex", c_int32),
        ("targetIndex", c_int32),
        ("x", c_int32),
        ("y", c_int32),
        ("extra", c_int32),
    ]


class UnitFinder(Structure):
    _fields_ = [("unitIndex", c_int32), ("searchValue", c_int32)]


class GameData(Structure):
    _fields_ = [
        ("client_version", c_int32),
        ("revision", c_int32),
        ("isDebug", c_bool),
        ("instanceID", c_int32),
        ("botAPM_noselects", c_int32),
        ("botAPM_selects", c_int32),
        ("forceCount", c_int32),
        ("forces", ForceData * MAX_FORCES),
        ("playerCount", c_int32),
        ("players", PlayerData * MAX_PLAYERS),
        ("initialUnitCount", c_int32),
        ("units", UnitData * MAX_UNITS),
        ("unitArray", c_int32 * MAX_UNIT_INDEX),
        ("bullets", BulletData * MAX_BULLETS),
        ("nukeDotCount", c_int32),
        ("nukeDots", Point * MAX_NUKE_DOTS),
        ("gameType", c_int32),
        ("latency", c_int32),
        ("latencyFrames", c_int32),
        ("latencyTime", c_int32),
        ("remainingLatencyFrames", c_int32),
        ("remainingLatencyTime", c_int32),
        ("hasLatCom", c_bool),
        ("hasGUI", c_bool),
        ("replayFrameCount", c_int32),
        ("randomSeed", c_uint32),
        ("frameCount", c_int32),
        ("elapsedTime", c_int32),
        ("countdownTimer", c_int32),
        ("fps", c_int32),
        ("averageFPS", c_double),
        ("mouseX", c_int32),
        ("mouseY", c_int32),
        ("mouseState", c_bool * MOUSE_MAX),
        ("keyState", c_bool * KEY_MAX),
        ("screenX", c_int32),
        ("screenY", c_int32),
        ("flags", c_bool * FLAG_MAX),
        ("mapWidth", c_int32),
        ("mapHeight", c_int32),
        ("mapFileName", c_char * 261),
        ("mapPathName", c_char * 261),
        ("mapName", c_char * 33),
        ("mapHash", c_char * 41),
        ("getGroundHeight", (c_int32 * 256) * 256),
        ("isWalkable", (c_bool * 1024) * 1024),
        ("isBuildable", (c_bool * 256) * 256),
        ("isVisible", (c_bool * 256) * 256),
        ("isExplored", (c_bool * 256) * 256),
        ("hasCreep", (c_bool * 256) * 256),
        ("isOccupied", (c_bool * 256) * 256),
        ("mapTileRegionId", (c_uint16 * 256) * 256),
        ("mapSplitTilesMiniTileMask", c_uint16 * 5000),
        ("mapSplitTilesRegion1", c_uint16 * 5000),
        ("mapSplitTilesRegion2", c_uint16 * 5000),
        ("regionCount", c_int32),
        ("regions", RegionData * MAX_REGIONS),
        ("startLocationCount", c_int32),
        ("startLocations", Point * 8),
        ("isInGame", c_bool),
        ("isMultiplayer", c_bool),
        ("isBattleNet", c_bool),
        ("isPaused", c_bool),
        ("isReplay", c_bool),
        ("selectedUnitCount", c_int32),
        ("selectedUnits", c_int32 * 12),
        ("self", c_int32),
        ("enemy", c_int32),
        ("neutral", c_int32),
        ("eventCount", c_int32),
        ("events", Event * MAX_EVENTS),
        ("eventStringCount", c_int32),
        ("eventStrings", (c_char * 256) * MAX_EVENT_STRINGS),
        ("stringCount", c_int32),
        ("strings", (c_char * 1024) * MAX_STRINGS),
        ("shapeCount", c_int32),
        ("shapes", Shape * MAX_SHAPES),
        ("commandCount", c_int32),
        ("commands", Command * MAX_COMMANDS),
        ("unitCommandCount", c_int32),
        ("unitCommands", UnitCommand * MAX_UNIT_COMMANDS),
        ("unitSearchSize", c_int32),
        ("xUnitSearch", UnitFinder * (MAX_UNIT_INDEX * 2)),
        ("yUnitSearch", UnitFinder * (MAX_UNIT_INDEX * 2)),
    ]


# BWAPIC::CommandType::Enum
class CommandType:
    None_ = 0
    SetScreenPosition = 1
    PingMinimap = 2
    EnableFlag = 3
    Printf = 4
    SendText = 5
    PauseGame = 6
    ResumeGame = 7
    LeaveGame = 8
    RestartGame = 9
    SetLocalSpeed = 10
    SetLatCom = 11
    SetGui = 12
    SetFrameSkip = 13
    SetMap = 14
    SetAllies = 15
    SetVision = 16
    SetCommandOptimizerLevel = 17
    SetRevealAll = 18


# BWAPIC::ShapeType::Enum
class ShapeType:
    None_ = 0
    Text = 1
    Box = 2
    Triangle = 3
    Circle = 4
    Ellipse = 5
    Dot = 6
    Line = 7
