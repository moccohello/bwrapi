"""BWAPI client protocol and the Game facade over a synthetic GameData."""

from __future__ import annotations

import asyncio
import ctypes
import sys
import threading
from types import SimpleNamespace

import pytest

from bwrapi.bwapi import (
    AIModule, Broodwar, BwapiModuleRunner, Errors, Orders, Position, TilePosition, UnitTypes, gamedata,
)
from bwrapi.bwapi.client import Client, ServerConnectionError, select_server
from bwrapi.bwapi.type_ids import EventTypeId as E
from fake_server import READY, UPDATE, FakeServer, FakeTransport, new_data, set_player, set_unit

SCV, MARINE, BARRACKS, MINERAL, GEYSER, PYLON, HATCHERY, LARVA = 7, 0, 111, 176, 188, 156, 131, 35


def _table(*rows):
    table = gamedata.GameTable()
    for index, (pid, connected, keep_alive) in enumerate(rows):
        row = table.gameInstances[index]
        row.serverProcessID, row.isConnected, row.lastKeepAliveTime = pid, connected, keep_alive
    return table


def test_server_selection_follows_the_official_client():
    assert select_server(None) is None
    assert select_server(_table()) is None
    # Connected rows are taken; among the rest the oldest keep-alive wins.
    assert select_server(_table((10, True, 1), (20, False, 50), (30, False, 40))) == 30


def test_connect_reads_the_table_and_waits_for_ready():
    data = new_data()
    transport = FakeTransport({30: data}, _table((20, True, 1), (30, False, 5)))
    client = Client(transport)

    def server():
        while 30 not in transport.pipes:
            pass
        transport.pipes[30].to_client.put(7)  # not a ready code: the client keeps reading
        transport.pipes[30].to_client.put(READY)

    thread = threading.Thread(target=server)
    thread.start()
    assert client.connect()
    thread.join()
    assert client.server_pid == 30 and client.isConnected()
    assert Broodwar.getClientVersion() == 10003
    client.disconnect()
    assert not client.isConnected() and transport.closed


def test_connect_without_server_or_with_other_version():
    assert Client(FakeTransport({}, _table())).connect() is False
    assert Client(FakeTransport({}, None)).connect(55) is False
    data = new_data()
    data.client_version = 10002
    with pytest.raises(ServerConnectionError):
        Client(FakeTransport({5: data})).connect(5)


def test_connect_reports_waiting_once_across_retries(monkeypatch):
    from bwrapi.bwapi import client as client_module

    monkeypatch.setattr(client_module.time, "sleep", lambda _seconds: None)
    waits = []
    with pytest.raises(ServerConnectionError):
        client_module.connect(transport=FakeTransport({}, None), wait=0.05, on_wait=waits.append)
    assert waits == ["waiting for the BWAPI server in a running game"]
    with pytest.raises(ServerConnectionError):
        client_module.connect(transport=FakeTransport({}, None), on_wait=waits.append)
    assert len(waits) == 1
    with pytest.raises(ServerConnectionError, match="process 77"):
        client_module.connect(77, transport=FakeTransport({}, None))


def test_a_server_with_a_client_is_named_as_busy(monkeypatch):
    from bwrapi.bwapi import client as client_module

    monkeypatch.setattr(client_module.time, "sleep", lambda _seconds: None)
    waits = []
    with pytest.raises(ServerConnectionError, match="process 30 already has a client"):
        client_module.connect(transport=FakeTransport({}, _table((30, True, 1))), wait=0.01, on_wait=waits.append)
    assert "process 30 already has a client" in waits[0]
    with pytest.raises(ServerConnectionError, match="process 30 already has a client"):
        client_module.connect(30, transport=FakeTransport({}, _table((30, True, 1))))
    with pytest.raises(ServerConnectionError, match="no BWAPI server found in process 31"):
        client_module.connect(31, transport=FakeTransport({}, _table((30, True, 1))))


