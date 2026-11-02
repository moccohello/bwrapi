"""Calls every public method of the BWAPI classes, so a wrong field name cannot hide in an untaken path.

Type classes need no server and run on every value. Unit, Player and the other handles
run against the fake server, in a match rich enough to pass the resource and tech checks.
"""

from __future__ import annotations

import inspect
import itertools
import random
import re

import pytest

from bwrapi.bwapi import (
    AIModule, Bullet, Event, Filter, Force, Game, Highest, K_A, Lowest, M_LEFT, Orders, Player, Position, Races, Region,
    TechTypes, TilePosition, Unit, UnitCommand, UnitCommandTypes, Unitset, UnitTypes, UpgradeTypes, WalkPosition, WeaponTypes, _latcom, dispatch,
    gamedata, types,
)
from bwrapi.bwapi.type_ids import EventTypeId as E
from fake_server import FakeServer, set_player, set_unit
import upstream_root

TYPE_CLASSES = [value for value in vars(types).values()
                if isinstance(value, type) and issubclass(value, types.Type) and value is not types.Type]
NAMESPACES = [getattr(types, cls.__name__ + "s") for cls in TYPE_CLASSES]
LEVELS = (1, 2, 3)


def _public_members(cls: type, *inherited_noise: type) -> list[str]:
    noise = set().union(*(dir(kind) for kind in inherited_noise)) if inherited_noise else set()
    return [name for name in dir(cls) if not name.startswith("_") and name not in noise]


def _type_calls(value: types.Type, name: str):
    """Every way to call one method of a type value: its default, each level, its required argument."""
    member = getattr(value, name)
    parameters = inspect.signature(member).parameters
    yield ()
    if "level" in parameters:
        yield from ((level,) for level in LEVELS)


REQUIRED_TYPE_ARGUMENTS = {"getType": lambda value: (value.getName(),), "isSuccessorOf": lambda value: (value,)}


@pytest.mark.parametrize("cls", TYPE_CLASSES, ids=lambda cls: cls.__name__)
def test_every_type_method_on_every_constant_and_id(cls):
    namespace = getattr(types, cls.__name__ + "s")
    constants = [value for value in vars(namespace).values() if isinstance(value, cls)]
    assert constants
    unknown = types.TYPE_DATA[cls.__name__]["enum"]["Unknown"]
    members = [name for name in _public_members(cls, int) if not isinstance(getattr(cls, name), type)]
    assert members
    for value in [*constants, *map(cls, range(unknown + 1))]:
        assert str(value) is not None and repr(value)
        for name in members:
            if isinstance(inspect.getattr_static(cls, name), property):
                getattr(value, name)
                continue
            if name in REQUIRED_TYPE_ARGUMENTS:
                getattr(value, name)(*REQUIRED_TYPE_ARGUMENTS[name](value))
                continue
            for arguments in _type_calls(value, name):
                getattr(value, name)(*arguments)


@pytest.mark.parametrize("namespace", NAMESPACES, ids=lambda namespace: type(next(iter(
    value for value in vars(namespace).values() if isinstance(value, types.Type)))).__name__)
def test_every_namespace_function(namespace):
    for value in vars(namespace).values():
        if callable(value) and not isinstance(value, types.Type):
            value()


def _upstream_array(source: str, name: str) -> list[int]:
    body = re.search(rf"\b{name}\[[^\]]*\]\s*=[^{{]*\{{(.*?)\}};", source, re.S).group(1)
    return [int(item) for item in re.findall(r"-?\d+", body)]


def test_upgrade_prices_and_times_at_each_level():
    carapace, armor, shells = UpgradeTypes.Zerg_Carapace, UpgradeTypes.Terran_Infantry_Armor, UpgradeTypes.U_238_Shells
    assert [carapace.gasPrice(level) for level in LEVELS] == [150, 225, 300]
    assert [carapace.mineralPrice(level) for level in LEVELS] == [150, 225, 300]
    assert [carapace.upgradeTime(level) for level in LEVELS] == [4000, 4480, 4960]
    assert [armor.gasPrice(level) for level in LEVELS] == [100, 175, 250]
    assert (shells.gasPrice(), shells.gasPriceFactor(), shells.upgradeTime()) == (150, 0, 1500)


def test_upgrade_values_match_the_upstream_tables():
    if not (upstream_root.SOURCE / "BWAPILIB" / "Source" / "UpgradeType.cpp").is_file():
        pytest.skip("needs the BWAPI 4.4 upstream source")
    source = (upstream_root.SOURCE / "BWAPILIB" / "Source" / "UpgradeType.cpp").read_text(encoding="utf-8", errors="replace")
    ore, ore_factor = _upstream_array(source, "defaultOreCostBase"), _upstream_array(source, "defaultOreCostFactor")
    time, time_factor = _upstream_array(source, "defaultTimeCostBase"), _upstream_array(source, "defaultTimeCostFactor")
    repeats = _upstream_array(source, "defaultMaxRepeats")
    assert len(ore) == len(time) == len(repeats) == 63
    for upgrade_id in range(63):
        upgrade = types.UpgradeType(upgrade_id)
        assert upgrade.maxRepeats() == repeats[upgrade_id]
        for level in LEVELS:
            # UpgradeType.cpp: the gas price is the mineral price.
            price = ore[upgrade_id] + (level - 1) * ore_factor[upgrade_id]
            assert (upgrade.mineralPrice(level), upgrade.gasPrice(level)) == (price, price)
            assert upgrade.upgradeTime(level) == time[upgrade_id] + (level - 1) * time_factor[upgrade_id]


# ---- Handles, against the fake server ----

