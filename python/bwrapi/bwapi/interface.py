from __future__ import annotations

from operator import index
from typing import Callable, Generic, TypeVar


_T = TypeVar("_T")
_DEFAULT_ACTION = object()


class InterfaceEvent(Generic[_T]):
    """InterfaceEvent.h pass-count scheduler, independent of game frames."""

    __slots__ = ("_condition", "_action", "_run_freq", "_run_count", "_step")

    def __init__(self, action=_DEFAULT_ACTION, condition=None, timesToRun=-1, framesToCheck=0):
        self._condition = condition
        self._action = None if action is _DEFAULT_ACTION else action
        self._run_freq = index(framesToCheck)
        self._run_count = 0 if action is _DEFAULT_ACTION else index(timesToRun)
        self._step = self._run_freq

    def isFinished(self) -> bool:
        return self._run_count == 0

    def removeEvent(self) -> None:
        self._run_count = 0

    def _execute(self, instance: _T) -> bool:
        if self.isFinished():
            return False
        self._step -= 1
        if self._step > 0:
            return False
        self._step = self._run_freq
        if self._condition is not None and not self._condition(instance):
            return False
        if self._action is None:
            return False
        self._action(instance)
        if self._run_count > 0:
            self._run_count -= 1
        return True


class Interface:
    """Per-handle Python storage for the official BWAPI client-info API."""

    __slots__ = ("_client_info", "_interface_events")
    _client_info: dict[int, object] | None

    def getClientInfo(self, key: int = 0) -> object | None:
        """Return bot-owned metadata stored for this handle and key."""
        key = index(key)
        values = getattr(self, "_client_info", None)
        return None if values is None else values.get(key)

    def setClientInfo(self, value: object, key: int = 0) -> None:
        """Store bot-owned metadata on this handle without touching game state."""
        key = index(key)
        values = getattr(self, "_client_info", None)
        if values is None:
            values = {}
            object.__setattr__(self, "_client_info", values)
        values[key] = value

    def registerEvent(
        self,
        action: Callable[[Interface], None] | None,
        condition: Callable[[Interface], bool] | None = None,
        timesToRun: int = -1,
        framesToCheck: int = 0,
    ) -> None:
        """Attach a conditional action; official registerEvent returns void."""
        event = InterfaceEvent(action, condition, timesToRun, framesToCheck)
        events = getattr(self, "_interface_events", None)
        if events is None:
            events = []
            object.__setattr__(self, "_interface_events", events)
        events.append(event)

    def _update_interface_events(self) -> None:
        events = getattr(self, "_interface_events", None)
        if events is None:
            return
        # Match std::list iteration: appended entries can run in this pass;
        # entries finished by their action are erased on the next pass.
        position = 0
        while position < len(events):
            event = events[position]
            if event.isFinished():
                del events[position]
            else:
                event._execute(self)
                position += 1

    def _clear_interface_events(self) -> None:
        events = getattr(self, "_interface_events", None)
        if events is not None:
            events.clear()
