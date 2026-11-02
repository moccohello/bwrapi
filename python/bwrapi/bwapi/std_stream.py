"""Python console aliases for BWAPI's active standard stream bindings.

Insertion uses bool 0/1, float ``.6g``, and Python ``str()`` for other values.
C++ formatting flags and arbitrary ostream overloads are outside this surface.
"""
from __future__ import annotations

import sys

from .global_game import endl, ends, flush


def _format_value(value: object) -> str:
    if isinstance(value, bool):
        return "1" if value else "0"
    if isinstance(value, float):
        return format(value, ".6g")
    # BWAPI Type values provide __str__ through getName(), as in Type.h.
    return str(value)


class _ConsoleStream:
    """Small ``<<`` proxy that resolves the current Python console on each write."""

    __slots__ = ("_channel", "_unitbuf")

    def __init__(self, channel: str, *, unitbuf: bool = False) -> None:
        self._channel = channel
        self._unitbuf = unitbuf

    def _write(self, text: str) -> None:
        target = getattr(sys, self._channel)
        target.write(text)
        if self._unitbuf:
            target.flush()

    def flush(self) -> None:
        getattr(sys, self._channel).flush()

    def __lshift__(self, value: object) -> _ConsoleStream:
        if value is endl:
            return endl(self)
        if value is ends:
            return ends(self)
        if value is flush:
            return flush(self)
        self._write(_format_value(value))
        return self


cout = out = bwout = _ConsoleStream("stdout")
cerr = err = bwerr = _ConsoleStream("stderr", unitbuf=True)

__all__ = ["bwout", "bwerr", "out", "err", "cout", "cerr", "endl", "ends", "flush"]
