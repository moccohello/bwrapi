from __future__ import annotations

import ctypes
import json
import os
import subprocess
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Callable

from ctypes import wintypes

from . import preflight
from .model import LauncherConfigError, ProductConfig

# This process's bit width. The inject function is called from the bwrapi.dll of this width.
OWN_ARCH = "x64" if ctypes.sizeof(ctypes.c_void_p) == 8 else "x86"


PHASES = (
    "Preflight",
    "Starting",
    "Injected",
    "Running",
    "Stopping",
    "Exited",
    "Failed",
)

SERVER_EXPORT = b"bwrapi_bwapi_callwndproc"
# A game the launcher starts is retried until its window is up.
GAME_WINDOW_TIMEOUT_SECONDS = 60.0
# When this value under "General settings" of the SC:R settings file is false, the unit and
# bullet limits are BWAPI's 1700 and 100.
SETTINGS_SECTION = "General settings"
EXTENDED_LIMITS_SETTING = "Starcraft-Game Extended Unit Limits"

INVALID_HANDLE_VALUE = wintypes.HANDLE(-1).value


class RunLog:
    """Append-only per-run JSONL status log."""

    def __init__(self, path: Path | None):
        self.path = path
        self._stream = None
        if path is not None:
            try:
                path.parent.mkdir(parents=True, exist_ok=True)
                self._stream = path.open("a", encoding="utf-8", newline="\n")
            except OSError as exc:
                raise LauncherConfigError(f"cannot open the run log {path}: {exc}") from exc

    def emit(self, phase: str, *, pid: int | None = None, reason: str = "", **extra: object) -> dict[str, object]:
        if phase not in PHASES:
            raise ValueError(f"unknown launcher phase: {phase}")
        event: dict[str, object] = {
            "timestamp_utc": datetime.now(timezone.utc).isoformat(),
            "phase": phase,
            "pid": pid,
            "reason": reason,
        }
        event.update(extra)
        if self._stream is not None:
            self._stream.write(json.dumps(event, ensure_ascii=False, separators=(",", ":")) + "\n")
            self._stream.flush()
        return event

    def close(self) -> None:
        if self._stream is not None:
            self._stream.close()
            self._stream = None

    def __enter__(self) -> "RunLog":
        return self

    def __exit__(self, *_args: object) -> None:
        self.close()


def run_product(
    product: ProductConfig,
    *,
    log_path: Path | None = None,
    game: Path | None = None,
    on_status: Callable[[dict[str, object]], None] | None = None,
) -> int:
    """Puts the in-game BWAPI server into a game and runs the product's loader.

    Without ``game`` the launcher attaches to a running StarCraft. With it,
    the launcher starts that executable itself, under the legacy unit
    limits, so BWAPI unit indexes match the game's one to one.
    """

    result = preflight.inspect_product(product)
    if not result.ok:
        raise preflight.PreflightError(result)
    if os.name != "nt":
        raise LauncherConfigError("the launcher runs bots only on Windows")
    if log_path is None:
        log_path = product.directory / f"{product.product_id}.run.jsonl"

    def publish(event: dict[str, object]) -> None:
        if on_status is not None:
            on_status(event)

    with RunLog(log_path) as log:
        publish(log.emit("Preflight", reason="files checked", product_id=product.product_id))
        process: subprocess.Popen[bytes] | None = None
        pid: int | None = None
        output_stream = None
        game_process: subprocess.Popen[bytes] | None = None
        # A game the launcher starts keeps the legacy limits setting until it exits; then it is restored.
        restore_settings: Callable[[], None] | None = None
        try:
            if game is not None:
                restore_settings = _disable_extended_limits()
                game_process = subprocess.Popen([str(game), "-launch"], cwd=game.parent, shell=False)
                pid = game_process.pid
                publish(log.emit("Starting", pid=pid, reason="starting StarCraft with the legacy unit limits", game=str(game)))
            else:
                pid = product.process_id or _running_game()
                publish(log.emit("Starting", pid=pid, reason="attaching to the running StarCraft"))
            arch = _game_arch(pid)
            product.mode_file(arch).write_text(product.mode, encoding="ascii")
            _inject(product, pid, arch, GAME_WINDOW_TIMEOUT_SECONDS if game is not None else 0.0)
            publish(log.emit("Injected", pid=pid, reason="in-game BWAPI server injected", mode=product.mode,
                             arch=arch, server=str(product.server(arch)), log=str(product.log_file(arch))))
            command = product.loader_command
            if command is None:
                code = game_process.wait() if game_process is not None else 0
                publish(log.emit("Exited", pid=pid, reason="server ready; a BWAPI client connects to it directly", exit_code=code))
                return code
            output_path = log_path.with_name(log_path.name + ".loader")
            output_stream = output_path.open("wb")
            process = subprocess.Popen(
                list(command),
                cwd=product.directory,
                shell=False,
                creationflags=getattr(subprocess, "CREATE_NEW_PROCESS_GROUP", 0),
                stdout=output_stream,
                stderr=subprocess.STDOUT,
            )
            publish(log.emit("Running", pid=process.pid, reason="loader started", game_pid=pid,
                             loader_output=str(output_path)))
            code = _wait_loader(process, game_process)
            publish(log.emit("Exited", pid=process.pid, reason=f"exit_code={code}", exit_code=code))
            return code
        except KeyboardInterrupt:
            if process is not None:
                publish(log.emit("Stopping", pid=process.pid, reason="interrupted by the user"))
                _terminate_tree(process)
                publish(log.emit("Exited", pid=process.pid, reason="stopped", exit_code=130))
            return 130
        except (OSError, ValueError) as exc:
            if process is not None:
                _terminate_tree(process)
            publish(log.emit("Failed", pid=pid, reason=str(exc)))
            if isinstance(exc, LauncherConfigError):
                raise
            raise LauncherConfigError(f"cannot run: {exc}") from exc
        except BaseException:
            if process is not None:
                _terminate_tree(process)
            raise
        finally:
            if output_stream is not None:
                output_stream.close()
            if restore_settings is not None:
                if game_process is not None and game_process.poll() is None:
                    publish(log.emit("Stopping", pid=pid, reason="waiting for the game to exit to restore the extended unit limits setting"))
                    game_process.wait()
                restore_settings()


