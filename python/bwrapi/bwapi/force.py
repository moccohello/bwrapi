"""BWAPI Force over a GameData ForceData row (ForceImpl.cpp)."""

from __future__ import annotations

from typing import TYPE_CHECKING

from .containers import Playerset
from .gamedata import decode_text
from .interface import Interface

if TYPE_CHECKING:
    from .game import Game


class Force(Interface):
    """One of the 5 BWAPI force slots of a Game."""

    __slots__ = ("game", "_id", "_d")

    def __init__(self, game: Game, force_id: int) -> None:
        self.game = game
        self._id = force_id
        self._d = game._data.forces[force_id]
        self._client_info = None
        self._interface_events = None

    def __repr__(self) -> str:
        return f"Force({self._id}, {self.getName()!r})"

    def getID(self) -> int: return self._id
    def getName(self) -> str: return decode_text(self._d.name)

    def getPlayers(self) -> Playerset:
        players = self.game._data.players
        return Playerset(self.game.getPlayer(i) for i in range(len(players)) if players[i].force == self._id)