ME, FOE, NEUTRAL = 0, 1, 11
SOME_UNIT_TYPES = (UnitTypes.Terran_Marine, UnitTypes.Terran_Barracks, UnitTypes.Terran_Machine_Shop,
                   UnitTypes.Zerg_Hatchery, UnitTypes.Zerg_Lurker, UnitTypes.Protoss_Pylon, UnitTypes.None_)
SOME_TECHS = (TechTypes.Stim_Packs, TechTypes.Burrowing, TechTypes.Spider_Mines, TechTypes.Tank_Siege_Mode,
              TechTypes.Lockdown, TechTypes.Archon_Warp, TechTypes.Healing, TechTypes.Cloaking_Field,
              TechTypes.Nuclear_Strike, TechTypes.None_)
SOME_UPGRADES = (UpgradeTypes.Zerg_Carapace, UpgradeTypes.Terran_Infantry_Armor, UpgradeTypes.U_238_Shells,
                 UpgradeTypes.None_)


def _rich_match():
    """A match whose player can afford everything, with units in many states."""
    server = FakeServer()
    data = server.data
    server.connect()
    set_player(data, ME, name="me", race=1, minerals=100000, gas=100000)
    set_player(data, FOE, name="foe", race=0)
    set_player(data, NEUTRAL, name="Neutral", race=7, type=7, force=0, neutral=True)
    data.self, data.enemy, data.neutral = ME, FOE, NEUTRAL
    data.players[ME].isEnemy[FOE] = data.players[FOE].isEnemy[ME] = True
    me = data.players[ME]
    me.supplyTotal[0] = me.supplyTotal[1] = me.supplyTotal[2] = 400
    for field in ("allUnitCount", "visibleUnitCount", "completedUnitCount", "isUnitAvailable"):
        array = getattr(me, field)
        for index in range(len(array)):
            array[index] = 3
    for index in range(len(me.isResearchAvailable)):
        me.isResearchAvailable[index] = me.hasResearched[index] = True
    me.hasResearched[int(TechTypes.Restoration)] = False
    for index in range(len(me.maxUpgradeLevel)):
        me.maxUpgradeLevel[index] = 3
    data.forceCount = 3
    data.latencyFrames, data.remainingLatencyFrames = 2, 2  # latency compensation runs on every command
    data.mapWidth, data.mapHeight = 64, 64
    for x in range(64):
        for y in range(64):
            data.isBuildable[x][y] = True
            data.isVisible[x][y] = data.isExplored[x][y] = True
            data.hasCreep[x][y] = x >= 32
    for x in range(64 * 4):
        for y in range(64 * 4):
            data.isWalkable[x][y] = True
    data.regionCount = 1  # every tile is in region 0
    data.regions[0].islandID = 1
    data.regions[0].isAccessible = True
    data.bullets[0].exists = True
    data.bullets[0].type, data.bullets[0].player, data.bullets[0].source, data.bullets[0].target = 0, ME, 0, 1
    full_energy = {"hitPoints": 40, "shields": 20, "energy": 250}
    rows = [
        dict(type=UnitTypes.Terran_SCV, player=ME, x=100, y=100, **full_energy),
        dict(type=UnitTypes.Terran_Marine, player=FOE, x=140, y=100, **full_energy),
        dict(type=UnitTypes.Resource_Mineral_Field, player=NEUTRAL, x=400, y=400, resources=1500),
        dict(type=UnitTypes.Resource_Vespene_Geyser, player=NEUTRAL, x=600, y=400, resources=5000),
        dict(type=UnitTypes.Terran_Barracks, player=ME, x=304, y=176, **full_energy),
        dict(type=UnitTypes.Terran_Command_Center, player=ME, x=800, y=800, isTraining=True, isIdle=False,
             trainingQueueCount=2, **full_energy),
        dict(type=UnitTypes.Terran_Engineering_Bay, player=ME, x=900, y=800, order=int(Orders.Upgrade), isIdle=False,
             upgrade=int(UpgradeTypes.Terran_Infantry_Armor), **full_energy),
        dict(type=UnitTypes.Terran_Academy, player=ME, x=1000, y=800, order=int(Orders.ResearchTech), isIdle=False,
             tech=int(TechTypes.Stim_Packs), **full_energy),
        dict(type=UnitTypes.Terran_Ghost, player=ME, x=200, y=200, **full_energy),
        dict(type=UnitTypes.Terran_Siege_Tank_Tank_Mode, player=ME, x=240, y=200, **full_energy),
        dict(type=UnitTypes.Terran_Siege_Tank_Siege_Mode, player=ME, x=280, y=200, **full_energy),
        dict(type=UnitTypes.Terran_Dropship, player=ME, x=300, y=300, isFlying=True, **full_energy),
        dict(type=UnitTypes.Terran_Science_Vessel, player=ME, x=330, y=300, isFlying=True, **full_energy),
        dict(type=UnitTypes.Terran_Barracks, player=ME, x=500, y=100, isLifted=True, isFlying=True, **full_energy),
        dict(type=UnitTypes.Terran_Factory, player=ME, x=560, y=100, addon=4, completed=False, isIdle=False,
             isConstructing=True, **full_energy),
        dict(type=UnitTypes.Zerg_Hatchery, player=ME, x=1200, y=1200, **full_energy),
        dict(type=UnitTypes.Zerg_Larva, player=ME, x=1200, y=1240, hatchery=15, **full_energy),
        dict(type=UnitTypes.Zerg_Drone, player=ME, x=1300, y=1200, isBurrowed=True, isCarryingMinerals=True,
             **full_energy),
        dict(type=UnitTypes.Zerg_Egg, player=ME, x=1320, y=1200, isMorphing=True, isIdle=False, **full_energy),
        dict(type=UnitTypes.Protoss_Probe, player=ME, x=1500, y=1500, **full_energy),
        dict(type=UnitTypes.Protoss_Carrier, player=ME, x=1540, y=1500, isFlying=True, interceptorCount=4,
             **full_energy),
        dict(type=UnitTypes.Protoss_Interceptor, player=ME, x=1560, y=1500, isFlying=True, carrier=20, **full_energy),
        dict(type=UnitTypes.Protoss_High_Templar, player=ME, x=1600, y=1500, **full_energy),
        dict(type=UnitTypes.Protoss_Stargate, player=ME, x=1650, y=1500, **full_energy),
        dict(type=UnitTypes.Special_Power_Generator, player=ME, x=1700, y=1500, **full_energy),
        dict(type=UnitTypes.Resource_Mineral_Field, player=NEUTRAL, x=450, y=400, resources=0, exists=False),
        dict(type=UnitTypes.Terran_Academy, player=ME, x=1100, y=800, **full_energy),
        dict(type=UnitTypes.Terran_Engineering_Bay, player=ME, x=1100, y=900, **full_energy),
    ]
    for index, row in enumerate(rows):
        unit = set_unit(data, index, **{key: int(value) if isinstance(value, types.Type) else value
                                        for key, value in row.items()})
        for player in range(9):
            unit.isVisible[player] = True
    data.initialUnitCount = len(rows)
    server.start_match(*((E.UNIT_DISCOVER, index, 0) for index in range(len(rows))))
    return server, server.client.game