def _wait_loader(loader: subprocess.Popen[bytes], game: subprocess.Popen[bytes] | None) -> int:
    """The loader's exit code. The loader keeps looking for a server after its
    game is gone, so a game the launcher started ends the loader with it."""
    if game is None:
        return loader.wait()
    while (code := loader.poll()) is None:
        if game.poll() is not None:
            _terminate_tree(loader)
            return loader.wait()
        time.sleep(0.5)
    return code


def _game_arch(pid: int) -> str:
    """The bit width of game ``pid``. A game running under WOW64 on 64-bit Windows is x86."""
    kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel32.OpenProcess.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
    kernel32.OpenProcess.restype = wintypes.HANDLE
    kernel32.IsWow64Process.argtypes = [wintypes.HANDLE, ctypes.POINTER(wintypes.BOOL)]
    kernel32.IsWow64Process.restype = wintypes.BOOL
    kernel32.CloseHandle.argtypes = [wintypes.HANDLE]
    kernel32.CloseHandle.restype = wintypes.BOOL
    handle = kernel32.OpenProcess(0x1000, False, pid)  # PROCESS_QUERY_LIMITED_INFORMATION
    if not handle:
        raise LauncherConfigError(f"cannot open process {pid} (Windows error {ctypes.get_last_error()}): check the PID; "
                                  "python -m bwrapi.launcher doctor lists running games")
    try:
        wow64 = wintypes.BOOL()
        if not kernel32.IsWow64Process(handle, ctypes.byref(wow64)):
            raise LauncherConfigError(f"cannot read the bit width of StarCraft process {pid}: Windows error {ctypes.get_last_error()}")
        return "x86" if wow64.value else "x64"
    finally:
        kernel32.CloseHandle(handle)


def _inject(product: ProductConfig, pid: int, arch: str, timeout: float) -> None:
    """Loads the ``arch`` server into game ``pid``; a second load of the same file is a no-op.

    The call goes through this process's own-width ``bwrapi.dll``; for a game
    of the other width it runs the ``bwrapi-inject.exe`` beside that server.
    """
    library = ctypes.WinDLL(str(product.bwrapi_dll(OWN_ARCH)))
    library.bwr_inject.argtypes = [ctypes.c_uint32, ctypes.c_char_p, ctypes.c_char_p]
    library.bwr_inject.restype = ctypes.c_uint32
    library.bwr_last_error.argtypes = [ctypes.c_void_p]
    library.bwr_last_error.restype = ctypes.c_char_p
    deadline = time.monotonic() + timeout
    while True:
        if library.bwr_inject(pid, str(product.server(arch)).encode("utf-8"), SERVER_EXPORT) == 0:
            return
        error = (library.bwr_last_error(None) or b"").decode("utf-8", "replace")
        if time.monotonic() >= deadline:
            raise LauncherConfigError(f"cannot inject the server into StarCraft process {pid}: {error}")
        time.sleep(0.5)


def _disable_extended_limits() -> Callable[[], None]:
    """Turns the extended limits off and returns a function that restores the user's setting."""
    path = _documents() / "StarCraft" / "CSettings.json"
    try:
        settings = json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        settings = {}
    except (OSError, json.JSONDecodeError) as exc:
        raise LauncherConfigError(f"cannot read the StarCraft settings {path}: {exc}") from exc
    missing = object()
    section = settings.get(SETTINGS_SECTION)
    if not isinstance(section, dict):
        section = {}
    original = section.get(EXTENDED_LIMITS_SETTING, missing)
    _write_settings(path, {**settings, SETTINGS_SECTION: {**section, EXTENDED_LIMITS_SETTING: False}})

    def restore() -> None:
        try:
            restored = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            restored = dict(settings)
        current = restored.get(SETTINGS_SECTION)
        current = dict(current) if isinstance(current, dict) else {}
        if original is missing:
            current.pop(EXTENDED_LIMITS_SETTING, None)
        else:
            current[EXTENDED_LIMITS_SETTING] = original
        restored[SETTINGS_SECTION] = current
        _write_settings(path, restored)

    return restore


