"""The Win32 transport against a server made in this process: a GameData file mapping and the BWAPI pipe."""

from __future__ import annotations

import ctypes
import os
import sys
import threading

import pytest

from bwrapi.bwapi import gamedata
from bwrapi.bwapi.client import Client
from bwrapi.bwapi.type_ids import EventTypeId

pytestmark = pytest.mark.skipif(sys.platform != "win32", reason="needs Win32 file mappings and named pipes")


def _kernel32():
    from ctypes import wintypes

    k = ctypes.WinDLL("kernel32", use_last_error=True)
    k.CreateFileMappingW.restype = wintypes.HANDLE
    k.CreateFileMappingW.argtypes = (wintypes.HANDLE, ctypes.c_void_p, wintypes.DWORD, wintypes.DWORD,
                                     wintypes.DWORD, wintypes.LPCWSTR)
    k.MapViewOfFile.restype = ctypes.c_void_p
    k.MapViewOfFile.argtypes = (wintypes.HANDLE, wintypes.DWORD, wintypes.DWORD, wintypes.DWORD, ctypes.c_size_t)
    k.UnmapViewOfFile.argtypes = (ctypes.c_void_p,)
    k.CreateNamedPipeW.restype = wintypes.HANDLE
    k.CreateNamedPipeW.argtypes = (wintypes.LPCWSTR, wintypes.DWORD, wintypes.DWORD, wintypes.DWORD,
                                   wintypes.DWORD, wintypes.DWORD, wintypes.DWORD, ctypes.c_void_p)
    k.ConnectNamedPipe.argtypes = (wintypes.HANDLE, ctypes.c_void_p)
    k.ReadFile.argtypes = (wintypes.HANDLE, ctypes.c_void_p, wintypes.DWORD, ctypes.POINTER(wintypes.DWORD),
                           ctypes.c_void_p)
    k.WriteFile.argtypes = (wintypes.HANDLE, ctypes.c_void_p, wintypes.DWORD, ctypes.POINTER(wintypes.DWORD),
                            ctypes.c_void_p)
    k.CloseHandle.argtypes = (wintypes.HANDLE,)
    k.DisconnectNamedPipe.argtypes = (wintypes.HANDLE,)
    return k, wintypes


def test_client_connects_and_updates_over_the_win32_pipe():
    k, wintypes = _kernel32()
    pid = os.getpid()
    size = ctypes.sizeof(gamedata.GameData)
    mapping = k.CreateFileMappingW(wintypes.HANDLE(-1), None, 0x04, 0, size, gamedata.game_data_name(pid))
    assert mapping, ctypes.get_last_error()
    view = k.MapViewOfFile(mapping, 0x0002 | 0x0004, 0, 0, size)
    data = gamedata.GameData.from_address(view)
    data.client_version = 10003
    data.frameCount = 17
    # Message-mode blocking pipe, as the server's accept leaves it.
    pipe = k.CreateNamedPipeW(gamedata.pipe_name(pid), 0x00000003, 0x00000004 | 0x00000002, 1, 4096, 4096, 0, None)
    assert pipe != wintypes.HANDLE(-1).value, ctypes.get_last_error()
    received: list[int] = []

    def server():
        code = ctypes.c_int32()
        done = wintypes.DWORD()
        k.ConnectNamedPipe(pipe, None)
        code.value = 2
        k.WriteFile(pipe, ctypes.byref(code), 4, ctypes.byref(done), None)
        k.ReadFile(pipe, ctypes.byref(code), 4, ctypes.byref(done), None)
        received.append(code.value)
        data.eventCount = 1
        data.events[0].type = EventTypeId.MENU_FRAME
        data.frameCount = 18
        code.value = 2
        k.WriteFile(pipe, ctypes.byref(code), 4, ctypes.byref(done), None)
        k.ReadFile(pipe, ctypes.byref(code), 4, ctypes.byref(done), None)  # returns when the client closes
        k.DisconnectNamedPipe(pipe)

    thread = threading.Thread(target=server)
    thread.start()
    client = Client()
    try:
        assert client.connect(pid)
        assert client.game.getFrameCount() == 17
        client.update()
        assert received == [1]
        assert client.game.getFrameCount() == 18
        assert [event.getType() for event in client.game.getEvents()] == [EventTypeId.MENU_FRAME]
    finally:
        client.disconnect()
        thread.join(5)
        k.CloseHandle(pipe)
        k.UnmapViewOfFile(view)
        k.CloseHandle(mapping)
