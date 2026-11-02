from __future__ import annotations

from itertools import dropwhile
from operator import index
from typing import TYPE_CHECKING, ClassVar, TypeVar

from .position import Position, TilePosition

if TYPE_CHECKING:
    from typing import Self

    from .bullet import Bullet
    from .force import Force
    from .player import Player
    from .region import Region
    from .types import Race
    from .unit import Unit

_T = TypeVar("_T")


class SetContainer(set[_T]):
    """BWAPI SetContainer set semantics, including conditional erase."""

    def contains(self, value: _T) -> bool: return value in self
    def size(self) -> int: return len(self)
    def empty(self) -> bool: return not self
    def insert(self, value: _T):
        inserted = value not in self
        self.add(value)
        return dropwhile(lambda item: item != value, iter(self)), inserted

    def erase(self, value: _T) -> int:
        count = int(value in self)
        self.discard(value)
        return count
    def erase_if(self, predicate) -> None:
        self.difference_update(value for value in tuple(self) if predicate(value))
    def copy(self) -> Self: return type(self)(self)
    def __or__(self, other) -> Self: return type(self)(super().__or__(other))
    def __and__(self, other) -> Self: return type(self)(super().__and__(other))
    def __sub__(self, other) -> Self: return type(self)(super().__sub__(other))
    def __xor__(self, other) -> Self: return type(self)(super().__xor__(other))


