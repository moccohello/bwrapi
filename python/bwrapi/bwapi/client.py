"""BWAPI 4.4 client: attaches to the shared memory and pipe of the in-game BWAPI server.

It follows the same steps as the official `BWAPIClient`:
1. Pick a server row that has no client yet from the GameTable
   (`Local\\bwapi_shared_memory_game_list`).
2. Open that server's pipe and GameData shared memory and check the client
   version (10003).
3. Read until the server sends the ready code 2.
`update()` writes the update code 1, waits for the next 2 and updates `Game`
from the events in GameData. GameData points straight at the shared memory, so
values are read without copies. Sync or async serving is a server setting; the
client protocol is the same.
"""

from __future__ import annotations

import asyncio
import ctypes
import sys
import time
from ctypes import sizeof
from typing import Callable, Protocol

from . import gamedata
from .gamedata import GameData, GameTable
from .profile import BWAPI_CLIENT_VERSION

_UPDATE = 1
_READY = 2


class ServerConnectionError(ConnectionError):
    """No BWAPI server was found, it is incompatible, or the connection to it broke."""


class Pipe(Protocol):
    def write_code(self, code: int) -> None: ...
    def read_code(self) -> int: ...
    def close(self) -> None: ...


class Transport(Protocol):
    """The operating-system boundary that opens the GameTable, GameData and pipe. Tests use a fake."""

    def open_table(self) -> GameTable | None: ...
    def open_server(self, server_pid: int) -> tuple[GameData, Pipe]: ...
    def process_running(self, pid: int) -> bool: ...
    def close(self) -> None: ...


class Client:
    """A BWAPI client with the official BWAPIClient connect/update protocol; one server at a time.

    After `connect()`, `game` is the connected `Game` (also `Broodwar`) and each
    `update()` hands the frame back to the server and receives the next one.
    """

    def __init__(self, transport: Transport | None = None) -> None:
        self._transport = transport
        self._pipe: Pipe | None = None
        self.data: GameData | None = None
        self.game = None
        self.server_pid: int | None = None

    def isConnected(self) -> bool:
        return self._pipe is not None

    def connect(self, server_pid: int | None = None) -> bool:
        """Attaches to one server; True on success. Without `server_pid` it picks one
        from the GameTable, as the official client does."""
        if self._pipe is not None:
            return True
        transport = self._transport or _default_transport()
        self._transport = transport
        try:
            if server_pid is None:
                server_pid = select_server(transport.open_table())
                if server_pid is None:
                    transport.close()
                    return False
            data, pipe = transport.open_server(server_pid)
        except OSError:
            transport.close()
            return False
        if data.client_version != BWAPI_CLIENT_VERSION:
            pipe.close()
            transport.close()
            raise ServerConnectionError(
                f"BWAPI client version {BWAPI_CLIENT_VERSION} is not compatible with server version "
                f"{data.client_version}")
        from .game import Game
        from .global_game import _set_current_game

        self.data, self._pipe, self.server_pid = data, pipe, server_pid
        self.game = Game(data, self)
        _set_current_game(self.game, owner=self)
        try:
            self._wait_ready()
        except ServerConnectionError:
            self.disconnect()
            return False
        return True

    def disconnect(self) -> None:
        from .global_game import _reset_current_game

        if self._pipe is not None:
            self._pipe.close()
        if self._transport is not None:
            self._transport.close()
        if self.game is not None:
            self.game._retire()
        _reset_current_game(self)
        self._pipe = None
        self.data = None
        self.game = None
        self.server_pid = None

    def update(self) -> None:
        """Hands this frame's commands and drawing to the server and receives the next frame."""
        self._exchange()
        self._apply()

    async def update_async(self) -> None:
        """Like `update()`, but leaves the event loop free while the server runs the frame.

        Only the pipe wait runs on another thread; `Game` is updated on the calling
        loop's thread.
        """
        await asyncio.to_thread(self._exchange)
        self._apply()

    def _exchange(self) -> None:
        if self._pipe is None:
            raise ServerConnectionError("not connected to a BWAPI server")
        try:
            try:
                self._pipe.write_code(_UPDATE)
            except OSError as error:
                raise ServerConnectionError(f"the BWAPI server pipe closed: {error}") from error
            self._wait_ready()
        except ServerConnectionError:
            # Like the official client, drop the state of a lost server. A new connect makes a new Game.
            self.disconnect()
            raise

    def _wait_ready(self) -> None:
        pipe = self._pipe
        try:
            while pipe.read_code() != _READY:
                pass
        except OSError as error:
            raise ServerConnectionError(f"the BWAPI server pipe closed: {error}") from error

    def _apply(self) -> None:
        self.game._on_update()