POSITION_TARGET = re.compile(r"Position|[Ll]and|COP|^build$|^unloadAll$|^move$|^patrol$")
UNIT_TARGET = re.compile(r"Unit|Gather|Follow|Repair|Load|Unload|^gather$|^follow$|^repair$|^load$|^unload$|^isInWeaponRange$")


def _target_values(game):
    unit = game.getUnit(1)
    return (unit, game.getUnit(0), game.getUnit(2), Position(200, 200), TilePosition(10, 10), WalkPosition(40, 40))


def _candidates(game, method_name, name):
    """Values for a parameter, by its name. C++ overloads fix a target's kind by the method."""
    if name == "target":
        values = _target_values(game)
        if POSITION_TARGET.search(method_name):
            return values[3:]
        return values[:3] if UNIT_TARGET.search(method_name) else values
    if name == "targetUnit":
        return (game.getUnit(1), game.getUnit(0))
    if name in ("type", "unit_type", "unit", "fromType", "toType"):
        return SOME_UNIT_TYPES
    if name == "tech":
        return SOME_TECHS
    if name == "upgrade":
        return SOME_UPGRADES
    if name == "weapon":
        return (WeaponTypes.Gauss_Rifle, WeaponTypes.None_)
    if name in ("player", "other"):
        return (game.self(), game.enemy())
    if name == "race":
        return (Races.Terran, Races.None_)
    if name == "command":
        return tuple(UnitCommand(game.getUnit(0), kind, game.getUnit(1), 200, 200, 0)
                     for kind in (UnitCommandTypes.Move, UnitCommandTypes.Attack_Unit, UnitCommandTypes.Build,
                                  UnitCommandTypes.Use_Tech, UnitCommandTypes.Upgrade, UnitCommandTypes.Research))
    if name == "command_type":
        return tuple(value for value in vars(UnitCommandTypes).values() if isinstance(value, types.UnitCommandType))
    if name == "slot":
        return (0, -2)
    if name in ("radius", "amount", "key"):
        return (1,)
    if name == "value":
        return ("info",)
    if name in ("pred", "predicate"):
        return (None, Filter.IsWorker)
    if name == "action":
        return (lambda _interface: None,)
    return None


def _call_forms(game, method_name, method):
    """Positional forms over the candidates of each named parameter, plus all check flags off."""
    signature = inspect.signature(method)
    names, flags = [], []
    choices = []
    for parameter in signature.parameters.values():
        if parameter.kind in (parameter.VAR_POSITIONAL, parameter.VAR_KEYWORD):
            continue
        if parameter.name.startswith("check"):
            flags.append(parameter.name)
            continue
        values = _candidates(game, method_name, parameter.name)
        required = parameter.default is parameter.empty
        if values is None:
            if required:
                raise AssertionError(f"no candidates for {method.__qualname__}({parameter.name})")
            continue
        names.append(parameter.name)
        choices.append(tuple(values) if required else (*values, _OMIT))
    for values in itertools.product(*choices):
        keywords = {name: value for name, value in zip(names, values) if value is not _OMIT}
        yield keywords
        if flags:
            yield {**keywords, **{flag: False for flag in flags}}


_OMIT = object()


def _call_all(game, target, cls, *skip):
    """Calls every public method of `target`; returns the failures instead of stopping at the first."""
    failures = []
    for name in _public_members(cls):
        if name in skip or not callable(getattr(cls, name)):
            continue
        method = getattr(target, name)
        for keywords in _call_forms(game, name, method):
            try:
                method(**keywords)
            except Exception as error:  # noqa: BLE001 - reported with the call that raised it
                failures.append(f"{cls.__name__}.{name}({keywords}) on {target!r}: {error!r}")
            game._data.unitCommandCount = game._data.commandCount = 0
    return failures


def test_every_unit_method_on_every_unit():
    _server, game = _rich_match()
    failures = []
    for unit in sorted(game.getAllUnits(), key=lambda value: value.getID()):
        failures += _call_all(game, unit, Unit)
    assert not failures, "\n".join(failures[:40])


def test_every_player_method_on_every_player():
    _server, game = _rich_match()
    failures = []
    for player in (game.self(), game.enemy(), game.neutral()):
        failures += _call_all(game, player, Player)
        for upgrade in vars(UpgradeTypes).values():
            if isinstance(upgrade, types.UpgradeType):
                failures += [] if player.getMaxUpgradeLevel(upgrade) >= 0 else [f"{upgrade!r}"]
    assert not failures, "\n".join(failures[:40])


