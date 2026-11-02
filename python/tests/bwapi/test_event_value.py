"""Pinned BWAPI Event value semantics; no producer or live-game validation."""
from copy import copy, deepcopy

import pytest

from bwrapi.bwapi import Event
from bwrapi.bwapi.type_ids import EventTypeId
from bwrapi.bwapi.position import Position


# Expected payloads come from pinned Event.cpp, independent of factory internals.
FACTORIES = (
    ("MatchStart", (), EventTypeId.MATCH_START),
    ("MatchEnd", (True,), EventTypeId.MATCH_END),
    ("MatchFrame", (), EventTypeId.MATCH_FRAME),
    ("MenuFrame", (), EventTypeId.MENU_FRAME),
    ("SendText", ("hello",), EventTypeId.SEND_TEXT),
    ("ReceiveText", ("player", "hello"), EventTypeId.RECEIVE_TEXT),
    ("PlayerLeft", ("player",), EventTypeId.PLAYER_LEFT),
    ("NukeDetect", (Position(21, 32),), EventTypeId.NUKE_DETECT),
    ("UnitDiscover", ("unit",), EventTypeId.UNIT_DISCOVER),
    ("UnitEvade", ("unit",), EventTypeId.UNIT_EVADE),
    ("UnitShow", ("unit",), EventTypeId.UNIT_SHOW),
    ("UnitHide", ("unit",), EventTypeId.UNIT_HIDE),
    ("UnitCreate", ("unit",), EventTypeId.UNIT_CREATE),
    ("UnitDestroy", ("unit",), EventTypeId.UNIT_DESTROY),
    ("UnitMorph", ("unit",), EventTypeId.UNIT_MORPH),
    ("UnitRenegade", ("unit",), EventTypeId.UNIT_RENEGADE),
    ("SaveGame", ("save",), EventTypeId.SAVE_GAME),
    ("UnitComplete", ("unit",), EventTypeId.UNIT_COMPLETE),
)


class Handle:
    def __eq__(self, other):
        return isinstance(other, Handle)

    def __deepcopy__(self, memo):
        raise AssertionError("Event must not clone a handle or its owner")


@pytest.mark.parametrize("name,args,event_type", FACTORIES)
def test_official_factories_payload_and_unrelated_defaults(name, args, event_type):
    unit, player = Handle(), Handle()
    args = tuple(unit if value == "unit" else player if value == "player" else value for value in args)
    event = getattr(Event, name)(*args)
    assert event.getType() == event_type
    assert event.getUnit() is (unit if name.startswith("Unit") else None)
    assert event.getPlayer() is (player if name in ("ReceiveText", "PlayerLeft") else None)
    assert event.getPosition() == (Position(21, 32) if name == "NukeDetect" else Position(32000, 32032))
    assert event.getText() == ("hello" if name in ("SendText", "ReceiveText") else "save" if name == "SaveGame" else "")
    assert event.isWinner() is (name == "MatchEnd")


def test_default_and_existing_three_positional_arguments():
    event = Event()
    assert event == Event(EventTypeId.NONE)
    assert event.getPosition() == Position(32000, 32032)
    assert event.getUnit() is None and event.getPlayer() is None
    assert event.getText() == "" and event.isWinner() is False
    unit, player = Handle(), Handle()
    event = Event(EventTypeId.UNIT_CREATE, unit, player)
    assert event.getUnit() is unit and event.getPlayer() is player


@pytest.mark.parametrize("factory,args", (("SendText", ()), ("ReceiveText", (None,)), ("SaveGame", ())))
def test_null_text_and_owned_empty_text_are_different_values(factory, args):
    null = getattr(Event, factory)(*args)
    empty = getattr(Event, factory)(*args, "")
    assert null.getText() == empty.getText() == ""
    assert null != empty
    assert null.setText("") == empty
    assert null.setText(None) != empty


def test_all_setters_return_self_and_preserve_other_fields():
    unit, player = Handle(), Handle()
    event = Event()
    assert event.setType(EventTypeId.MATCH_END) is event
    assert event.setPosition(Position(1, 2)) is event
    assert event.setText("owned") is event
    assert event.setUnit(unit) is event
    assert event.setPlayer(player) is event
    assert event.setWinner(True) is event
    assert event == Event(EventTypeId.MATCH_END, unit, player, Position(1, 2), "owned", True)
    assert event.setText("new").getText() == "new"
    assert event.setText(None).getText() == ""


def test_none_unknown_and_invalid_positions_remain_distinct():
    assert Event().getPosition() == Position(32000, 32032)
    assert Event.NukeDetect(Position(32000, 32064)).getPosition() == Position(32000, 32064)
    assert Event.NukeDetect(Position(32000, 32000)).getPosition() == Position(32000, 32000)
    assert Event.NukeDetect(Position(32000, 32064)) != Event.NukeDetect(Position(32000, 32032))


@pytest.mark.parametrize("copier", (copy, deepcopy))
def test_copy_owns_value_but_retains_handle_identity(copier):
    unit, player = Handle(), Handle()
    original = Event.ReceiveText(player, "text").setUnit(unit).setPosition(Position(8, 9)).setWinner(True)
    copied = copier(original)
    assert copied is not original and copied == original
    assert copied.getUnit() is unit and copied.getPlayer() is player
    copied.setText("").setPosition(Position(4, 5)).setWinner(False)
    assert original.getText() == "text"
    assert original.getPosition() == Position(8, 9)
    assert original.isWinner() is True
    null = copier(Event.SendText())
    assert null == Event.SendText() and null != Event.SendText("")


def test_equal_python_handles_do_not_replace_pointer_identity():
    first, second = Handle(), Handle()
    assert first == second
    assert Event.UnitCreate(first) != Event.UnitCreate(second)
    assert Event.PlayerLeft(first) != Event.PlayerLeft(second)
    assert Event.UnitCreate(first) == Event.UnitCreate(first)
    assert Event().__eq__(object()) is NotImplemented
    with pytest.raises(TypeError):
        hash(Event())


def test_text_is_owned_and_c_string_nul_is_respected():
    text = "hello"
    event = Event.SendText(text)
    text = "changed"
    assert event.getText() == "hello"
    assert Event.SendText("hello\0suffix") == Event.SendText("hello")
    assert Event.SaveGame("\0suffix") == Event.SaveGame("")
