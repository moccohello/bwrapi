"""Minimal Python BWAPI bot: workers mine the closest minerals and depots train workers.

Start StarCraft: Remastered and a game, then run (from this folder's parent):

    python -m bwrapi.bwapi.run examples/bot.py --inject

`--inject` puts the BWAPI server shipped with the package into the running
game; leave it out when the server is already in the game.
"""

from bwrapi.bwapi import AIModule, Broodwar, Filter


class Bot(AIModule):
    def onStart(self):
        Broodwar.sendText("hello from Python")

    def onFrame(self):
        me = Broodwar.self()
        if me is None or Broodwar.isReplay():
            return
        Broodwar.drawTextScreen(10, 10, "frame %d  minerals %d", Broodwar.getFrameCount(), me.minerals())
        # Commands reach the game only after the latency, so do not repeat them in between.
        if Broodwar.getFrameCount() % max(Broodwar.getLatencyFrames(), 1):
            return
        for unit in me.getUnits():
            if not unit.exists() or not unit.isCompleted():
                continue
            kind = unit.getType()
            if kind.isWorker() and unit.isIdle():
                mineral = unit.getClosestUnit(Filter.IsMineralField)
                if mineral is not None:
                    unit.gather(mineral)
            elif kind.isResourceDepot() and unit.isIdle():
                unit.train(kind.getRace().getWorker())

    def onUnitDestroy(self, unit):
        if unit.getPlayer() is Broodwar.self():
            Broodwar.printf("lost %s", unit.getType())
