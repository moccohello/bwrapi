from __future__ import annotations

from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from .player import Player
    from .position import Position
    from .unit import Unit


class AIModule:
    """Base class of a BWAPI bot: override the BWAPI 4.4 callbacks you need.

    Every callback has the BWAPI name and arguments and does nothing by default.
    Use `Broodwar` inside them.
    """

    __slots__ = ()

    def onStart(self) -> None:
        """Called once at the start of a game (or on the first frame of a client that connects during one)."""

    def onEnd(self, isWinner: bool) -> None:
        """Called once at the end of a game; ``isWinner`` is False for a loss or a replay."""

    def onFrame(self) -> None:
        """Called for every logical frame; most bot code goes here."""

    def onSendText(self, text: str) -> None:
        """The user tried to send ``text``; not called while ``Flag.UserInput`` is off."""

    def onReceiveText(self, player: Player, text: str) -> None:
        """Another ``player`` sent ``text``; the local player's own messages never arrive here."""

    def onPlayerLeft(self, player: Player) -> None:
        """``player`` left the game; their units go to the neutral player."""

    def onNukeDetect(self, target: Position) -> None:
        """A nuke was launched at ``target``; ``Positions.Unknown`` if not visible and no ``Flag.CompleteMapInformation``."""

    def onUnitDiscover(self, unit: Unit) -> None:
        """``unit`` became accessible."""

    def onUnitEvade(self, unit: Unit) -> None:
        """``unit`` became inaccessible."""

    def onUnitShow(self, unit: Unit) -> None:
        """``unit`` became visible."""

    def onUnitHide(self, unit: Unit) -> None:
        """``unit`` is no longer visible."""

    def onUnitCreate(self, unit: Unit) -> None:
        """``unit`` was created; not for Zerg morphs or buildings placed on a geyser (see `onUnitMorph`)."""

    def onUnitDestroy(self, unit: Unit) -> None:
        """``unit`` died or was otherwise removed from the game."""

    def onUnitMorph(self, unit: Unit) -> None:
        """``unit`` changed its UnitType (a Drone into a Hatchery, a Siege Tank sieging, a Geyser into a Refinery)."""

    def onUnitRenegade(self, unit: Unit) -> None:
        """``unit`` changed ownership (Mind Control, Use Map Settings triggers)."""

    def onSaveGame(self, gameName: str) -> None:
        """The game state was saved to the file ``gameName``."""

    def onUnitComplete(self, unit: Unit) -> None:
        """``unit`` went from incomplete to complete."""
