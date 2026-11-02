from __future__ import annotations

from typing import TYPE_CHECKING, Protocol, TypeVar


if TYPE_CHECKING:
    from .game import Game


class _ConsoleStream(Protocol):
    def _write(self, text: str, /) -> None: ...
    def flush(self) -> None: ...


_current_game: Game | None = None
_current_owner: object | None = None


def _get_current_game() -> Game | None:
    """Read the process-global pointer, excluding a retired Game lifetime."""
    global _current_game, _current_owner
    if _current_game is not None and getattr(_current_game, "_retired", False):
        _current_game = None
        _current_owner = None
    return _current_game


def _set_current_game(game: Game | None, *, owner: object | None = None) -> None:
    global _current_game, _current_owner
    _current_game = game
    _current_owner = owner if game is not None else None


def _reset_current_game(owner: object) -> None:
    """One runner cannot remove the pointer last bound by another runner."""
    if _current_owner is owner:
        _set_current_game(None)


def _get_current_binding() -> tuple[Game | None, object | None]:
    return _get_current_game(), _current_owner


def __getattr__(name: str):
    if name == "BroodwarPtr":
        return _get_current_game()
    raise AttributeError(name)


class GameWrapper:
    """`Broodwar`: the connected `Game` (see `help(bwrapi.bwapi.Game)`) and the Game.cpp output stream.

    Every `Game` method is forwarded to the game of the current connection. Outside
    a connection, game methods raise RuntimeError; `Broodwar << "text" << endl`
    prints to the game like C++.
    """

    __slots__ = ("_stream",)

    def __init__(self) -> None:
        self._stream = ""

    def __getattr__(self, name: str):
        game = _get_current_game()
        if (name.startswith("__") and name.endswith("__")) or (game is None and name.startswith("_")):
            # Introspection (help, dir, copy, IPython) probes these names; they are not Game methods.
            raise AttributeError(name)
        if game is None:
            raise RuntimeError(f"Broodwar.{name} requires a running match: connect to a game first "
                               "(bwrapi.bwapi.connect(), or run the bot with python -m bwrapi.bwapi.run)")
        return getattr(game, name)

    def __dir__(self) -> list[str]:
        from .game import Game

        return sorted(set(object.__dir__(self)) | {name for name in dir(Game) if not name.startswith("_")})

    def __lshift__(self, value: object) -> GameWrapper:
        if value is endl or value is ends or value is flush:
            return value(self)
        if isinstance(value, bool):
            text = "1" if value else "0"
        elif isinstance(value, float):
            text = format(value, ".6g")
        else:
            text = str(value)
        self._stream += text
        return self

    def flush(self) -> None:
        game = _get_current_game()
        if game is None or not self._stream:
            return
        # printf("%s", ss.str().c_str()) owns a string but stops at its first NUL.
        game.printf("%s", self._stream.split("\0", 1)[0])
        self._stream = ""


_Stream = TypeVar("_Stream", bound=GameWrapper | _ConsoleStream)


def endl(stream: _Stream) -> _Stream:
    """Apply std::endl to Broodwar or a console stream proxy."""
    if isinstance(stream, GameWrapper):
        stream._stream += "\n"
    else:
        stream._write("\n")
    stream.flush()
    return stream


def ends(stream: _Stream) -> _Stream:
    """Apply std::ends while keeping each stream's own flush behavior."""
    if isinstance(stream, GameWrapper):
        stream._stream += "\0"
        stream.flush()
    else:
        stream._write("\0")
    return stream


def flush(stream: _Stream) -> _Stream:
    """Flush console output; std::flush leaves Broodwar's memory buffer intact."""
    if not isinstance(stream, GameWrapper):
        stream.flush()
    return stream


Broodwar = GameWrapper()

__all__ = ["GameWrapper", "Broodwar", "BroodwarPtr", "endl", "ends", "flush"]