def test_every_unit_check_against_every_type_constant():
    """The checks whose costs and requirements read the type tables, on every constant."""
    _server, game = _rich_match()
    constants = {
        "canTrain": types.UnitType, "canBuild": types.UnitType, "canMorph": types.UnitType,
        "canBuildAddon": types.UnitType, "canResearch": types.TechType, "canUpgrade": types.UpgradeType,
        "canUseTechWithOrWithoutTarget": types.TechType,
    }
    namespaces = {types.UnitType: UnitTypes, types.TechType: TechTypes, types.UpgradeType: UpgradeTypes}
    failures = []
    for unit in game.getAllUnits():
        for name, kind in constants.items():
            for value in vars(namespaces[kind]).values():
                if not isinstance(value, kind):
                    continue
                for flag in (True, False):
                    try:
                        getattr(unit, name)(value, checkCommandibility=flag)
                    except Exception as error:  # noqa: BLE001 - reported with the call that raised it
                        failures.append(f"{name}({value!r}, checkCommandibility={flag}) on {unit!r}: {error!r}")
    assert not failures, "\n".join(failures[:40])


def test_every_force_region_bullet_method():
    _server, game = _rich_match()
    failures = []
    for force in game.getForces():
        failures += _call_all(game, force, Force)
    failures += _call_all(game, game.getRegion(0), Region, "getDistance")
    assert game.getRegion(0).getDistance(game.getRegion(0)) == 0
    for bullet in game.getBullets():
        failures += _call_all(game, bullet, Bullet)
    assert not failures, "\n".join(failures[:40])


_UNIT_REFERENCES = {"target", "orderTarget", "buildUnit", "transport", "carrier", "hatchery", "addon", "nydusExit",
                    "powerUp", "rallyUnit"}
_UNIT_RANGES = {"type": 234, "order": 190, "secondaryOrder": 190, "tech": 47, "upgrade": 64, "buildType": 234,
                "carryResourceType": 3, "trainingQueueCount": 6}


def scramble_units(data, rng: random.Random) -> None:
    """Puts every unit in a random state, to reach the branches one fixed match does not."""
    count = data.initialUnitCount
    for index in range(count):
        unit = data.units[index]
        for name, kind in gamedata.UnitData._fields_:
            if name in ("id", "player"):
                continue
            if name == "transport":  # a loaded unit takes no command
                unit.transport = rng.randrange(count) if rng.random() < 0.1 else -1
            elif name == "type":  # half keep the type, so the commands of that type pass their checks
                if rng.random() < 0.5:
                    unit.type = rng.randrange(_UNIT_RANGES[name])
            elif name in _UNIT_REFERENCES:
                setattr(unit, name, rng.choice((-1, rng.randrange(count))))
            elif name in _UNIT_RANGES:
                setattr(unit, name, rng.randrange(_UNIT_RANGES[name]))
            elif name == "trainingQueue":
                for slot in range(5):
                    unit.trainingQueue[slot] = rng.randrange(234)
            elif name == "isVisible":
                for slot in range(9):
                    unit.isVisible[slot] = rng.random() < 0.8
            elif name in ("exists", "isCompleted", "isPowered", "isInterruptible"):
                setattr(unit, name, rng.random() < 0.95)
            elif kind.__name__ == "c_bool":
                setattr(unit, name, rng.random() < 0.06)  # several of these flags each stop a command
            elif kind.__name__ == "c_double":
                setattr(unit, name, rng.choice((0.0, 1.5)))
            elif name.endswith("Timer"):  # a running lockdown, stasis or maelstrom timer stops every command
                setattr(unit, name, rng.choice((50, 0, 0, 0, 0, 0, 0, 0, 0, 0)))
            elif kind.__name__ == "c_int":
                setattr(unit, name, rng.choice((-1, 0, 1, 2, 50, 300)))
        unit.player = rng.choice((ME, ME, ME, FOE, NEUTRAL))
        unit.positionX, unit.positionY = rng.randrange(2048), rng.randrange(2048)


def scramble_player(data, rng: random.Random) -> None:
    me = data.players[ME]
    me.minerals, me.gas = rng.choice((0, 100000)), rng.choice((0, 100000))
    me.supplyUsed[1] = rng.choice((0, 400))
    for field in ("isUnitAvailable", "hasResearched", "isResearchAvailable", "isResearching", "isUpgrading"):
        array = getattr(me, field)
        for index in range(len(array)):
            array[index] = rng.random() < 0.7
    for index in range(len(me.upgradeLevel)):
        me.upgradeLevel[index] = rng.randrange(4)
        me.maxUpgradeLevel[index] = rng.randrange(4)
    for index in range(len(me.completedUnitCount)):
        me.completedUnitCount[index] = rng.randrange(2)


def latency_compensation_calls(game, rng: random.Random) -> list:
    """Every command type the checks let through goes on into latency compensation."""
    calls = []
    units = sorted(game.getAllUnits(), key=lambda value: value.getID())
    for unit in units:
        for kind in vars(UnitCommandTypes).values():
            if not isinstance(kind, types.UnitCommandType):
                continue
            extra = {2: rng.randrange(234), 3: rng.randrange(234), 4: rng.randrange(234), 5: rng.randrange(234),
                     6: rng.randrange(47), 7: rng.randrange(64), 34: rng.randrange(5)}.get(int(kind), 0)
            target = rng.choice((None, rng.choice(units)))
            command = UnitCommand(unit, kind, target, rng.randrange(2048), rng.randrange(2048), extra)
            calls.append(lambda unit=unit, command=command: unit.canIssueCommand(command) and _latcom.execute(game, command))
    return calls


