import copy
import re

import pytest

from bwrapi.bwapi.position import Position, TilePosition
from bwrapi.bwapi import global_game
from bwrapi.bwapi.api_namespaces import WalkPosition
from bwrapi.bwapi.unit import Unit
from bwrapi.bwapi.unit_command import UnitCommand
from upstream_root import SOURCE, requires_upstream


_UPSTREAM = SOURCE
_PINNED = {
    "include/BWAPI/UnitCommand.h": "a7c63b1debb7f779db665d03e65671240a7dcce17852b09f2fde2804941bbf1f",
    "BWAPILIB/UnitCommand.cpp": "fc56044d3597bcb764c956e8f74606fe2158f0f86d1994fa908ec17b9332e214",
    "BWAPILIB/Source/Position.cpp": "b415528f850e8139d92f08395e198326689867ebcc7d6524474c72f5012116f0",
    "include/BWAPI/UnitCommandType.h": "45e7c4419dff1131e81e24a890bf7eb544f1f0d3553daa638ddb54dbb011ed98",
    "include/BWAPI/Position.h": "84bd6df2c4349e591681746ab000b644345d793d6efed0cea14386413f34314d",
}


class _Game:
    _retired = False

    def __init__(self, width=64, height=32):
        self.width, self.height = width, height
        self._units_by_tag = {}
        self._unit_handles_by_birth = {}

    def mapWidth(self): return self.width
    def mapHeight(self): return self.height
    def getUnit(self, tag): return self._units_by_tag.get(tag)


class _Unit(Unit):
    def __init__(self, game, tag, *, burrowed=False, cloaked=False, sieged=False):
        self.game, self.tag = game, tag
        self.burrowed, self.cloaked, self.sieged = burrowed, cloaked, sieged
        self.alive, self.known = True, True
        self.submitted = 0
        game._units_by_tag[tag] = self

    def getTag(self): return self.tag
    def exists(self): return self.alive
    def isBurrowed(self): return self.burrowed
    def isCloaked(self): return self.cloaked
    def isSieged(self): return self.sieged
    def issueCommand(self, command):
        self.submitted += 1
        raise AssertionError("value construction submitted a game command")
    def __eq__(self, other): return isinstance(other, _Unit)
    __hash__ = object.__hash__


@pytest.fixture(autouse=True)
def _unbound_global():
    previous = global_game._get_current_binding()
    global_game._set_current_game(None)
    yield
    global_game._set_current_game(previous[0], owner=previous[1])


# Independent expected tuples transcribed from the pinned UnitCommand.cpp factories.
# "P"/"T" select pixel/tile input; "U" is the target handle. They are not derived
# from the Python factory implementation or its command-kind mapping.
_FACTORIES = (
    ("attack", ("U", True), 1, "U", 32000, 32032, 1),
    ("attack", ("P", True), 0, None, 7, 9, 1),
    ("build", ("T", 106), 2, None, 3, 4, 106),
    ("buildAddon", (107,), 3, None, 32000, 32032, 107),
    ("train", (0,), 4, None, 32000, 32032, 0),
    ("morph", (38,), 5, None, 32000, 32032, 38),
    ("research", (19,), 6, None, 32000, 32032, 19),
    ("upgrade", (1,), 7, None, 32000, 32032, 1),
    ("setRallyPoint", ("P",), 8, None, 7, 9, 0),
    ("setRallyPoint", ("U",), 9, "U", 32000, 32032, 0),
    ("move", ("P", True), 10, None, 7, 9, 1),
    ("patrol", ("P", True), 11, None, 7, 9, 1),
    ("holdPosition", (True,), 12, None, 32000, 32032, 1),
    ("stop", (True,), 13, None, 32000, 32032, 1),
    ("follow", ("U", True), 14, "U", 32000, 32032, 1),
    ("gather", ("U", True), 15, "U", 32000, 32032, 1),
    ("returnCargo", (True,), 16, None, 32000, 32032, 1),
    ("repair", ("U", True), 17, "U", 32000, 32032, 1),
    ("burrow", (), 18, None, 32000, 32032, 0),
    ("unburrow", (), 19, None, 32000, 32032, 0),
    ("cloak", (), 20, None, 32000, 32032, 0),
    ("decloak", (), 21, None, 32000, 32032, 0),
    ("siege", (), 22, None, 32000, 32032, 0),
    ("unsiege", (), 23, None, 32000, 32032, 0),
    ("lift", (), 24, None, 32000, 32032, 0),
    ("land", ("T",), 25, None, 3, 4, 0),
    ("load", ("U", True), 26, "U", 32000, 32032, 1),
    ("unload", ("U",), 27, "U", 32000, 32032, 0),
    ("unloadAll", (True,), 28, None, 32000, 32032, 1),
    ("unloadAll", ("P", True), 29, None, 7, 9, 1),
    ("rightClick", ("P", True), 30, None, 7, 9, 1),
    ("rightClick", ("U", True), 31, "U", 32000, 32032, 1),
    ("haltConstruction", (), 32, None, 32000, 32032, 0),
    ("cancelConstruction", (), 33, None, 32000, 32032, 0),
    ("cancelAddon", (), 34, None, 32000, 32032, 0),
    ("cancelTrain", (), 35, None, 32000, 32032, -2),
    ("cancelMorph", (), 37, None, 32000, 32032, 0),
    ("cancelResearch", (), 38, None, 32000, 32032, 0),
    ("cancelUpgrade", (), 39, None, 32000, 32032, 0),
    ("useTech", (19,), 40, None, 32000, 32032, 19),
    ("useTech", (19, "P"), 41, None, 7, 9, 19),
    ("useTech", (19, "U"), 42, "U", 32000, 32032, 19),
    ("placeCOP", ("T",), 43, None, 3, 4, 0),
)


