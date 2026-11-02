"""Runs a Python AIModule as a BWAPI client.

The in-game server already turns each frame into BWAPI events; the runner
hands the frame back, takes the next one and calls the module's callback for
each event in the order the server staged them, as BWAPI calls an AIModule.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from .aimodule import AIModule
from .client import Client, ServerConnectionError
from .type_ids import EventTypeId

if TYPE_CHECKING:
    from .game import Game

_UNIT_CALLBACKS = {
    EventTypeId.UNIT_DISCOVER: "onUnitDiscover",
    EventTypeId.UNIT_EVADE: "onUnitEvade",
    EventTypeId.UNIT_SHOW: "onUnitShow",
    EventTypeId.UNIT_HIDE: "onUnitHide",
    EventTypeId.UNIT_CREATE: "onUnitCreate",
    EventTypeId.UNIT_DESTROY: "onUnitDestroy",
    EventTypeId.UNIT_MORPH: "onUnitMorph",
    EventTypeId.UNIT_RENEGADE: "onUnitRenegade",
    EventTypeId.UNIT_COMPLETE: "onUnitComplete",
}


def dispatch(module: AIModule, event) -> None:
    """Calls the AIModule callback of one BWAPI event."""
    kind = event.getType()
    if kind == EventTypeId.MATCH_FRAME:
        module.onFrame()
    elif kind in _UNIT_CALLBACKS:
        getattr(module, _UNIT_CALLBACKS[kind])(event.getUnit())
    elif kind == EventTypeId.MATCH_START:
        module.onStart()
    elif kind == EventTypeId.MATCH_END:
        module.onEnd(event.isWinner())
    elif kind == EventTypeId.SEND_TEXT:
        module.onSendText(event.getText())
    elif kind == EventTypeId.RECEIVE_TEXT:
        module.onReceiveText(event.getPlayer(), event.getText())
    elif kind == EventTypeId.PLAYER_LEFT:
        module.onPlayerLeft(event.getPlayer())
    elif kind == EventTypeId.NUKE_DETECT:
        module.onNukeDetect(event.getPosition())
    elif kind == EventTypeId.SAVE_GAME:
        module.onSaveGame(event.getText())


class BwapiModuleRunner:
    """Drives one AIModule from one connected `Client`, frame by frame.

        client = connect(wait=math.inf)
        runner = BwapiModuleRunner(client, MyBot())
        try:
            runner.run()
        finally:
            runner.close()
    """

    def __init__(self, client: Client, module: AIModule) -> None:
        self.client = client
        self.module = module

    @property
    def game(self) -> Game | None:
        return self.client.game

    def step(self) -> bool:
        """Hands back this frame and runs the callbacks of the next. False once the server is gone."""
        if not self.client.isConnected():
            return False
        self.client.update()
        self._dispatch()
        return True

    async def step_async(self) -> bool:
        """`step()` that leaves the event loop free while the server runs the game."""
        if not self.client.isConnected():
            return False
        await self.client.update_async()
        self._dispatch()
        return True

    def _dispatch(self) -> None:
        game = self.client.game
        if game is None:
            return
        for event in game.getEvents():
            dispatch(self.module, event)

    def run(self) -> None:
        """Runs until the server closes the connection (the game exits)."""
        try:
            while self.step():
                pass
        except ServerConnectionError:
            pass

    async def run_async(self) -> None:
        """`run()` on an asyncio event loop."""
        try:
            while await self.step_async():
                pass
        except ServerConnectionError:
            pass

    def close(self) -> None:
        """Disconnects the client."""
        self.client.disconnect()
