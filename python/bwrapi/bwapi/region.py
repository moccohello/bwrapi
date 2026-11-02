"""BWAPI Region over a GameData RegionData row (RegionImpl.cpp, RegionShared.cpp, Region.cpp)."""

from __future__ import annotations

from typing import TYPE_CHECKING

from .containers import Regionset, Unitset
from .filters import matches as _matches_predicate
from .interface import Interface
from .position import Position

if TYPE_CHECKING:
    from .game import Game


class Region(Interface):
    """A pathing region of the current match; the Game makes them at match start."""

    __slots__ = ("game", "_d", "_neighbors", "_closest_accessible", "_closest_inaccessible")

    def __init__(self, game: Game, index: int) -> None:
        self.game = game
        self._d = game._data.regions[index]
        self._neighbors = Regionset()
        self._closest_accessible = None
        self._closest_inaccessible = None
        self._client_info = None
        self._interface_events = None

    def _set_neighbors(self) -> None:
        accessible_best = inaccessible_best = 99999
        for i in range(self._d.neighborCount):
            neighbor = self.game.getRegion(self._d.neighbors[i])
            if neighbor is None:
                continue
            self._neighbors.add(neighbor)
            distance = self.getCenter().getApproxDistance(neighbor.getCenter())
            if self.getRegionGroupID() == neighbor.getRegionGroupID():
                if distance < accessible_best:
                    accessible_best = distance
                    self._closest_accessible = neighbor
            elif distance < inaccessible_best:
                inaccessible_best = distance
                self._closest_inaccessible = neighbor

    def __repr__(self) -> str:
        return f"Region({self.getID()})"

    def getID(self) -> int: return self._d.id
    def getRegionGroupID(self) -> int: return self._d.islandID
    def getCenter(self) -> Position: return Position(self._d.center_x, self._d.center_y)
    def isHigherGround(self) -> bool: return self._d.isHigherGround
    def getDefensePriority(self) -> int: return self._d.priority
    def isAccessible(self) -> bool: return self._d.isAccessible
    def getBoundsLeft(self) -> int: return self._d.leftMost
    def getBoundsTop(self) -> int: return self._d.topMost
    def getBoundsRight(self) -> int: return self._d.rightMost
    def getBoundsBottom(self) -> int: return self._d.bottomMost
    def getNeighbors(self) -> Regionset: return Regionset(self._neighbors)
    def getClosestAccessibleRegion(self) -> Region | None: return self._closest_accessible
    def getClosestInaccessibleRegion(self) -> Region | None: return self._closest_inaccessible

    def getDistance(self, other: Region | None) -> int:
        if other is None:
            return 2_147_483_647
        return self.getCenter().getApproxDistance(other.getCenter())

    def getUnits(self, pred=None) -> Unitset:
        return self.game.getUnitsInRectangle(
            self.getBoundsLeft(), self.getBoundsTop(), self.getBoundsRight(), self.getBoundsBottom(),
            lambda unit: unit.getRegion() is self and _matches_predicate(unit, pred))