def test_names_decode_as_utf8_then_as_the_system_code_page():
    server, game = _match()
    server.data.players[0].name = "\uc0ac\ub974\uac00\uc2a4".encode("utf-8")
    assert game.self().getName() == "\uc0ac\ub974\uac00\uc2a4"
    name = "\uc0ac\ub974\uac00\uc2a4 \ubd80\uc871"
    server.data.players[1].name = name.encode("cp949")
    server.data.forces[1].name = name.encode("cp949")
    decoded = game.enemy().getName()
    assert isinstance(decoded, str) and decoded and isinstance(game.getForce(1).getName(), str)
    assert repr(game.enemy())
    if sys.platform == "win32" and ctypes.windll.kernel32.GetACP() == 949:
        assert decoded == name


def test_inject_failures_end_in_one_line_without_touching_a_game(monkeypatch, tmp_path):
    from bwrapi.bwapi import run
    from bwrapi.launcher import model, process

    monkeypatch.chdir(tmp_path)
    monkeypatch.setattr(process, "os", SimpleNamespace(name="nt"))
    monkeypatch.setattr(process.preflight, "inspect_product", lambda _product: SimpleNamespace(ok=True))
    monkeypatch.setattr(process, "_inject", lambda *_args: pytest.fail("must not inject"))
    monkeypatch.setattr(process, "_running_starcraft_pids", lambda: [])
    (tmp_path / "bot.py").write_text("from bwrapi.bwapi import AIModule\nclass Bot(AIModule): pass\n", encoding="utf-8")
    with pytest.raises(SystemExit, match="no StarCraft process is running: start the game first"):
        run.main(["bot.py", "--inject"])

    def no_process(pid):
        raise model.LauncherConfigError(f"cannot open process {pid}")

    monkeypatch.setattr(process, "_game_arch", no_process)
    with pytest.raises(SystemExit, match="cannot open process 1"):
        run.main(["bot.py", "--inject", "--process-id", "1"])


def test_default_callbacks_take_what_dispatch_passes():
    from bwrapi.bwapi import dispatch

    for kind in E:
        event = SimpleNamespace(getType=lambda kind=kind: kind, getUnit=lambda: None, getPlayer=lambda: None,
                                getText=lambda: "", getPosition=lambda: None, isWinner=lambda: False)
        dispatch(AIModule(), event)


@pytest.mark.parametrize("name,class_name,message", [
    ("missing.py", None, "bot file not found"),
    ("bot.py", "Nope", "class Nope not found"),
    ("two.py", None, "pick one with --class"),
    ("empty.py", None, "no AIModule subclass found"),
])
def test_bot_file_errors_name_the_problem(tmp_path, name, class_name, message):
    from bwrapi.bwapi.run import load_module

    base = "from bwrapi.bwapi import AIModule\n"
    (tmp_path / "bot.py").write_text(base + "class Bot(AIModule): pass\n", encoding="utf-8")
    (tmp_path / "two.py").write_text(base + "class A(AIModule): pass\nclass B(AIModule): pass\n", encoding="utf-8")
    (tmp_path / "empty.py").write_text(base, encoding="utf-8")
    with pytest.raises(SystemExit, match=message):
        load_module(tmp_path / name, class_name)


def test_update_hands_over_commands_and_disconnects_when_the_pipe_breaks():
    server = FakeServer()
    client = server.connect()
    set_player(server.data, 0, name="me")
    server.data.self = 0
    server.start_match()
    game = client.game
    game.printf("hi %d", 3)
    game.sendTextEx(True, "gg")
    game.pingMinimap(Position(64, 96))
    server.frame((E.MATCH_FRAME, 0, 0))
    taken = server.taken[-1]
    assert taken["strings"] == ["hi 3", "gg"]
    assert taken["commands"] == [(gamedata.CommandType.Printf, 0, 0), (gamedata.CommandType.SendText, 1, 1),
                                 (gamedata.CommandType.PingMinimap, 64, 96)]
    server.pipe.to_client.put(None)
    with pytest.raises(ServerConnectionError):
        client.update()
    assert not client.isConnected() and client.game is None


