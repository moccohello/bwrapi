"""BWAPI Bullet over a GameData BulletData row (BulletShared.cpp)."""

from __future__ import annotations

from typing import TYPE_CHECKING

from .interface import Interface
from .position import Position
from .types import BulletType

if TYPE_CHECKING:
    from .game import Game
    from .player import Player
    from .unit import Unit


class Bullet(Interface):
    """One of the 100 BWAPI bullet slots of a Game."""

    __slots__ = ("game", "_id", "_d")

    def __init__(self, game: Game, bullet_id: int) -> None:
        self.game = game
        self._id = bullet_id
        self._d = game._data.bullets[bullet_id]
        self._client_info = None
        self._interface_events = None

    def __repr__(self) -> str:
        return f"Bullet({self._id}, {self.getType()})"

    def getID(self) -> int: return self._d.id
    def getPlayer(self) -> Player | None: return self.game.getPlayer(self._d.player)
    def getType(self) -> BulletType: return BulletType(self._d.type)
    def getSource(self) -> Unit | None: return self.game.getUnit(self._d.source)
    def getPosition(self) -> Position: return Position(self._d.positionX, self._d.positionY)
    def getAngle(self) -> float: return self._d.angle
    def getVelocityX(self) -> float: return self._d.velocityX
    def getVelocityY(self) -> float: return self._d.velocityY
    def getTarget(self) -> Unit | None: return self.game.getUnit(self._d.target)
    def getTargetPosition(self) -> Position: return Position(self._d.targetPositionX, self._d.targetPositionY)
    def getRemoveTimer(self) -> int: return self._d.removeTimer
    def exists(self) -> bool: return self._d.exists

    def isVisible(self, player: Player | None = None) -> bool:
        visible = self._d.isVisible
        if player is None:
            player = self.game.self()
        if player is None:
            return any(visible)
        player_id = player.getID()
        return 0 <= player_id < 9 and visible[player_id]