@requires_upstream
def test_pinned_factory_inventory_and_semantic_source():
    texts = {}
    for path, digest in _PINNED.items():
        raw = (_UPSTREAM / path).read_bytes()
        texts[path] = raw.decode()
    names = re.findall(r"static UnitCommand (\w+)\(", texts["include/BWAPI/UnitCommand.h"])
    assert len(names) == len(_FACTORIES) == 43
    assert sorted(names) == sorted(case[0] for case in _FACTORIES)
    assert "std::tie(type, target, x, y, extra)" in texts["BWAPILIB/UnitCommand.cpp"]
    assert "if ( !BroodwarPtr )" in texts["BWAPILIB/Source/Position.cpp"]


@pytest.mark.parametrize("name,args,type_id,target,x,y,extra", _FACTORIES)
def test_every_official_factory_overload(name, args, type_id, target, x, y, extra):
    game = _Game()
    actor, victim = _Unit(game, "actor"), _Unit(game, "target")
    inputs = {"P": Position(7, 9), "T": TilePosition(3, 4), "U": victim}
    actual_args = tuple(inputs[v] if isinstance(v, str) else v for v in args)
    command = getattr(UnitCommand, name)(actor, *actual_args)
    assert command.unit is actor
    assert int(command.type) == type_id
    assert command.target is (victim if target == "U" else None)
    assert (command.x, command.y, command.extra) == (x, y, extra)
    assert actor.submitted == victim.submitted == 0


def test_constructors_mutable_fields_copy_and_handle_identity():
    command = UnitCommand()
    assert command.getType() == 44 and command.getUnit() is None and command.getTarget() is None
    assert (command.x, command.y, command.extra) == (32000, 32032, 0)
    assert command.getTargetPosition() == Position(32000, 32032)
    assert command.getTargetTilePosition() == TilePosition(1000, 1001)
    game = _Game()
    actor, victim = _Unit(game, "a"), _Unit(game, "b")
    two = UnitCommand(actor, 10)
    assert (two.x, two.y, two.extra) == (32000, 32032, 0)
    command = UnitCommand(actor, 31, victim, -8, 77, 1)
    for copied in (UnitCommand(command), copy.copy(command), copy.deepcopy(command)):
        assert copied is not command and copied == command
        assert copied.unit is actor and copied.target is victim
        copied.x = 5
        assert command.x == -8
    other = UnitCommand(_Unit(game, "c"), 31, victim, -8, 77, 1)
    assert other == command  # actor is deliberately absent from official equality
    other.target = _Unit(game, "d")
    assert victim == other.target and other != command  # target pointer identity matters