def test_update_async_waits_off_the_event_loop():
    server = FakeServer()
    client = server.connect()

    async def round_trip():
        task = asyncio.ensure_future(client.update_async())
        await asyncio.sleep(0)  # the loop still runs while the client waits
        assert server.pipe.to_server.get(timeout=5) == UPDATE
        server.data.eventCount = 1
        server.data.events[0].type = E.MENU_FRAME
        server.pipe.to_client.put(READY)
        await task

    asyncio.run(round_trip())
    assert [event.getType() for event in client.game.getEvents()] == [E.MENU_FRAME]


def _match():
    server = FakeServer()
    data = server.data
    server.connect()
    set_player(data, 0, name="me", race=1, minerals=500, gas=100)
    set_player(data, 1, name="foe", race=0)
    set_player(data, 11, name="Neutral", race=7, type=7, force=0, neutral=True)
    data.self, data.enemy, data.neutral = 0, 1, 11
    data.players[0].isEnemy[1] = data.players[1].isEnemy[0] = True
    data.players[0].supplyTotal[1], data.players[0].supplyUsed[1] = 20, 8
    data.forceCount = 3
    data.forces[1].name = b"Team 1"
    data.mapWidth, data.mapHeight = 64, 64
    for x in range(64):
        for y in range(64):
            data.isBuildable[x][y] = True
    data.regionCount = 1  # every tile is in region 0
    data.regions[0].islandID = 1
    set_unit(data, 0, type=SCV, player=0, x=100, y=100)
    set_unit(data, 1, type=MARINE, player=1, x=140, y=100)
    set_unit(data, 2, type=MINERAL, player=11, x=400, y=400, resources=1500)
    set_unit(data, 3, type=GEYSER, player=11, x=600, y=400, resources=5000)
    set_unit(data, 4, type=BARRACKS, player=0, x=304, y=176)
    data.initialUnitCount = 5
    data.players[0].completedUnitCount[BARRACKS] = 1
    data.players[0].isUnitAvailable[MARINE] = True
    server.start_match(*((E.UNIT_DISCOVER, i, 0) for i in range(5)))
    return server, server.client.game


def test_match_start_builds_the_official_sets():
    server, game = _match()
    me, foe, neutral = game.self(), game.enemy(), game.neutral()
    assert (me.getName(), foe.getName(), neutral.getID()) == ("me", "foe", 11)
    assert game.enemies() == {foe} and not game.allies()
    assert {u.getID() for u in game.getAllUnits()} == {0, 1, 2, 3, 4}
    assert [u.getID() for u in game.getMinerals()] == [2] and [u.getID() for u in game.getGeysers()] == [3]
    assert game.getStaticMinerals() == game.getMinerals()
    assert {u.getID() for u in me.getUnits()} == {0, 4}
    assert game.getUnit(2).getInitialResources() == 1500
    assert {f.getName() for f in game.getForces()} == {"Team 1", ""}
    assert me.supplyTotal() == 20 and me.supplyUsed() == 8
    assert game.isInGame() and Broodwar.self() is me


def test_unit_values_and_spatial_queries():
    server, game = _match()
    scv, marine = game.getUnit(0), game.getUnit(1)
    assert scv.getType() == UnitTypes.Terran_SCV and scv.getPosition() == Position(100, 100)
    assert scv.getTilePosition() == TilePosition(2, 2)
    assert scv.getDistance(marine) == marine.getDistance(scv) > 0
    assert scv.getDistance(Position(100, 100)) == 0
    assert game.getUnitsInRadius(Position(100, 100), 60) == {scv, marine}
    assert scv.getUnitsInRadius(60) == {marine}
    assert game.getClosestUnit(Position(130, 100)) is marine
    assert game.getUnitsInRectangle(Position(300, 300), Position(500, 500)) == {game.getUnit(2)}
    assert game.getUnitsOnTile(12, 12) == {game.getUnit(2)}


