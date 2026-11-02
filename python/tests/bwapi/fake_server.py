"""In-process stand-in for the in-game BWAPI server: fills a GameData and answers the pipe protocol.

The GameData lives in ordinary memory instead of a file mapping, and the pipe is
a pair of queues; the Client under test runs its real protocol over them.
"""

from __future__ import annotations

import queue
import threading

from bwrapi.bwapi import gamedata
from bwrapi.bwapi.client import Client
from bwrapi.bwapi.type_ids import EventTypeId

READY, UPDATE = 2, 1


class QueuePipe:
    """Client end of the fake pipe."""

    def __init__(self) -> None:
        self.to_server: queue.Queue[int] = queue.Queue()
        self.to_client: queue.Queue[int | None] = queue.Queue()
        self.closed = False

    def write_code(self, code: int) -> None:
        if self.closed:
            raise BrokenPipeError("closed")
        self.to_server.put(code)

    def read_code(self) -> int:
        code = self.to_client.get(timeout=5)
        if code is None:
            raise BrokenPipeError("server closed the pipe")
        return code

    def close(self) -> None:
        self.closed = True


class FakeTransport:
    def __init__(self, servers: dict[int, gamedata.GameData], table: gamedata.GameTable | None = None) -> None:
        self.servers = servers
        self.table = table
        self.pipes: dict[int, QueuePipe] = {}
        self.closed = 0

    def process_running(self, pid):
        return True

    def open_table(self):
        return self.table

    def open_server(self, server_pid: int):
        if server_pid not in self.servers:
            raise FileNotFoundError(server_pid)
        pipe = self.pipes[server_pid] = QueuePipe()
        return self.servers[server_pid], pipe

    def close(self) -> None:
        self.closed += 1


def new_data() -> gamedata.GameData:
    data = gamedata.GameData()
    data.client_version = 10003
    data.revision = 5030
    data.hasLatCom = True
    data.hasGUI = True
    data.self = data.enemy = data.neutral = -1
    return data


class FakeServer:
    """Server side of one connection: stage a frame, hand it over, read the client's commands."""

    def __init__(self, data: gamedata.GameData | None = None, pid: int = 4242) -> None:
        self.data = data or new_data()
        self.pid = pid
        self.transport = FakeTransport({pid: self.data})
        self.client = Client(self.transport)
        self.taken: list[dict[str, list]] = []

    @property
    def pipe(self) -> QueuePipe:
        return self.transport.pipes[self.pid]

    def connect(self) -> Client:
        # The handshake ready of the server's accept is already in the pipe.
        connected = threading.Event()

        def server_ready():
            while self.pid not in self.transport.pipes:
                pass
            self.pipe.to_client.put(READY)
            connected.set()

        thread = threading.Thread(target=server_ready)
        thread.start()
        assert self.client.connect(self.pid)
        thread.join()
        return self.client

    def frame(self, *events: tuple[int, int, int]) -> None:
        """One round: take the client's update, then deliver a frame with `events`."""
        data = self.data
        data.eventCount = len(events)
        for i, (kind, v1, v2) in enumerate(events):
            data.events[i].type, data.events[i].v1, data.events[i].v2 = int(kind), v1, v2

        def serve():
            assert self.pipe.to_server.get(timeout=5) == UPDATE
            self.taken.append(take_commands(data))
            self.pipe.to_client.put(READY)

        thread = threading.Thread(target=serve)
        thread.start()
        self.client.update()
        thread.join()

    def start_match(self, *extra: tuple[int, int, int]) -> None:
        self.data.isInGame = True
        self.frame((EventTypeId.MATCH_START, 0, 0), *extra, (EventTypeId.MATCH_FRAME, 0, 0))


def take_commands(data: gamedata.GameData) -> dict[str, list]:
    """What the server reads back after an update, then clears as it does."""
    taken = {
        "unit_commands": [(c.type, c.unitIndex, c.targetIndex, c.x, c.y, c.extra)
                          for c in data.unitCommands[:data.unitCommandCount]],
        "commands": [(c.type, c.value1, c.value2) for c in data.commands[:data.commandCount]],
        "shapes": [(s.type, s.ctype, s.x1, s.y1, s.x2, s.y2, s.extra1, s.extra2, s.color, s.isSolid)
                   for s in data.shapes[:data.shapeCount]],
        "strings": [data.strings[i].value.decode() for i in range(data.stringCount)],
    }
    data.unitCommandCount = data.commandCount = data.shapeCount = data.stringCount = 0
    return taken


def set_player(data, index, *, name="", race=1, type=1, force=1, neutral=False, minerals=0, gas=0):
    player = data.players[index]
    player.name = name.encode()
    player.race, player.type, player.force = race, type, force
    player.isNeutral = neutral
    player.isParticipating = not neutral
    player.minerals, player.gas = minerals, gas
    data.playerCount = max(data.playerCount, index + 1)
    return player


def set_unit(data, index, *, type, player, x, y, completed=True, **fields):
    """A live unit row plus its unit-finder entries, kept sorted as the server keeps them."""
    unit = data.units[index]
    unit.id, unit.type, unit.player = index, type, player
    unit.positionX, unit.positionY = x, y
    unit.exists = True
    unit.isCompleted = completed
    unit.isPowered = True
    unit.isInterruptible = True
    unit.isIdle = True
    unit.isVisible[player if 0 <= player < 9 else 0] = True
    for name in ("target", "orderTarget", "buildUnit", "transport", "carrier", "hatchery", "addon",
                 "nydusExit", "powerUp", "rallyUnit", "lastAttackerPlayer"):
        setattr(unit, name, -1)
    unit.order = 3  # PlayerGuard
    unit.buildType, unit.tech, unit.upgrade = 228, 44, 61
    for name, value in fields.items():
        setattr(unit, name, value)
    # The server keys both search tables by the unit's position and keeps them sorted.
    xs = sorted([(data.xUnitSearch[i].searchValue, data.xUnitSearch[i].unitIndex)
                 for i in range(data.unitSearchSize)] + [(x, index)])
    ys = sorted([(data.yUnitSearch[i].searchValue, data.yUnitSearch[i].unitIndex)
                 for i in range(data.unitSearchSize)] + [(y, index)])
    for i, (value, uid) in enumerate(xs):
        data.xUnitSearch[i].searchValue, data.xUnitSearch[i].unitIndex = value, uid
    for i, (value, uid) in enumerate(ys):
        data.yUnitSearch[i].searchValue, data.yUnitSearch[i].unitIndex = value, uid
    data.unitSearchSize = len(xs)
    return unit