def _write_settings(path: Path, settings: dict[str, object]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(settings, ensure_ascii=False, indent=4), encoding="utf-8")


def _documents() -> Path:
    buffer = ctypes.create_unicode_buffer(260)
    # CSIDL_PERSONAL follows a Documents folder the user has moved.
    if ctypes.windll.shell32.SHGetFolderPathW(None, 5, None, 0, buffer) != 0:
        return Path.home() / "Documents"
    return Path(buffer.value)


def _running_game() -> int:
    """The PID of the running StarCraft when exactly one is running."""
    pids = _running_starcraft_pids()
    if not pids:
        raise LauncherConfigError("no StarCraft process is running: start the game first")
    if len(pids) != 1:
        raise LauncherConfigError(
            f"{len(pids)} StarCraft processes are running ({', '.join(map(str, pids))}): "
            "choose one with --process-id"
        )
    return pids[0]


class _ProcessEntry32W(ctypes.Structure):
    _fields_ = [
        ("dwSize", wintypes.DWORD),
        ("cntUsage", wintypes.DWORD),
        ("th32ProcessID", wintypes.DWORD),
        ("th32DefaultHeapID", ctypes.POINTER(ctypes.c_ulong)),
        ("th32ModuleID", wintypes.DWORD),
        ("cntThreads", wintypes.DWORD),
        ("th32ParentProcessID", wintypes.DWORD),
        ("pcPriClassBase", ctypes.c_long),
        ("dwFlags", wintypes.DWORD),
        ("szExeFile", wintypes.WCHAR * 260),
    ]


def running_games() -> list[dict[str, object]]:
    """The running StarCraft processes: PID, bit width (``None`` when the process
    cannot be opened) and whether a BWAPI server in it is listed in BWAPI's game
    table, and with a client connected."""
    pids = _running_starcraft_pids()
    servers = _listed_servers() if pids else {}
    games: list[dict[str, object]] = []
    for pid in pids:
        try:
            arch: str | None = _game_arch(pid)
        except LauncherConfigError:
            arch = None
        games.append({"pid": pid, "arch": arch, "server_injected": pid in servers,
                      "client_connected": servers.get(pid, False)})
    return games


def _listed_servers() -> dict[int, bool]:
    """Server PID -> client connected, from the game table every in-game BWAPI server joins."""
    from bwrapi.bwapi.client import WindowsTransport

    transport = WindowsTransport()
    try:
        table = transport.open_table()
        if table is None:
            return {}
        return {int(row.serverProcessID): bool(row.isConnected)
                for row in table.gameInstances if row.serverProcessID}
    except OSError:
        return {}
    finally:
        transport.close()


def _running_starcraft_pids() -> list[int]:
    if os.name != "nt":
        return []
    kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel32.CreateToolhelp32Snapshot.argtypes = [wintypes.DWORD, wintypes.DWORD]
    kernel32.CreateToolhelp32Snapshot.restype = wintypes.HANDLE
    kernel32.Process32FirstW.argtypes = [wintypes.HANDLE, ctypes.POINTER(_ProcessEntry32W)]
    kernel32.Process32FirstW.restype = wintypes.BOOL
    kernel32.Process32NextW.argtypes = [wintypes.HANDLE, ctypes.POINTER(_ProcessEntry32W)]
    kernel32.Process32NextW.restype = wintypes.BOOL
    kernel32.CloseHandle.argtypes = [wintypes.HANDLE]
    kernel32.CloseHandle.restype = wintypes.BOOL
    snapshot = kernel32.CreateToolhelp32Snapshot(0x2, 0)  # TH32CS_SNAPPROCESS
    if snapshot is None or snapshot == INVALID_HANDLE_VALUE:
        raise LauncherConfigError("cannot list the running processes")
    try:
        entry = _ProcessEntry32W()
        entry.dwSize = ctypes.sizeof(_ProcessEntry32W)
        pids: list[int] = []
        has_entry = kernel32.Process32FirstW(snapshot, ctypes.byref(entry))
        while has_entry:
            if entry.szExeFile.casefold() == "starcraft.exe":
                pids.append(int(entry.th32ProcessID))
            has_entry = kernel32.Process32NextW(snapshot, ctypes.byref(entry))
        return pids
    finally:
        kernel32.CloseHandle(snapshot)


def _terminate_tree(process: subprocess.Popen[object]) -> None:
    if process.poll() is not None:
        return
    subprocess.run(
        ["taskkill", "/PID", str(process.pid), "/T", "/F"],
        check=False,
        shell=False,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    try:
        process.wait(timeout=5)
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait()