def every_method_in_scrambled_states(rounds: int, seed: int = 4242) -> list[str]:
    server, game = _rich_match()
    rng = random.Random(seed)
    unit_types = [value for value in vars(UnitTypes).values() if isinstance(value, types.UnitType)]
    failures = []
    for _ in range(rounds):
        scramble_units(server.data, rng)
        scramble_player(server.data, rng)
        server.data.remainingLatencyFrames = rng.choice((0, 2))
        server.frame()
        units = sorted(game.getAllUnits(), key=lambda value: value.getID())
        for unit in units:
            failures += _call_all(game, unit, Unit)
        failures += _call_all(game, game.self(), Player)
        failures += _run_calls(game_calls(game))
        tile = TilePosition(rng.randrange(64), rng.randrange(64))
        for unit in units:
            calls = [lambda unit=unit: game.canBuildHere(tile, rng.choice(unit_types), unit),
                     lambda unit=unit: game.canMake(rng.choice(unit_types), unit)]
            failures += _run_calls(calls)
        failures += _run_calls(latency_compensation_calls(game, rng))
        game._data.unitCommandCount = game._data.commandCount = game._data.stringCount = game._data.shapeCount = 0
    return failures


def test_every_method_in_scrambled_states():
    failures = every_method_in_scrambled_states(3)
    assert not failures, "\n".join(failures[:40])


def test_every_event_kind_reaches_the_game():
    server, game = _rich_match()
    server.data.eventStringCount = 1
    server.data.eventStrings[0].value = b"text"
    server.frame(
        (E.MATCH_END, 1, 0), (E.NUKE_DETECT, 200, 200), (E.PLAYER_LEFT, FOE, 0), (E.SAVE_GAME, 0, 0), (E.SEND_TEXT, 0, 0),
        (E.RECEIVE_TEXT, FOE, 0), (E.UNIT_EVADE, 1, 0), (E.UNIT_RENEGADE, 0, 0), (E.UNIT_MORPH, 3, 0), (E.UNIT_MORPH, 0, 0),
        (E.UNIT_SHOW, 0, 0), (E.UNIT_HIDE, 0, 0), (E.UNIT_CREATE, 0, 0), (E.UNIT_DESTROY, 0, 0), (E.UNIT_COMPLETE, 0, 0),
        (E.MENU_FRAME, 0, 0), (E.MATCH_FRAME, 0, 0),
    )
    module = AIModule()
    for event in game.getEvents():
        dispatch(module, event)
        assert Event(event) == event
    assert game.getEvents()[3].getText() == "text"