def test_unit_command_goes_out_with_latency_compensation():
    server, game = _match()
    data = server.data
    data.remainingLatencyFrames = 2
    scv = game.getUnit(0)
    assert scv.move(Position(200, 300))
    assert scv.getOrder() == Orders.Move and scv.getTargetPosition() == Position(200, 300)
    assert scv.getLastCommand().getType() == 10
    assert not game.getUnit(1).move(Position(1, 1))  # the enemy's marine
    assert game.getLastError() == Errors.Unit_Not_Owned
    barracks = game.getUnit(4)
    assert barracks.train(UnitTypes.Terran_Marine)
    assert game.self().minerals() == 450 and barracks.isTraining()
    assert barracks.getTrainingQueue() == [UnitTypes.Terran_Marine]
    server.frame((E.MATCH_FRAME, 0, 0))
    assert server.taken[-1]["unit_commands"] == [(10, 0, -1, 200, 300, 0), (4, 4, -1, 32000, 32032, 0)]


def test_train_on_a_hatchery_uses_a_larva():
    server = FakeServer()
    data = server.data
    server.connect()
    set_player(data, 0, race=0, minerals=200)
    data.self = 0
    data.players[0].supplyTotal[0] = 18
    data.players[0].isUnitAvailable[37] = True
    data.players[0].completedUnitCount[142] = 1  # Spawning Pool
    data.players[0].completedUnitCount[LARVA] = 1
    set_unit(data, 0, type=HATCHERY, player=0, x=200, y=200)
    set_unit(data, 1, type=LARVA, player=0, x=180, y=230, hatchery=0)
    data.initialUnitCount = 2
    data.mapWidth = data.mapHeight = 64
    server.start_match((E.UNIT_DISCOVER, 0, 0), (E.UNIT_DISCOVER, 1, 0))
    hatchery = server.client.game.getUnit(0)
    assert hatchery.getLarva() == {server.client.game.getUnit(1)}
    assert hatchery.train(UnitTypes.Zerg_Zergling)
    server.frame((E.MATCH_FRAME, 0, 0))
    assert server.taken[-1]["unit_commands"][0][:2] == (4, 1)


def test_events_update_sets_and_reach_the_module_in_order():
    server, game = _match()
    data = server.data
    calls = []

    class Bot(AIModule):
        def onFrame(self): calls.append("frame")
        def onUnitCreate(self, unit): calls.append(("create", unit.getID()))
        def onUnitDestroy(self, unit): calls.append(("destroy", unit.getID()))
        def onReceiveText(self, player, text): calls.append(("text", player.getName(), text))
        def onEnd(self, isWinner): calls.append(("end", isWinner))

    runner = BwapiModuleRunner(server.client, Bot())
    set_unit(data, 5, type=PYLON, player=0, x=500, y=100)
    data.eventStrings[0].value = b"hello"

    def serve(*events):
        data.eventCount = len(events)
        for i, (kind, v1, v2) in enumerate(events):
            data.events[i].type, data.events[i].v1, data.events[i].v2 = int(kind), v1, v2

        def answer():
            assert server.pipe.to_server.get(timeout=5) == UPDATE
            server.pipe.to_client.put(READY)

        thread = threading.Thread(target=answer)
        thread.start()
        assert runner.step()
        thread.join()

    serve((E.UNIT_DISCOVER, 5, 0), (E.UNIT_CREATE, 5, 0), (E.RECEIVE_TEXT, 1, 0), (E.MATCH_FRAME, 0, 0))
    assert calls == [("create", 5), ("text", "foe", "hello"), "frame"]
    assert game.getUnit(5) in game.self().getUnits() and game.getUnit(5) in game._pylons
    data.units[1].exists = False
    serve((E.UNIT_DESTROY, 1, 0), (E.UNIT_EVADE, 1, 0), (E.MATCH_FRAME, 0, 0))
    assert calls[-2:] == [("destroy", 1), "frame"]
    serve((E.MATCH_FRAME, 0, 0), (E.MATCH_END, 1, 0))
    assert calls[-2:] == ["frame", ("end", True)]
    assert game.getUnit(1) not in game.getAllUnits()
    data.isInGame = False
    serve((E.MENU_FRAME, 0, 0))
    assert not game.getAllUnits() and game.self() is None


