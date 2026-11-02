"""Pinned GameWrapper stream behavior and offline runner current-Game ownership."""
from types import SimpleNamespace

import pytest

from bwrapi.bwapi import global_game
from bwrapi.bwapi.global_game import GameWrapper, Broodwar, endl, ends, flush


@pytest.fixture(autouse=True)
def isolated_pointer_and_stream():
    global_game._set_current_game(None)
    Broodwar._stream = ""
    yield
    global_game._set_current_game(None)
    Broodwar._stream = ""


def test_dynamic_raw_pointer_and_wrapper_method_lookup():
    first = SimpleNamespace(getFrameCount=lambda: 3, _retired=False)
    second = SimpleNamespace(getFrameCount=lambda: 9, _retired=False)
    assert global_game.BroodwarPtr is None
    with pytest.raises(RuntimeError, match="requires a running match"):
        Broodwar.getFrameCount()
    global_game._set_current_game(first)
    assert global_game.BroodwarPtr is first
    assert Broodwar.getFrameCount() == 3
    global_game._set_current_game(second)
    assert global_game.BroodwarPtr is second
    assert Broodwar.getFrameCount() == 9
    second._retired = True
    assert global_game.BroodwarPtr is None
    assert global_game._get_current_game() is None
    with pytest.raises(AttributeError):
        getattr(global_game, "not_a_public_pointer")


def printer():
    calls = []
    value = SimpleNamespace(printf=lambda *args: calls.append(args), _retired=False)
    global_game._set_current_game(value)
    return calls


def test_generic_insert_accumulates_and_literal_newline_does_not_flush():
    calls = printer()
    value = GameWrapper()
    assert value << "100% " << 7 << " " << True << " " << 1.23456789 << "\n" is value
    assert calls == []
    assert value.flush() is None
    assert calls == [("%s", "100% 7 1 1.23457\n")]
    value.flush()
    assert len(calls) == 1


@pytest.mark.parametrize("manipulator,expected", [(endl, "hello\n"), (ends, "hello")])
def test_endl_and_ends_are_the_only_recognized_automatic_game_flushes(manipulator, expected):
    calls = printer()
    value = GameWrapper()
    assert value << "hello" << manipulator is value
    assert calls == [("%s", expected)]
    assert value._stream == ""


def test_std_flush_manipulator_keeps_buffer_while_explicit_flush_prints():
    calls = printer()
    value = GameWrapper()
    value << "kept" << flush
    assert calls == [] and value._stream == "kept"
    value.flush()
    assert calls == [("%s", "kept")]


def test_absent_pointer_retains_buffer_and_ends_c_string_boundary():
    value = GameWrapper()
    value << "before" << endl
    assert value._stream == "before\n"
    calls = printer()
    value << "after\0hidden" << ends
    assert calls == [("%s", "before\nafter")]
    assert value._stream == ""
    value << ends
    assert calls[-1] == ("%s", "")


def test_failed_printf_does_not_discard_the_stream():
    value = GameWrapper()
    value << "retry"
    global_game._set_current_game(SimpleNamespace(
        printf=lambda *args: (_ for _ in ()).throw(ValueError("printf failed")), _retired=False))
    with pytest.raises(ValueError, match="printf failed"):
        value.flush()
    assert value._stream == "retry"
    calls = printer()
    value.flush()
    assert calls == [("%s", "retry")]