def game_calls(game) -> list:
    """Every Game method with required arguments, in each of its overloads."""
    unit, foe = game.getUnit(0), game.getUnit(1)
    me = game.self()
    position, tile, walk = Position(200, 200), TilePosition(10, 10), WalkPosition(40, 40)
    calls = [
        lambda: game.canMake(UnitTypes.Terran_Marine), lambda: game.canMake(UnitTypes.Terran_Marine, game.getUnit(4)),
        lambda: game.canResearch(TechTypes.Stim_Packs), lambda: game.canResearch(TechTypes.Stim_Packs, game.getUnit(7)),
        lambda: game.canUpgrade(UpgradeTypes.U_238_Shells), lambda: game.canUpgrade(UpgradeTypes.U_238_Shells, game.getUnit(6)),
        lambda: game.canBuildHere(tile, UnitTypes.Terran_Barracks),
        lambda: game.canBuildHere(tile, UnitTypes.Terran_Refinery, unit, True),
        lambda: game.canBuildHere(tile, UnitTypes.Protoss_Gateway, unit),
        lambda: game.canBuildHere(tile, UnitTypes.Zerg_Spawning_Pool, unit),
        lambda: game.getBuildLocation(UnitTypes.Terran_Barracks, tile),
        lambda: game.getBuildLocation(UnitTypes.Terran_Refinery, tile),
        lambda: game.getBuildLocation(UnitTypes.Terran_Barracks, tile, 8, True),
        lambda: game.getBestUnit(Lowest(Filter.HP), Filter.IsWorker, position),
        lambda: game.getBestUnit(Highest(Filter.HP), Filter.IsWorker, position, 500),
        lambda: game.getClosestUnit(position),
        lambda: game.getClosestUnit(position, Filter.IsWorker, 500),
        lambda: game.getClosestUnitInRectangle(position),
        lambda: game.getClosestUnitInRectangle(position, Filter.IsWorker, 0, 0, 500, 500),
        lambda: game.getDamageFrom(UnitTypes.Terran_Marine, UnitTypes.Zerg_Zergling),
        lambda: game.getDamageFrom(UnitTypes.Terran_Marine, UnitTypes.Zerg_Zergling, me, game.enemy()),
        lambda: game.getDamageTo(UnitTypes.Zerg_Zergling, UnitTypes.Terran_Marine),
        lambda: game.getDamageTo(UnitTypes.Zerg_Zergling, UnitTypes.Terran_Marine, game.enemy(), me),
        lambda: game.getForce(1), lambda: game.getPlayer(0), lambda: game.getRegion(0), lambda: game.getUnit(0),
        lambda: game.indexToUnit(0), lambda: game.getKeyState(K_A), lambda: game.getMouseState(M_LEFT),
        lambda: game.getGroundHeight(10, 10), lambda: game.getGroundHeight(tile),
        lambda: game.getRegionAt(100, 100), lambda: game.getRegionAt(position),
        lambda: game.getUnitsInRadius(position, 100), lambda: game.getUnitsInRadius(position, 100, Filter.IsWorker),
        lambda: game.getUnitsInRadius(position, radius=100), lambda: game.getUnitsInRadius(position, 100, pred=Filter.IsWorker),
        lambda: game.getUnitsInRadius(200, 200, 100), lambda: game.getUnitsInRadius(200, 200, 100, Filter.IsWorker),
        lambda: game.getUnitsInRectangle(0, 0, 500, 500), lambda: game.getUnitsInRectangle(Position(0, 0), position),
        lambda: game.getUnitsOnTile(3, 3), lambda: game.getUnitsOnTile(tile), lambda: game.getUnitsOnTile(tile, pred=Filter.IsWorker),
        lambda: game.hasCreep(10, 10), lambda: game.hasCreep(tile), lambda: game.hasPath(position, Position(300, 300)),
        lambda: game.hasPower(10, 10), lambda: game.hasPower(tile),
        lambda: game.hasPower(10, 10, 3, 2), lambda: game.hasPower(tile, 3, 2),
        lambda: game.hasPower(tile, unitType=UnitTypes.Protoss_Gateway),
        lambda: game.hasPowerPrecise(100, 100), lambda: game.hasPowerPrecise(position, unitType=UnitTypes.Protoss_Gateway),
        lambda: game.isBuildable(10, 10), lambda: game.isBuildable(tile, includeBuildings=True),
        lambda: game.isExplored(10, 10), lambda: game.isExplored(tile),
        lambda: game.isVisible(10, 10), lambda: game.isVisible(tile),
        lambda: game.isWalkable(40, 40), lambda: game.isWalkable(walk),
        lambda: game.isFlagEnabled(0), lambda: game.enableFlag(1),
        lambda: game.issueCommand(Unitset({unit}), UnitCommand.move(None, position)),
        lambda: game.issueCommand(Unitset({unit, game.getUnit(8)}), UnitCommand.attack(None, foe)),
        lambda: game.pingMinimap(10, 10), lambda: game.pingMinimap(position),
        lambda: game.printf("%d %s", 1, "text"), lambda: game.vPrintf("text"), lambda: game.sendText("%chi", 3),
        lambda: game.vSendText("text"), lambda: game.sendTextEx(True, "%d", 1), lambda: game.vSendTextEx(False, "text"),
        lambda: game.setAlliance(game.enemy()), lambda: game.setVision(game.enemy(), False),
        lambda: game.setCommandOptimizationLevel(2), lambda: game.setFrameSkip(2), lambda: game.setGUI(True),
        lambda: game.setLastError(), lambda: game.setLatCom(True), lambda: game.setLocalSpeed(0),
        lambda: game.setMap("map.scm"), lambda: game.setRevealAll(), lambda: game.setScreenPosition(10, 10),
        lambda: game.setScreenPosition(position), lambda: game.setTextSize(2),
        lambda: game.setClientInfo("info", 3), lambda: game.getClientInfo(3),
        lambda: game.registerEvent(lambda _interface: None),
        lambda: game.drawTextScreen(1, 1, "%d", 2), lambda: game.drawTextMap(position, "text"),
        lambda: game.drawTextMouse(1, 1, "text"), lambda: game.drawText(1, 1, 1, "text"), lambda: game.vDrawText(1, 1, 1, "t"),
    ]
    for suffix, ctype in (("Screen", 1), ("Map", 2), ("Mouse", 3)):
        calls += [
            lambda s=suffix: getattr(game, "drawBox" + s)(1, 1, 2, 2, 111),
            lambda s=suffix: getattr(game, "drawBox" + s)(position, position, 111, True),
            lambda s=suffix: getattr(game, "drawTriangle" + s)(1, 1, 2, 2, 3, 3, 111),
            lambda s=suffix: getattr(game, "drawTriangle" + s)(position, position, position, 111, True),
            lambda s=suffix: getattr(game, "drawCircle" + s)(1, 1, 2, 111),
            lambda s=suffix: getattr(game, "drawCircle" + s)(position, 2, 111, True),
            lambda s=suffix: getattr(game, "drawEllipse" + s)(1, 1, 2, 3, 111),
            lambda s=suffix: getattr(game, "drawEllipse" + s)(position, 2, 3, 111, True),
            lambda s=suffix: getattr(game, "drawDot" + s)(1, 1, 111),
            lambda s=suffix: getattr(game, "drawDot" + s)(position, 111),
            lambda s=suffix: getattr(game, "drawLine" + s)(1, 1, 2, 2, 111),
            lambda s=suffix: getattr(game, "drawLine" + s)(position, position, 111),
            lambda c=ctype: game.drawBox(c, 1, 1, 2, 2, 111),
            lambda c=ctype: game.drawTriangle(c, 1, 1, 2, 2, 3, 3, 111),
            lambda c=ctype: game.drawCircle(c, 1, 1, 2, 111),
            lambda c=ctype: game.drawEllipse(c, 1, 1, 2, 3, 111),
            lambda c=ctype: game.drawDot(c, 1, 1, 111),
            lambda c=ctype: game.drawLine(c, 1, 1, 2, 2, 111),
        ]
    return calls


def _run_calls(calls) -> list[str]:
    failures = []
    for call in calls:
        try:
            call()
        except Exception as error:  # noqa: BLE001 - reported with the call that raised it
            failures.append(f"{inspect.getsource(call).strip()}: {error!r}")
    return failures


def test_every_game_method():
    server, game = _rich_match()
    needs_arguments = []
    failures = []
    for name in _public_members(Game):
        member = getattr(Game, name)
        if not callable(member):
            continue
        parameters = list(inspect.signature(member).parameters.values())[1:]
        if any(item.default is item.empty and item.kind is not item.VAR_POSITIONAL for item in parameters) or (
                parameters and parameters[0].kind is parameters[0].VAR_POSITIONAL):
            needs_arguments.append(name)
            continue
        failures += _run_calls([getattr(game, name)]) if name != "self" else []
    server.data.commandCount = server.data.stringCount = server.data.shapeCount = 0
    assert not failures, "\n".join(failures[:40])

    calls = game_calls(game)
    called = {name for call in calls for name in call.__code__.co_names}
    called |= {"draw" + shape + suffix for shape in ("Box", "Triangle", "Circle", "Ellipse", "Dot", "Line")
               for suffix in ("Screen", "Map", "Mouse")}
    assert set(needs_arguments) <= called, sorted(set(needs_arguments) - called)
    failures = _run_calls(calls)
    assert not failures, "\n".join(failures[:40])