def select_server(table: GameTable | None) -> int | None:
    """Picks, as the official client does, the row without a client whose keep-alive is oldest."""
    if table is None:
        return None
    best = None
    for row in table.gameInstances:
        if row.serverProcessID == 0 or row.isConnected:
            continue
        if best is None or row.lastKeepAliveTime < best.lastKeepAliveTime:
            best = row
    return None if best is None else int(best.serverProcessID)


def connect(server_pid: int | None = None, *, wait: float = 0.0,
            transport: Transport | None = None,
            on_wait: Callable[[str], None] | None = None) -> Client:
    """Returns a `Client` connected to the in-game BWAPI server.

    `server_pid` selects the game; without it a free server is picked from the
    GameTable. When no server answers, it retries every second for `wait`
    seconds (`math.inf` waits forever) and then raises `ServerConnectionError`.
    `on_wait` is called once with what it is waiting for, before the first retry.
    """
    client = Client(transport)
    deadline = time.monotonic() + wait
    waiting = False
    while not client.connect(server_pid):
        if time.monotonic() >= deadline:
            raise ServerConnectionError(_no_server_message(client._transport, server_pid))
        if not waiting and on_wait is not None:
            on_wait(_waiting_message(client._transport, server_pid))
        waiting = True
        time.sleep(1.0)
    return client


def _servers_with_client(transport: Transport, server_pid: int | None) -> str:
    """The processes of the listed servers that already have a client; a server takes one at a time."""
    try:
        table = transport.open_table()
        if table is None:
            return ""
        return ", ".join(str(row.serverProcessID) for row in table.gameInstances
                         if row.serverProcessID and row.isConnected and server_pid in (None, row.serverProcessID)
                         and transport.process_running(row.serverProcessID))
    except OSError:
        return ""
    finally:
        transport.close()


def _no_server_message(transport: Transport, server_pid: int | None) -> str:
    if busy := _servers_with_client(transport, server_pid):
        return (f"the BWAPI server in process {busy} already has a client: stop that bot")
    if server_pid is not None:
        return (f"no BWAPI server found in process {server_pid}: check the PID "
                "(python -m bwrapi.launcher doctor lists running games) and inject the server "
                "(python -m bwrapi.bwapi.run <bot.py> --inject)")
    return ("no BWAPI server found: start StarCraft, and inject the server if it is not in the game yet "
            "(python -m bwrapi.bwapi.run <bot.py> --inject)")


def _waiting_message(transport: Transport, server_pid: int | None) -> str:
    if busy := _servers_with_client(transport, server_pid):
        return (f"waiting for a free BWAPI server: process {busy} already has a client "
                "(stop that bot)")
    where = "a running game" if server_pid is None else f"process {server_pid}"
    return f"waiting for the BWAPI server in {where}"


def _default_transport() -> Transport:
    if sys.platform != "win32":
        raise ServerConnectionError("connecting to a BWAPI server works only on Windows")
    return WindowsTransport()