class Unitset(SetContainer["Unit"]):
    none: ClassVar[Unitset]

    def setClientInfo(self, value, key=0) -> None:
        key = index(key)
        if not 0 <= key <= 255:
            return
        for unit in self:
            unit.setClientInfo(value, key)

    def getPosition(self) -> Position:
        # Unitset::getPosition: the mean of the valid positions.
        positions = [position for position in (unit.getPosition() for unit in self) if position.isValid()]
        if not positions:
            return Position(0, 0)
        return Position(sum(p.x for p in positions) // len(positions), sum(p.y for p in positions) // len(positions))

    def getLoadedUnits(self) -> Unitset: return Unitset(v for u in self for v in u.getLoadedUnits())
    def getInterceptors(self) -> Unitset: return Unitset(v for u in self for v in u.getInterceptors())
    def getLarva(self) -> Unitset: return Unitset(v for u in self for v in u.getLarva())

    def _game(self):
        return next(iter(self)).game

    def getUnitsInRadius(self, radius, pred=None) -> Unitset:
        return Unitset() if not self else self._game().getUnitsInRadius(self.getPosition(), radius, pred)

    def getClosestUnit(self, pred=None, radius=999999) -> Unit | None:
        return None if not self else self._game().getClosestUnit(self.getPosition(), pred, radius)

    def issueCommand(self, command) -> bool:
        return self._game().issueCommand(self, command) if self else False

    def _issue(self, factory, *args) -> bool:
        from .unit_command import UnitCommand

        return self.issueCommand(getattr(UnitCommand, factory)(None, *args))

    def attack(self, target, queued=False) -> bool: return self._issue("attack", target, queued)

    def build(self, type_id, target_position=None) -> bool:
        if target_position is None or TilePosition(target_position) == TilePosition(32000 // 32, 32032 // 32):
            return self.train(type_id)
        return self._issue("build", target_position, type_id)

    def buildAddon(self, type_id) -> bool:
        result = False
        for unit in self:
            result = unit.buildAddon(type_id) or result
        return result

    def train(self, type_id) -> bool: return self._issue("train", type_id)
    def morph(self, type_id) -> bool: return self._issue("morph", type_id)
    def setRallyPoint(self, target) -> bool: return self._issue("setRallyPoint", target)
    def move(self, target, queued=False) -> bool: return self._issue("move", target, queued)
    def patrol(self, target, queued=False) -> bool: return self._issue("patrol", target, queued)
    def holdPosition(self, queued=False) -> bool: return self._issue("holdPosition", queued)
    def stop(self, queued=False) -> bool: return self._issue("stop", queued)
    def follow(self, target, queued=False) -> bool: return self._issue("follow", target, queued)
    def gather(self, target, queued=False) -> bool: return self._issue("gather", target, queued)
    def returnCargo(self, queued=False) -> bool: return self._issue("returnCargo", queued)
    def repair(self, target, queued=False) -> bool: return self._issue("repair", target, queued)
    def burrow(self) -> bool: return self._issue("burrow")
    def unburrow(self) -> bool: return self._issue("unburrow")
    def cloak(self) -> bool: return self._issue("cloak")
    def decloak(self) -> bool: return self._issue("decloak")
    def siege(self) -> bool: return self._issue("siege")
    def unsiege(self) -> bool: return self._issue("unsiege")
    def lift(self) -> bool: return self._issue("lift")
    def load(self, target, queued=False) -> bool: return self._issue("load", target, queued)

    def unloadAll(self, target=None, queued=False) -> bool:
        if type(target) is bool:
            queued, target = target, None
        return self._issue("unloadAll", target, queued)

    def rightClick(self, target, queued=False) -> bool: return self._issue("rightClick", target, queued)
    def haltConstruction(self) -> bool: return self._issue("haltConstruction")
    def cancelConstruction(self) -> bool: return self._issue("cancelConstruction")
    def cancelAddon(self) -> bool: return self._issue("cancelAddon")
    def cancelTrain(self, slot=-2) -> bool: return self._issue("cancelTrain", slot)
    def cancelMorph(self) -> bool: return self._issue("cancelMorph")
    def cancelResearch(self) -> bool: return self._issue("cancelResearch")
    def cancelUpgrade(self) -> bool: return self._issue("cancelUpgrade")

    def useTech(self, tech, target=None) -> bool:
        if target is not None:
            return self._issue("useTech", tech, target)
        # Burrow, cloak and siege toggle on each unit's own state.
        result = False
        for unit in self:
            result = unit.useTech(tech) or result
        return result


class _ConstUnitset(Unitset):
    def _immutable(self, *args, **kwargs):
        raise TypeError("Unitset.none is a constant")

    add = update = clear = discard = remove = pop = _immutable
    difference_update = intersection_update = symmetric_difference_update = _immutable
    __ior__ = __iand__ = __isub__ = __ixor__ = _immutable
    insert = erase = erase_if = _immutable

    def copy(self) -> Unitset: return Unitset(self)
    def __or__(self, other) -> Unitset: return Unitset(set.__or__(self, other))
    def __and__(self, other) -> Unitset: return Unitset(set.__and__(self, other))
    def __sub__(self, other) -> Unitset: return Unitset(set.__sub__(self, other))
    def __xor__(self, other) -> Unitset: return Unitset(set.__xor__(self, other))


Unitset.none = _ConstUnitset()


class Playerset(SetContainer["Player"]):
    def getUnits(self) -> Unitset: return Unitset(unit for player in self for unit in player.getUnits())
    def getRaces(self) -> SetContainer[Race]: return SetContainer(player.getRace() for player in self)
    def setAlliance(self, allies=True, alliedVictory=True) -> None:
        for player in self:
            player.game.setAlliance(player, allies, alliedVictory)


class Forceset(SetContainer["Force"]):
    def getPlayers(self) -> Playerset: return Playerset(player for force in self for player in force.getPlayers())


class Regionset(SetContainer["Region"]):
    def getCenter(self) -> Position:
        positions = [center for center in (region.getCenter() for region in self) if center.isValid()]
        if not positions:
            return Position(0, 0)
        return Position(sum(p.x for p in positions) // len(positions), sum(p.y for p in positions) // len(positions))

    def getUnits(self, pred=None) -> Unitset:
        return Unitset(unit for region in self for unit in region.getUnits(pred))


class Bulletset(SetContainer["Bullet"]):
    pass