@pytest.mark.parametrize("type_id", range(46))
def test_queued_types_ignore_nonqueue_payload(type_id):
    command = UnitCommand(None, type_id, extra=-3)
    assert command.isQueued() is (type_id in (0, 1, 10, 11, 12, 13, 14, 15, 16, 17, 26, 28, 29, 30, 31))
    command.extra = 0
    assert command.isQueued() is False


def test_getters_unit_tech_upgrade_slot_and_coordinate_scales():
    for type_id in range(46):
        command = UnitCommand(None, type_id, x=-33, y=63, extra=7)
        assert command.getUnitType() == (7 if type_id in (2, 3, 4, 5) else 228)
        assert command.getTechType() == (7 if type_id in (6, 40, 41, 42) else 44)
        assert command.getUpgradeType() == (7 if type_id == 7 else 61)
        assert command.getSlot() == (7 if type_id == 36 else -1)
        if type_id in (2, 25, 43):
            assert command.getTargetPosition() == Position(-1056, 2016)
            assert command.getTargetTilePosition() == TilePosition(-33, 63)
        else:
            assert command.getTargetPosition() == Position(-33, 63)
            assert command.getTargetTilePosition() == TilePosition(-1, 1)


@pytest.mark.parametrize("slot,type_id,returned", [(-2, 35, -1), (-1, 35, -1), (-9, 35, -1), (0, 36, 0), (3, 36, 3)])
def test_cancel_train_payload_is_preserved(slot, type_id, returned):
    command = UnitCommand.cancelTrain(None, slot)
    assert (int(command.type), command.extra, command.getSlot()) == (type_id, slot, returned)


@pytest.mark.parametrize("tech,flag,false_type,true_type", [(11, "burrowed", 18, 19), (9, "cloaked", 20, 21), (10, "cloaked", 20, 21), (5, "sieged", 22, 23)])
def test_toggle_is_decided_at_factory_time_and_keeps_tech_payload(tech, flag, false_type, true_type):
    actor = _Unit(_Game(), "a")
    first = UnitCommand.useTech(actor, tech)
    setattr(actor, flag, True)
    second = UnitCommand.useTech(actor, tech)
    assert (int(first.type), first.extra) == (false_type, tech)
    assert (int(second.type), second.extra) == (true_type, tech)
    assert int(first.type) == false_type
    assert first.getTechType() == second.getTechType() == 44


def test_assign_target_copy_clamp_uses_current_global_not_actor_game():
    actor = _Unit(_Game(200, 200), "a")
    original = Position(-4, 9000)
    command = UnitCommand.move(actor, original)
    assert (command.x, command.y) == (0, 8191)
    assert original == Position(-4, 9000)
    global_game._set_current_game(_Game(8, 4))
    command = UnitCommand.move(actor, original)
    assert (command.x, command.y) == (0, 127)
    tile = TilePosition(900, -4)
    command = UnitCommand.build(actor, tile, 106)
    assert (command.x, command.y) == (7, 0)
    assert tile == TilePosition(900, -4)
    command = UnitCommand(actor, 10, x=-4, y=9000)
    assert (command.x, command.y) == (-4, 9000)  # raw ctor never clamps


def test_current_map_unknown_is_not_replaced_by_maximum_map():
    class Unknown(_Game):
        def mapWidth(self): return 0  # official default for an unknown map
    global_game._set_current_game(Unknown())
    command = UnitCommand.move(None, Position(1, 2))
    assert (command.x, command.y) == (-1, 2)  # zero-width map clamps x only


def test_walk_target_preserves_input_and_uses_scale_eight():
    original = WalkPosition(-3, 4000)
    command = UnitCommand()
    assert command.assignTarget(original) is None
    assert (command.x, command.y) == (0, 1023)
    assert original == WalkPosition(-3, 4000)
    global_game._set_current_game(_Game(8, 4))
    command.assignTarget(original)
    assert (command.x, command.y) == (0, 15)
    command.assignTarget(WalkPosition(4000, 1))
    assert (command.x, command.y) == (31, 1)