def test_game_checks_on_every_type_constant():
    _server, game = _rich_match()
    me_units = [game.getUnit(0), game.getUnit(4), game.getUnit(6), game.getUnit(7), game.getUnit(15), None]
    tile = TilePosition(10, 10)
    calls = []
    for value in vars(UnitTypes).values():
        if isinstance(value, types.UnitType):
            calls.append(lambda value=value: [game.canMake(value, unit) for unit in me_units])
            calls.append(lambda value=value: game.canBuildHere(tile, value, game.getUnit(0)))
            calls.append(lambda value=value: game.getBuildLocation(value, tile, 2))
    for value in vars(TechTypes).values():
        if isinstance(value, types.TechType):
            calls.append(lambda value=value: [game.canResearch(value, unit) for unit in me_units])
    for value in vars(UpgradeTypes).values():
        if isinstance(value, types.UpgradeType):
            calls.append(lambda value=value: [game.canUpgrade(value, unit) for unit in me_units])
    failures = _run_calls(calls)
    assert not failures, "\n".join(failures[:40])


def test_every_set_method():
    _server, game = _rich_match()
    unit, foe = game.getUnit(0), game.getUnit(1)
    position, tile = Position(200, 200), TilePosition(10, 10)
    units = Unitset({unit, game.getUnit(8), game.getUnit(9)})
    calls = [
        lambda: units.contains(unit), units.size, units.empty, lambda: units.copy().insert(foe),
        lambda: units.copy().erase(unit), lambda: units.copy().erase_if(lambda _unit: True),
        lambda: units | Unitset(), lambda: units & Unitset(), lambda: units - Unitset(), lambda: units ^ Unitset(),
        lambda: units.setClientInfo("info", 2), units.getPosition, units.getLoadedUnits, units.getInterceptors,
        units.getLarva, lambda: units.getUnitsInRadius(100), lambda: units.getUnitsInRadius(100, Filter.IsWorker),
        units.getClosestUnit, lambda: units.getClosestUnit(Filter.IsWorker, 100),
        lambda: units.issueCommand(UnitCommand.stop(None)), lambda: Unitset().issueCommand(UnitCommand.stop(None)),
        lambda: units.attack(foe), lambda: units.attack(position, True), lambda: units.build(UnitTypes.Terran_Marine),
        lambda: units.build(UnitTypes.Terran_Barracks, tile), lambda: units.buildAddon(UnitTypes.Terran_Machine_Shop),
        lambda: units.train(UnitTypes.Terran_Marine), lambda: units.morph(UnitTypes.Zerg_Lurker),
        lambda: units.setRallyPoint(position), lambda: units.setRallyPoint(foe), lambda: units.move(position),
        lambda: units.patrol(position), units.holdPosition, units.stop, lambda: units.follow(foe),
        lambda: units.gather(game.getUnit(2)), units.returnCargo, lambda: units.repair(foe), units.burrow, units.unburrow,
        units.cloak, units.decloak, units.siege, units.unsiege, units.lift, lambda: units.load(foe), units.unloadAll,
        lambda: units.unloadAll(True), lambda: units.unloadAll(position), lambda: units.rightClick(foe),
        lambda: units.rightClick(position), units.haltConstruction, units.cancelConstruction, units.cancelAddon,
        units.cancelTrain, units.cancelMorph, units.cancelResearch, units.cancelUpgrade,
        lambda: units.useTech(TechTypes.Stim_Packs), lambda: units.useTech(TechTypes.Lockdown, foe),
        lambda: units.useTech(TechTypes.Spider_Mines, position), lambda: Unitset.none.copy(),
        game.getPlayers().getUnits, game.getPlayers().getRaces, lambda: game.getPlayers().setAlliance(),
        game.getForces().getPlayers, game.getAllRegions().getCenter, game.getAllRegions().getUnits,
        lambda: game.getAllRegions().getUnits(Filter.IsWorker), lambda: game.getBullets().size(),
    ]
    failures = _run_calls(calls)
    for name in _public_members(Unitset, set):
        assert name in {item for call in calls for item in getattr(getattr(call, "__code__", None), "co_names", ())} | {
            getattr(call, "__name__", "") for call in calls} | {"none"}, name
    assert not failures, "\n".join(failures[:40])


