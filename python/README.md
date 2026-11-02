# BWRAPI

BWRAPI is BWAPI for StarCraft: Remastered 1.23.10: a library for reading the game state and issuing commands, for x64 and x86 games. A DLL inside the game reads the state every frame and executes commands at the frame boundary. On top of it, a BWAPI 4.4 server provides the BWAPI 4.4 API, so existing BWAPI 4.4 bots run without changes, and a public C ABI 1.0 gives lower-level access (state tables, command results, drawing, chat, reading from another process).

This is the Python distribution. Project home and full documentation: [github.com/moccohello/bwrapi](https://github.com/moccohello/bwrapi).

## What is in the package

- `bwrapi.bwapi`: the BWAPI 4.4 API in Python, with the same names and rules as BWAPI 4.4 C++.
- `bwrapi.native`: bindings for the C ABI 1.0 of `bwrapi.dll`.
- `bwrapi.launcher`: injects BWRAPI and the BWAPI server into a running game and starts BWAPI 4.4 bots (AIModule DLLs, client programs), as Chaoslauncher did.
- Native files for x64 and x86 games: `bwrapi.dll`, the BWAPI server, the injector and the x86 loaders.

Requirements: Windows 10/11 x64, StarCraft: Remastered 1.23.10, Python 3.11 or later (x64).

## Usage

Start StarCraft: Remastered as usual and open a game first.

A bot written with `bwrapi.bwapi`:

```python
# bot.py
from bwrapi.bwapi import AIModule, Broodwar

class Bot(AIModule):
    def onFrame(self):
        Broodwar.drawTextScreen(10, 10, "frame %d", Broodwar.getFrameCount())
```

```powershell
pip install bwrapi
python -m bwrapi.bwapi.run bot.py --inject         # injects the server and connects
python -m bwrapi.launcher run C:\Bots\MyBot.dll    # an existing BWAPI 4.4 AIModule DLL
python -m bwrapi.launcher doctor C:\Bots\MyBot.dll # checks files and running games
```

Reading state through the C ABI from Python, without injecting anything (`doctor` lists the PIDs of running games):

```python
import ctypes
from bwrapi import native

lib = native.load()
abi = native.abi
pid = 1234  # the game's process id
session = ctypes.c_void_p()
if lib.bwr_open(pid, 0, ctypes.byref(session)) == abi.STATUS_OK:
    lib.bwr_subscribe(session, abi.SOURCE_UNITS)
    frame = ctypes.POINTER(abi.Frame)()
    if lib.bwr_acquire(session, ctypes.byref(frame)) == abi.STATUS_OK:
        if frame.contents.units:  # NULL when the table was not read
            print(frame.contents.units.contents.rows, "units")
        lib.bwr_release(session, frame)
    lib.bwr_close(session)
```

The wheel is `py3-none-win_amd64` because it carries Windows native files. Connecting to a game works only on Windows.

## Documentation

- [README](https://github.com/moccohello/bwrapi#readme): overview, quick start for every kind of user
- [Python SDK guide](https://github.com/moccohello/bwrapi/blob/main/docs/SDK.md): the Python BWAPI API and its differences from C++
- [C ABI reference](https://github.com/moccohello/bwrapi/blob/main/docs/C-ABI.md): functions, sources, columns and commands
- [Launcher guide](https://github.com/moccohello/bwrapi/blob/main/python/bwrapi/launcher/README.md)
- [Compatibility with BWAPI 4.4](https://github.com/moccohello/bwrapi/blob/main/docs/BWAPI-COMPATIBILITY.md)
- [BWAPI documentation](https://bwapi.github.io/): names and semantics of the BWAPI 4.4 API
