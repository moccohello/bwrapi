from typing import overload

from .global_game import GameWrapper


class _ConsoleStream:
    def __lshift__(self, value: object, /) -> _ConsoleStream: ...
    def _write(self, text: str, /) -> None: ...
    def flush(self) -> None: ...


bwout: _ConsoleStream
bwerr: _ConsoleStream
out: _ConsoleStream
err: _ConsoleStream
cout: _ConsoleStream
cerr: _ConsoleStream


@overload
def endl(stream: _ConsoleStream, /) -> _ConsoleStream: ...
@overload
def endl(stream: GameWrapper, /) -> GameWrapper: ...
@overload
def ends(stream: _ConsoleStream, /) -> _ConsoleStream: ...
@overload
def ends(stream: GameWrapper, /) -> GameWrapper: ...
@overload
def flush(stream: _ConsoleStream, /) -> _ConsoleStream: ...
@overload
def flush(stream: GameWrapper, /) -> GameWrapper: ...

__all__: list[str]