class WindowsTransport:
    """Opens the server through Win32 file mappings and named pipes."""

    def __init__(self) -> None:
        from ctypes import wintypes

        kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
        self._k = kernel32
        kernel32.OpenFileMappingW.restype = wintypes.HANDLE
        kernel32.OpenFileMappingW.argtypes = (wintypes.DWORD, wintypes.BOOL, wintypes.LPCWSTR)
        kernel32.MapViewOfFile.restype = ctypes.c_void_p
        kernel32.MapViewOfFile.argtypes = (wintypes.HANDLE, wintypes.DWORD, wintypes.DWORD,
                                           wintypes.DWORD, ctypes.c_size_t)
        kernel32.UnmapViewOfFile.argtypes = (ctypes.c_void_p,)
        kernel32.CloseHandle.argtypes = (wintypes.HANDLE,)
        kernel32.OpenProcess.restype = wintypes.HANDLE
        kernel32.OpenProcess.argtypes = (wintypes.DWORD, wintypes.BOOL, wintypes.DWORD)
        kernel32.GetExitCodeProcess.argtypes = (wintypes.HANDLE, ctypes.POINTER(wintypes.DWORD))
        kernel32.CreateFileW.restype = wintypes.HANDLE
        kernel32.CreateFileW.argtypes = (wintypes.LPCWSTR, wintypes.DWORD, wintypes.DWORD, ctypes.c_void_p,
                                         wintypes.DWORD, wintypes.DWORD, wintypes.HANDLE)
        kernel32.ReadFile.argtypes = (wintypes.HANDLE, ctypes.c_void_p, wintypes.DWORD,
                                      ctypes.POINTER(wintypes.DWORD), ctypes.c_void_p)
        kernel32.WriteFile.argtypes = (wintypes.HANDLE, ctypes.c_void_p, wintypes.DWORD,
                                       ctypes.POINTER(wintypes.DWORD), ctypes.c_void_p)
        self._table: _MappedView | None = None
        self._dword = wintypes.DWORD

    def _map(self, name: str, size: int) -> _MappedView | None:
        file_map_all = 0x0002 | 0x0004  # FILE_MAP_WRITE | FILE_MAP_READ
        handle = self._k.OpenFileMappingW(file_map_all, False, name)
        if not handle:
            return None
        address = self._k.MapViewOfFile(handle, file_map_all, 0, 0, size)
        if not address:
            self._k.CloseHandle(handle)
            raise ctypes.WinError(ctypes.get_last_error())
        return _MappedView(self._k, handle, address)

    def open_table(self) -> GameTable | None:
        self._table = self._map(gamedata.GAME_TABLE_NAME, sizeof(GameTable))
        return None if self._table is None else GameTable.from_address(self._table.address)

    def process_running(self, pid: int) -> bool:
        still_active = 259
        handle = self._k.OpenProcess(0x1000, False, pid)  # PROCESS_QUERY_LIMITED_INFORMATION
        if not handle:
            return False
        try:
            code = wintypes.DWORD()
            return bool(self._k.GetExitCodeProcess(handle, ctypes.byref(code))) and code.value == still_active
        finally:
            self._k.CloseHandle(handle)

    def open_server(self, server_pid: int) -> tuple[GameData, _WindowsPipe]:
        generic_rw = 0x80000000 | 0x40000000
        open_existing = 3
        pipe = self._k.CreateFileW(gamedata.pipe_name(server_pid), generic_rw, 0, None, open_existing, 0, None)
        if pipe in (None, ctypes.c_void_p(-1).value):
            raise ctypes.WinError(ctypes.get_last_error())
        view = self._map(gamedata.game_data_name(server_pid), sizeof(GameData))
        if view is None:
            self._k.CloseHandle(pipe)
            raise ctypes.WinError(ctypes.get_last_error())
        data = GameData.from_address(view.address)
        # Unit, Player and Bullet handles keep views into this GameData. The
        # mapping goes when the last of them does, so a handle kept past the
        # connection reads stale values instead of unmapped memory.
        data._view = view
        return data, _WindowsPipe(self._k, pipe, self._dword)

    def close(self) -> None:
        self._table = None


class _MappedView:
    def __init__(self, kernel32, handle, address) -> None:
        self._k, self._handle, self.address = kernel32, handle, address

    def __del__(self) -> None:
        self._k.UnmapViewOfFile(self.address)
        self._k.CloseHandle(self._handle)


class _WindowsPipe:
    def __init__(self, kernel32, handle, dword) -> None:
        self._k, self._handle, self._dword = kernel32, handle, dword
        self._buffer = ctypes.c_int32()

    def write_code(self, code: int) -> None:
        self._buffer.value = code
        done = self._dword()
        if not self._k.WriteFile(self._handle, ctypes.byref(self._buffer), 4, ctypes.byref(done), None):
            raise ctypes.WinError(ctypes.get_last_error())

    def read_code(self) -> int:
        done = self._dword()
        if not self._k.ReadFile(self._handle, ctypes.byref(self._buffer), 4, ctypes.byref(done), None):
            raise ctypes.WinError(ctypes.get_last_error())
        return self._buffer.value

    def close(self) -> None:
        if self._handle:
            self._k.CloseHandle(self._handle)
            self._handle = None


# The C++ client's global `BWAPI::BWAPIClient`.
BWAPIClient = Client()