def test_drawing_writes_shapes_and_text_strings():
    server, game = _match()
    game.drawBoxMap(Position(1, 2), Position(3, 4), 111, True)
    game.drawCircleScreen(10, 20, 5, 117)
    game.drawLineMouse(0, 0, 7, 8, 255)
    game.setTextSize(3)
    game.drawTextMap(Position(5, 6), "x=%d", 9)
    server.frame((E.MATCH_FRAME, 0, 0))
    taken = server.taken[-1]
    assert taken["shapes"] == [
        (gamedata.ShapeType.Box, 2, 1, 2, 3, 4, 0, 0, 111, True),
        (gamedata.ShapeType.Circle, 1, 10, 20, 0, 0, 5, 0, 117, False),
        (gamedata.ShapeType.Line, 3, 0, 0, 7, 8, 0, 0, 255, False),
        (gamedata.ShapeType.Text, 2, 5, 6, 0, 0, 0, 3, 0, False),
    ]
    assert taken["strings"] == ["x=9"]
    server.data.hasGUI = False
    game.drawDotMap(1, 1, 111)
    server.frame((E.MATCH_FRAME, 0, 0))
    assert server.taken[-1]["shapes"] == []


def test_full_arrays_drop_appends_like_the_release_client():
    server, game = _match()
    data = server.data
    data.stringCount = gamedata.MAX_STRINGS
    commands = data.commandCount
    game.printf("dropped")  # GameImpl.cpp checks these capacities with assert only
    assert data.stringCount == gamedata.MAX_STRINGS
    assert data.commandCount == commands  # no command points at a string it could not store
    data.commandCount = gamedata.MAX_COMMANDS
    game.pingMinimap(Position(64, 96))
    assert data.commandCount == gamedata.MAX_COMMANDS
    data.unitCommandCount = gamedata.MAX_UNIT_COMMANDS
    assert game.getUnit(0).move(Position(200, 300))
    assert data.unitCommandCount == gamedata.MAX_UNIT_COMMANDS
    data.shapeCount = gamedata.MAX_SHAPES
    game.drawDotScreen(Position(3, 4), 111)
    assert data.shapeCount == gamedata.MAX_SHAPES


def test_long_strings_cut_at_a_utf8_character_boundary():
    server, game = _match()
    index = game._add_string("Ã" + "한" * 600)
    stored = server.data.strings[index].value
    assert len(stored) == 1022  # 1023 bytes would split the trailing character
    assert stored.decode("utf-8") == "Ã" + "한" * 340
    assert game._add_string("a" * 1024) == index + 1  # a pure-ASCII cut keeps all 1023 bytes
    assert server.data.strings[index + 1].value == b"a" * 1023


def test_map_queries_and_regions():
    server = FakeServer()
    data = server.data
    server.connect()
    data.mapWidth, data.mapHeight = 4, 4
    data.isWalkable[3][5] = True
    data.isBuildable[1][2] = True
    data.isOccupied[1][2] = True
    data.regionCount = 3
    for index, (group, cx) in enumerate(((1, 16), (1, 48), (2, 80))):
        region = data.regions[index]
        region.id, region.islandID, region.center_x, region.center_y = index, group, cx, 16
    data.regions[0].neighborCount = 2
    data.regions[0].neighbors[0], data.regions[0].neighbors[1] = 1, 2
    data.mapTileRegionId[1][0] = 1
    data.mapTileRegionId[2][0] = 0x2000  # a split tile
    data.mapSplitTilesMiniTileMask[0] = 1 << 1  # minitile (1, 0) belongs to region 2
    data.mapSplitTilesRegion1[0], data.mapSplitTilesRegion2[0] = 1, 2
    server.start_match()
    game = server.client.game
    assert game.isWalkable(3, 5) and not game.isWalkable(16, 0)
    assert game.isBuildable(1, 2) and not game.isBuildable(1, 2, True)
    assert game.getRegionAt(40, 4).getID() == 1
    assert game.getRegionAt(64 + 1, 1).getID() == 1 and game.getRegionAt(64 + 9, 1).getID() == 2
    assert game.hasPath(Position(4, 4), Position(40, 4)) and not game.hasPath(Position(4, 4), Position(73, 1))
    first = game.getRegion(0)
    assert first.getClosestAccessibleRegion() is game.getRegion(1)
    assert first.getClosestInaccessibleRegion() is game.getRegion(2)


