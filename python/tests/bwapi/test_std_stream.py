"""Offline checks for the pinned BWAPI standard streams and Python console proxy."""
from __future__ import annotations

import re

import pytest

from bwrapi.bwapi import global_game
from bwrapi.bwapi import std_stream
from bwrapi.bwapi.types import UnitType
from upstream_root import SOURCE, requires_upstream


_UPSTREAM = SOURCE


def _active_cpp(source: str) -> str:
    """Remove nested disabled preprocessor branches for the pinned source check."""
    active: list[str] = []
    disabled_depth = 0
    for line in source.splitlines():
        directive = line.strip()
        if disabled_depth:
            if re.match(r"#\s*(if|ifdef|ifndef)\b", directive):
                disabled_depth += 1
            elif re.match(r"#\s*endif\b", directive):
                disabled_depth -= 1
            continue
        if re.fullmatch(r"#\s*if\s+0(?:\s+.*)?", directive):
            disabled_depth = 1
            continue
        active.append(line)
    assert disabled_depth == 0
    return "\n".join(active)


@pytest.fixture(autouse=True)
def no_current_game():
    global_game._set_current_game(None)
    yield
    global_game._set_current_game(None)


@requires_upstream
def test_pinned_primary_stream_active_aliases():
    source_path = _UPSTREAM / "BWAPILIB" / "Source" / "Streams.cpp"
    header_path = _UPSTREAM / "include" / "BWAPI" / "Streams.h"
    type_path = _UPSTREAM / "include" / "BWAPI" / "Type.h"
    source = source_path.read_text(encoding="utf-8")
    header = header_path.read_text(encoding="utf-8")
    type_header = type_path.read_text(encoding="utf-8")

    active = _active_cpp(source)
    for declaration in (
        "std::ostream& bwout = std::cout;",
        "std::ostream& bwerr = std::cerr;",
        "std::ostream& out = std::cout;",
        "std::ostream& err = std::cerr;",
    ):
        assert declaration in active
    assert "out_stream" not in active
    assert "extern std::ostream& bwout;" in header
    assert "extern std::ostream& out;" in header
    assert "return os << t.getName();" in type_header


class _RecordingTextStream:
    def __init__(self) -> None:
        self.events: list[tuple[str, str | None]] = []
        self.parts: list[str] = []

    def write(self, text: str) -> int:
        self.events.append(("write", text))
        self.parts.append(text)
        return len(text)

    def flush(self) -> None:
        self.events.append(("flush", None))

    def getvalue(self) -> str:
        return "".join(self.parts)


def test_alias_identity_and_stdout_stderr_capture_are_separate(monkeypatch):
    stdout = _RecordingTextStream()
    stderr = _RecordingTextStream()
    monkeypatch.setattr(std_stream.sys, "stdout", stdout)
    monkeypatch.setattr(std_stream.sys, "stderr", stderr)
    global_game._set_current_game(None)

    assert std_stream.cout is std_stream.out is std_stream.bwout
    assert std_stream.cerr is std_stream.err is std_stream.bwerr
    assert std_stream.cout is not std_stream.cerr

    assert std_stream.out << "visible" is std_stream.out
    assert std_stream.err << "problem" is std_stream.err
    assert stdout.getvalue() == "visible"
    assert stderr.getvalue() == "problem"
    assert stderr.events == [("write", "problem"), ("flush", None)]


def test_console_output_is_game_independent_and_follows_current_sys_binding(monkeypatch):
    first = _RecordingTextStream()
    second = _RecordingTextStream()
    calls: list[tuple[object, ...]] = []
    game = type("GameSpy", (), {"printf": lambda self, *args: calls.append(args)})()
    global_game._set_current_game(game)

    monkeypatch.setattr(std_stream.sys, "stdout", first)
    std_stream.bwout << "first"
    monkeypatch.setattr(std_stream.sys, "stdout", second)
    std_stream.cout << "second"

    assert first.getvalue() == "first"
    assert second.getvalue() == "second"
    assert calls == []
    global_game._set_current_game(None)


def test_stdout_manipulators_write_in_order_and_flush_only_when_requested(monkeypatch):
    target = _RecordingTextStream()
    monkeypatch.setattr(std_stream.sys, "stdout", target)

    std_stream.out << "prefix" << 7
    assert target.events == [("write", "prefix"), ("write", "7")]

    assert std_stream.out << std_stream.endl is std_stream.out
    assert target.events[-2:] == [("write", "\n"), ("flush", None)]
    flushes_after_endl = sum(event[0] == "flush" for event in target.events)

    assert std_stream.ends(std_stream.out) is std_stream.out
    assert target.events[-1] == ("write", "\0")
    assert sum(event[0] == "flush" for event in target.events) == flushes_after_endl

    assert std_stream.out << std_stream.flush is std_stream.out
    assert target.events[-1] == ("flush", None)


def test_direct_manipulators_and_scalar_and_type_name_formatting(monkeypatch):
    target = _RecordingTextStream()
    monkeypatch.setattr(std_stream.sys, "stdout", target)
    unit_type = UnitType(0)

    std_stream.out << True << False << " " << 1.23456789 << " " << unit_type
    assert target.getvalue() == f"10 1.23457 {unit_type.getName()}"

    assert std_stream.endl(std_stream.out) is std_stream.out
    assert target.getvalue().endswith("\n")
    before_ends_flushes = sum(event[0] == "flush" for event in target.events)
    assert std_stream.ends(std_stream.out) is std_stream.out
    assert target.getvalue().endswith("\n\0")
    assert sum(event[0] == "flush" for event in target.events) == before_ends_flushes
    assert std_stream.flush(std_stream.out) is std_stream.out
    assert target.events[-1] == ("flush", None)


def test_stderr_unitbuf_flushes_after_each_insert(monkeypatch):
    target = _RecordingTextStream()
    monkeypatch.setattr(std_stream.sys, "stderr", target)

    std_stream.err << "one" << 2

    writes = [index for index, event in enumerate(target.events) if event[0] == "write"]
    assert [target.events[index][1] for index in writes] == ["one", "2"]
    assert all(target.events[index + 1][0] == "flush" for index in writes)