def test_every_unit_command_factory_and_getter():
    _server, game = _rich_match()
    unit, foe = game.getUnit(0), game.getUnit(1)
    position, tile = Position(200, 200), TilePosition(10, 10)
    commands = [
        UnitCommand.attack(unit, foe), UnitCommand.attack(unit, position, True),
        UnitCommand.build(unit, tile, UnitTypes.Terran_Barracks), UnitCommand.buildAddon(unit, UnitTypes.Terran_Machine_Shop),
        UnitCommand.train(unit, UnitTypes.Terran_Marine), UnitCommand.morph(unit, UnitTypes.Zerg_Lurker),
        UnitCommand.research(unit, TechTypes.Stim_Packs), UnitCommand.upgrade(unit, UpgradeTypes.U_238_Shells),
        UnitCommand.setRallyPoint(unit, position), UnitCommand.setRallyPoint(unit, foe), UnitCommand.move(unit, position),
        UnitCommand.patrol(unit, position), UnitCommand.holdPosition(unit), UnitCommand.stop(unit),
        UnitCommand.follow(unit, foe), UnitCommand.gather(unit, game.getUnit(2)), UnitCommand.returnCargo(unit),
        UnitCommand.repair(unit, foe), UnitCommand.burrow(unit), UnitCommand.unburrow(unit), UnitCommand.cloak(unit),
        UnitCommand.decloak(unit), UnitCommand.siege(unit), UnitCommand.unsiege(unit), UnitCommand.lift(unit),
        UnitCommand.land(unit, tile), UnitCommand.load(unit, foe), UnitCommand.unload(unit, foe),
        UnitCommand.unloadAll(unit), UnitCommand.unloadAll(unit, position), UnitCommand.rightClick(unit, foe),
        UnitCommand.rightClick(unit, position), UnitCommand.haltConstruction(unit), UnitCommand.cancelConstruction(unit),
        UnitCommand.cancelAddon(unit), UnitCommand.cancelTrain(unit), UnitCommand.cancelTrain(unit, 1),
        UnitCommand.cancelMorph(unit), UnitCommand.cancelResearch(unit), UnitCommand.cancelUpgrade(unit),
        UnitCommand.useTech(unit, TechTypes.Stim_Packs), UnitCommand.useTech(unit, TechTypes.Lockdown, foe),
        UnitCommand.useTech(unit, TechTypes.Spider_Mines, position), UnitCommand.placeCOP(unit, tile),
    ]
    getters = [name for name in _public_members(UnitCommand) if name.startswith(("get", "is"))]
    assert getters
    for command in commands:
        for name in getters:
            getattr(command, name)()
        assert command == UnitCommand(command)
        assert isinstance(unit.canIssueCommand(command), bool)
        assert isinstance(unit.canIssueCommandGrouped(command), bool)
        assert isinstance(unit.issueCommand(command), bool)
        game._data.unitCommandCount = 0


READY_COMMANDS = {
    "move": lambda g: g.getUnit(0).move(Position(200, 200)),
    "attack move": lambda g: g.getUnit(0).attack(Position(200, 200)),
    "attack unit": lambda g: g.getUnit(0).attack(g.getUnit(1)),
    "gather": lambda g: g.getUnit(0).gather(g.getUnit(2)),
    "build": lambda g: g.getUnit(0).build(UnitTypes.Terran_Supply_Depot, TilePosition(20, 20)),
    "train": lambda g: g.getUnit(4).train(UnitTypes.Terran_Marine),
    "train at a full supply check": lambda g: g.getUnit(15).train(UnitTypes.Zerg_Drone) in (True, False),
    "morph": lambda g: g.getUnit(17).morph(UnitTypes.Zerg_Hatchery) in (True, False),
    "research": lambda g: g.getUnit(26).research(TechTypes.Restoration),
    "upgrade": lambda g: g.getUnit(27).upgrade(UpgradeTypes.Terran_Infantry_Armor),
    "stop": lambda g: g.getUnit(0).stop(),
    "hold": lambda g: g.getUnit(0).holdPosition(),
    "patrol": lambda g: g.getUnit(0).patrol(Position(200, 200)),
    "follow": lambda g: g.getUnit(0).follow(g.getUnit(8)),
    "repair": lambda g: g.getUnit(0).repair(g.getUnit(9)) in (True, False),
    "rally position": lambda g: g.getUnit(4).setRallyPoint(Position(200, 200)),
    "rally unit": lambda g: g.getUnit(4).setRallyPoint(g.getUnit(8)),
    "use tech": lambda g: g.getUnit(8).useTech(TechTypes.Lockdown, g.getUnit(1)) in (True, False),
    "use tech position": lambda g: g.getUnit(8).useTech(TechTypes.Nuclear_Strike, Position(200, 200)) in (True, False),
    "use tech self": lambda g: g.getUnit(8).useTech(TechTypes.Personnel_Cloaking) in (True, False),
    "siege": lambda g: g.getUnit(9).siege(),
    "unsiege": lambda g: g.getUnit(10).unsiege(),
    "lift": lambda g: g.getUnit(4).lift(),
    "land": lambda g: g.getUnit(13).land(TilePosition(30, 30)) in (True, False),
    "unburrow": lambda g: g.getUnit(17).unburrow(),
    "load": lambda g: g.getUnit(11).load(g.getUnit(8)),
    "unload all": lambda g: g.getUnit(11).unloadAll() in (True, False),
    "right click": lambda g: g.getUnit(0).rightClick(Position(200, 200)),
    "right click unit": lambda g: g.getUnit(0).rightClick(g.getUnit(2)),
    "cancel train": lambda g: g.getUnit(5).cancelTrain(),
    "cancel train slot": lambda g: g.getUnit(5).cancelTrain(0),
    "cancel upgrade": lambda g: g.getUnit(6).cancelUpgrade(),
    "cancel research": lambda g: g.getUnit(7).cancelResearch(),
    "cancel morph": lambda g: g.getUnit(18).cancelMorph() in (True, False),
    "halt": lambda g: g.getUnit(0).haltConstruction() in (True, False),
    "cancel construction": lambda g: g.getUnit(14).cancelConstruction() in (True, False),
    "cancel addon": lambda g: g.getUnit(14).cancelAddon() in (True, False),
    "build addon": lambda g: g.getUnit(14).buildAddon(UnitTypes.Terran_Machine_Shop) in (True, False),
    "recall": lambda g: g.getUnit(22).useTech(TechTypes.Psionic_Storm, Position(200, 200)) in (True, False),
}


@pytest.mark.parametrize("name", READY_COMMANDS)
def test_a_ready_unit_takes_the_command(name):
    _server, game = _rich_match()
    assert READY_COMMANDS[name](game), game.getLastError()


def test_colors_from_rgb_components():
    color = types.Color(255, 0, 0)
    assert all(isinstance(part, int) for part in (color.red(), color.green(), color.blue()))
    with pytest.raises(TypeError):
        types.Color(1, 2)