def test_every_command_check_runs_on_game_data():
    from bwrapi.bwapi import UnitCommand

    server, game = _match()
    scv, marine, mineral, barracks = (game.getUnit(i) for i in (0, 1, 2, 4))
    commands = [
        UnitCommand.attack(None, marine), UnitCommand.attack(None, Position(10, 10)),
        UnitCommand.build(None, TilePosition(20, 20), UnitTypes.Terran_Supply_Depot),
        UnitCommand.buildAddon(None, UnitTypes.Terran_Comsat_Station), UnitCommand.train(None, UnitTypes.Terran_Marine),
        UnitCommand.morph(None, UnitTypes.Zerg_Lair), UnitCommand.research(None, 0), UnitCommand.upgrade(None, 0),
        UnitCommand.setRallyPoint(None, Position(5, 5)), UnitCommand.setRallyPoint(None, scv),
        UnitCommand.move(None, Position(50, 50)), UnitCommand.patrol(None, Position(50, 50)),
        UnitCommand.holdPosition(None), UnitCommand.stop(None), UnitCommand.follow(None, marine),
        UnitCommand.gather(None, mineral), UnitCommand.returnCargo(None), UnitCommand.repair(None, barracks),
        UnitCommand.burrow(None), UnitCommand.unburrow(None), UnitCommand.cloak(None), UnitCommand.decloak(None),
        UnitCommand.siege(None), UnitCommand.unsiege(None), UnitCommand.lift(None),
        UnitCommand.land(None, TilePosition(9, 9)), UnitCommand.load(None, marine), UnitCommand.unload(None, marine),
        UnitCommand.unloadAll(None), UnitCommand.unloadAll(None, Position(70, 70)),
        UnitCommand.rightClick(None, Position(1, 1)), UnitCommand.rightClick(None, mineral),
        UnitCommand.haltConstruction(None), UnitCommand.cancelConstruction(None), UnitCommand.cancelAddon(None),
        UnitCommand.cancelTrain(None), UnitCommand.cancelTrain(None, 0), UnitCommand.cancelMorph(None),
        UnitCommand.cancelResearch(None), UnitCommand.cancelUpgrade(None), UnitCommand(None, 40, extra=0),
        UnitCommand.useTech(None, 1, Position(3, 3)), UnitCommand.useTech(None, 2, marine),
        UnitCommand.placeCOP(None, TilePosition(4, 4)),
    ]
    assert sorted({int(c.type) for c in commands}) == list(range(44))
    for unit in (scv, marine, mineral, barracks):
        for command in commands:
            assert type(unit.canIssueCommand(command)) is bool
            assert type(unit.canIssueCommandGrouped(command)) is bool
    assert scv.canIssueCommand(UnitCommand.gather(None, mineral))
    server.data.remainingLatencyFrames = 2  # latency compensation applies to later frames only
    assert scv.gather(mineral) and scv.isGatheringMinerals()
    assert scv.getOrder() == Orders.MoveToMinerals


def test_example_bot_runs_through_the_run_entry_point():
    from pathlib import Path

    from bwrapi.bwapi.run import load_module
    from fake_server import take_commands

    bot = load_module(Path(__file__).resolve().parents[2] / "examples" / "bot.py")
    server, game = _match()
    data = server.data
    data.latencyFrames, data.frameCount = 2, 4
    runner = BwapiModuleRunner(server.client, bot)
    bot.onStart()
    data.eventCount = 1
    data.events[0].type = E.MATCH_FRAME

    def answer():
        assert server.pipe.to_server.get(timeout=5) == UPDATE
        server.taken.append(take_commands(data))
        server.pipe.to_client.put(READY)

    thread = threading.Thread(target=answer)
    thread.start()
    assert runner.step()  # hands over onStart's text, then runs onFrame
    thread.join()
    server.frame((E.MATCH_FRAME, 0, 0))
    started, framed = server.taken[-2:]
    assert started["commands"] == [(gamedata.CommandType.SendText, 0, 0)]
    assert (15, 0, 2, 32000, 32032, 0) in framed["unit_commands"]  # the SCV gathers the mineral
    assert framed["strings"] == ["frame 4  minerals 500"]
