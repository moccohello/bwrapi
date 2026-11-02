from __future__ import annotations

from operator import index

from .global_game import _get_current_game
from .position import Position, TilePosition, WalkPosition
from .types import UnitCommandType, UnitType, TechType, UpgradeType


_NONE_X, _NONE_Y = 32000, 32032
_QUEUED_TYPES = frozenset((0, 1, 10, 11, 12, 13, 14, 15, 16, 17, 26, 28, 29, 30, 31))
_TILE_TYPES = frozenset((2, 25, 43))
_UNIT_TYPE_TYPES = frozenset((2, 3, 4, 5))
_TECH_TYPE_TYPES = frozenset((6, 40, 41, 42))
_ACTOR_UNSET = object()


def _xy(point):
    if isinstance(point, tuple) and len(point) == 2:
        return index(point[0]), index(point[1])
    return index(point.x), index(point.y)


def _trunc_div(value, divisor):
    value = index(value)
    return value // divisor if value >= 0 else -((-value) // divisor)


def _is_unit_target(target):
    from .unit import Unit

    return target is None or isinstance(target, Unit)


class UnitCommand:
    """A mutable BWAPI UnitCommand value. Creating one does not issue a game command."""

    __slots__ = ("unit", "type", "target", "x", "y", "extra")
    __hash__ = None

    def __init__(self, unit=None, type=None, target=None, x=_NONE_X, y=_NONE_Y, extra=0):
        if isinstance(unit, UnitCommand):
            if type is not None or target is not None or (x, y, extra) != (_NONE_X, _NONE_Y, 0):
                raise TypeError("UnitCommand copy construction accepts only the source value")
            self.unit, self.type, self.target = unit.unit, unit.type, unit.target
            self.x, self.y, self.extra = unit.x, unit.y, unit.extra
            return
        self.unit = unit
        self.type = UnitCommandType(type)
        self.target = target
        self.x = index(x)
        self.y = index(y)
        self.extra = index(extra)

    def __copy__(self):
        return type(self)(self.unit, self.type, self.target, self.x, self.y, self.extra)

    def __deepcopy__(self, memo):
        copied = self.__copy__()
        memo[id(self)] = copied
        return copied

    def __eq__(self, other):
        if not isinstance(other, UnitCommand):
            return NotImplemented
        return (self.type == other.type and self.target is other.target
                and (self.x, self.y, self.extra) == (other.x, other.y, other.extra))

    def __ne__(self, other):
        result = self.__eq__(other)
        return NotImplemented if result is NotImplemented else not result

    def assignTarget(self, target):
        x, y = _xy(target)
        scale = 32 if isinstance(target, TilePosition) else 8 if isinstance(target, WalkPosition) else 1
        game = _get_current_game()
        width, height = (256, 256) if game is None else (game.mapWidth(), game.mapHeight())
        self.x = min(max(x, 0), index(width) * 32 // scale - 1)
        self.y = min(max(y, 0), index(height) * 32 // scale - 1)

    @classmethod
    def _position(cls, unit, type_id, target, extra=0):
        command = cls(unit, type_id, extra=extra)
        point_type = TilePosition if type_id in _TILE_TYPES else Position
        value = point_type(*target) if isinstance(target, tuple) else point_type(target)
        command.assignTarget(value)
        return command

    @classmethod
    def _unit_target(cls, unit, type_id, target, extra=0):
        return cls(unit, type_id, target=target, extra=extra)

    @classmethod
    def attack(cls, unit, target, shiftQueueCommand=False):
        if _is_unit_target(target):
            return cls._unit_target(unit, 1, target, int(bool(shiftQueueCommand)))
        return cls._position(unit, 0, target, int(bool(shiftQueueCommand)))

    @classmethod
    def build(cls, unit, target, type):
        return cls._position(unit, 2, target, index(type))

    @classmethod
    def buildAddon(cls, unit, type): return cls(unit, 3, extra=index(type))
    @classmethod
    def train(cls, unit, type): return cls(unit, 4, extra=index(type))
    @classmethod
    def morph(cls, unit, type): return cls(unit, 5, extra=index(type))
    @classmethod
    def research(cls, unit, tech): return cls(unit, 6, extra=index(tech))
    @classmethod
    def upgrade(cls, unit, upgrade): return cls(unit, 7, extra=index(upgrade))

    @classmethod
    def setRallyPoint(cls, unit, target):
        return (cls._unit_target(unit, 9, target) if _is_unit_target(target)
                else cls._position(unit, 8, target))

    @classmethod
    def move(cls, unit, target, shiftQueueCommand=False):
        return cls._position(unit, 10, target, int(bool(shiftQueueCommand)))
    @classmethod
    def patrol(cls, unit, target, shiftQueueCommand=False):
        return cls._position(unit, 11, target, int(bool(shiftQueueCommand)))
    @classmethod
    def holdPosition(cls, unit, shiftQueueCommand=False): return cls(unit, 12, extra=int(bool(shiftQueueCommand)))
    @classmethod
    def stop(cls, unit, shiftQueueCommand=False): return cls(unit, 13, extra=int(bool(shiftQueueCommand)))
    @classmethod
    def follow(cls, unit, target, shiftQueueCommand=False): return cls._unit_target(unit, 14, target, int(bool(shiftQueueCommand)))
    @classmethod
    def gather(cls, unit, target, shiftQueueCommand=False): return cls._unit_target(unit, 15, target, int(bool(shiftQueueCommand)))
    @classmethod
    def returnCargo(cls, unit, shiftQueueCommand=False): return cls(unit, 16, extra=int(bool(shiftQueueCommand)))
    @classmethod
    def repair(cls, unit, target, shiftQueueCommand=False): return cls._unit_target(unit, 17, target, int(bool(shiftQueueCommand)))
    @classmethod
    def burrow(cls, unit): return cls(unit, 18)
    @classmethod
    def unburrow(cls, unit): return cls(unit, 19)
    @classmethod
    def cloak(cls, unit): return cls(unit, 20)
    @classmethod
    def decloak(cls, unit): return cls(unit, 21)
    @classmethod
    def siege(cls, unit): return cls(unit, 22)
    @classmethod
    def unsiege(cls, unit): return cls(unit, 23)
    @classmethod
    def lift(cls, unit): return cls(unit, 24)
    @classmethod
    def land(cls, unit, target): return cls._position(unit, 25, target)
    @classmethod
    def load(cls, unit, target, shiftQueueCommand=False): return cls._unit_target(unit, 26, target, int(bool(shiftQueueCommand)))
    @classmethod
    def unload(cls, unit, target): return cls._unit_target(unit, 27, target)

    @classmethod
    def unloadAll(cls, unit, target=None, shiftQueueCommand=False):
        if isinstance(target, bool):
            return cls(unit, 28, extra=int(target))
        if target is None:
            return cls(unit, 28, extra=int(bool(shiftQueueCommand)))
        return cls._position(unit, 29, target, int(bool(shiftQueueCommand)))

    @classmethod
    def rightClick(cls, unit, target, shiftQueueCommand=False):
        if _is_unit_target(target):
            return cls._unit_target(unit, 31, target, int(bool(shiftQueueCommand)))
        return cls._position(unit, 30, target, int(bool(shiftQueueCommand)))

    @classmethod
    def haltConstruction(cls, unit): return cls(unit, 32)
    @classmethod
    def cancelConstruction(cls, unit): return cls(unit, 33)
    @classmethod
    def cancelAddon(cls, unit): return cls(unit, 34)
    @classmethod
    def cancelTrain(cls, unit, slot=-2): return cls(unit, 36 if index(slot) >= 0 else 35, extra=index(slot))
    @classmethod
    def cancelMorph(cls, unit): return cls(unit, 37)
    @classmethod
    def cancelResearch(cls, unit): return cls(unit, 38)
    @classmethod
    def cancelUpgrade(cls, unit): return cls(unit, 39)

    @classmethod
    def useTech(cls, unit, tech, target=_ACTOR_UNSET):
        tech = index(tech)
        if target is not _ACTOR_UNSET:
            return (cls._unit_target(unit, 42, target, tech) if _is_unit_target(target)
                    else cls._position(unit, 41, target, tech))
        type_id = 40
        if tech == 11:
            type_id = 19 if unit.isBurrowed() else 18
        elif tech in (9, 10):
            type_id = 21 if unit.isCloaked() else 20
        elif tech == 5:
            type_id = 23 if unit.isSieged() else 22
        return cls(unit, type_id, extra=tech)

    @classmethod
    def placeCOP(cls, unit, target): return cls._position(unit, 43, target)

    def getType(self): return UnitCommandType(self.type)
    def getUnit(self): return self.unit
    def getTarget(self): return self.target
    def getTargetPosition(self):
        return Position(self.x * 32, self.y * 32) if self.type in _TILE_TYPES else Position(self.x, self.y)
    def getTargetTilePosition(self):
        return (TilePosition(self.x, self.y) if self.type in _TILE_TYPES
                else TilePosition(_trunc_div(self.x, 32), _trunc_div(self.y, 32)))
    def getUnitType(self): return UnitType(self.extra) if self.type in _UNIT_TYPE_TYPES else UnitType()
    def getTechType(self): return TechType(self.extra) if self.type in _TECH_TYPE_TYPES else TechType()
    def getUpgradeType(self): return UpgradeType(self.extra) if self.type == 7 else UpgradeType()
    def getSlot(self): return self.extra if self.type == 36 else -1
    def isQueued(self): return bool(self.extra) if self.type in _QUEUED_TYPES else False
