# BWRAPI Python SDK

How to run BWAPI 4.4 bots in Python on StarCraft: Remastered (x64 and x86 games), and how to read information outside BWAPI through the public C ABI 1.0. The C ABI functions and columns are described in the [C ABI reference](C-ABI.md); the .NET binding is in [`dotnet/BwrApi.Client`](../dotnet/BwrApi.Client/README.md).

- BWAPI rules live in one place: the in-game BWAPI server (`bwrapi_bwapi.dll`). Every frame the server fills the official BWAPI 4.4 GameData, and `bwrapi.bwapi` reads that GameData and writes commands with the same protocol as the official client.
- `bwrapi.native` carries the ABI 1.0 declarations of `bwrapi.dll` as they are. The binding has no logic.
- Connecting to a server works on Windows only. The pure Python parts of the package (type tables, positions, command values) import on other operating systems too.

## 1. Install and run

```powershell
pip install bwrapi
# Start StarCraft as usual (Battle.net, local or LAN).
python -m bwrapi.bwapi.run mybot.py --inject
```

- `--inject` uses the launcher to put `bwrapi.dll` and the BWAPI server into the running game. It checks whether the game is x64 or x86 and injects the files of the same bit width. A game that already has them is not injected again; the same server is used. `--process-id` picks the game; without it, the running game is used when there is exactly one.
- `--mode sync` (default) makes the game wait for the bot every frame (as in BWAPI). With `--mode async` the game does not wait: after the bot responds, its commands run on the first following frame and that frame is handed to the bot. `--mode` takes effect with `--inject`; the server reads the mode when a client connects.
- If the server was injected separately by the launcher (`python -m bwrapi.launcher run <client product>`, entrypoint `client`), run without `--inject`. If no server is free, the command prints one waiting line and looks again every second for `--wait` seconds (default: forever). A server takes one client at a time, so a second bot waits until the first one disconnects, or runs on another game. `python -m bwrapi.launcher doctor` checks the package's files and lists the running games: PID, bit width and whether a server is in them.
- The bot file defines a class derived from `AIModule` (`Bot`, or the only such subclass in the file; `--class` names another). Example: [`python/examples/bot.py`](../python/examples/bot.py).
- `--asyncio` runs the bot in an asyncio event loop (section 4).

### Logs and checking that it works

- The server appends to `bwrapi_bwapi.log` next to the server DLL it was injected from. For a pip install that is `<site-packages>\bwrapi\bin\x64\` or `<site-packages>\bwrapi\bin\x86\`, matching the game's bit width. When a bot connects, the log shows `[legacy44] client connected mode=sync` (or `mode=async`). Finished commands are logged as `[command] status=OK id=<n> frame=<n>` (or another result such as `REJECTED`): the first 32 one by one, then every 256th. Commands the server could not execute are logged as `[command] status=not_executed` with a reason.
- With `--inject`, the launcher writes its run log to `client.run.jsonl` in the current directory. Phase `Injected` there means only that the server is in the game; whether the bot connected is shown by the server log line above, and whether its commands work is seen in the game.

## 2. BWAPI client

```python
from bwrapi.bwapi import AIModule, Broodwar, BwapiModuleRunner, connect

class Bot(AIModule):
    def onFrame(self):
        Broodwar.drawTextScreen(10, 10, "frame %d", Broodwar.getFrameCount())

client = connect(wait=60)            # attaches to one server
BwapiModuleRunner(client, Bot()).run()
```

The same order as the C++ client also works:

```python
import time
from bwrapi.bwapi import BWAPIClient, Broodwar

while not BWAPIClient.connect():
    time.sleep(1)
while BWAPIClient.isConnected():
    BWAPIClient.update()
    for event in Broodwar.getEvents():
        ...
```

- `connect(server_pid=None, *, wait=0.0, transport=None, on_wait=None)` returns a connected `Client`. Like the official client, it picks from the GameTable (`Local\bwapi_shared_memory_game_list`) the server with the oldest keep-alive among rows without a client, then opens its pipe (`\\.\pipe\bwapi_pipe_<pid>`) and GameData (`Local\bwapi_shared_memory_<pid>`). If the client version (10003) differs, it raises `ServerConnectionError`. With `server_pid`, it attaches to that server directly.
- `wait` is how many seconds `connect` keeps looking when no server answers, trying once a second; then it raises `ServerConnectionError`. The default `0.0` tries once and `math.inf` waits forever. `on_wait` is called once with a message saying what it is waiting for, before the first retry. A server takes one client at a time: a row that has a client is not picked, and the error and the message name that process. `transport` replaces the Win32 transport (the tests use it). `Client.connect(server_pid=None)`, also on `BWAPIClient`, makes one attempt and returns `True` or `False`.
- `update()` hands this frame's commands, text and shapes to the server (code 1), waits until the next frame is ready (code 2), then updates the match state from the GameData events. If the pipe breaks, it cleans up the connection and raises `ServerConnectionError`.
- GameData points straight at the shared memory. Values are read without copying and change to what the server writes at the next `update()`. Copy values you need in another frame into Python values.
- Right after connecting, `Game` is in the menu state. The match collections (units, players, regions, start locations) exist from the `update()` in which the server sends the `MatchStart` event. A client that connects during a match also gets `MatchStart` on its first frame.

## 3. Values and events

- Every value comes from the GameData the server filled. As with the official client, Python does not correct or estimate values; a value that is not available is the BWAPI default the server put there.
- `Game.getAllUnits()`, `Player.getUnits()`, `getMinerals()`, `getGeysers()` and `getNeutralUnits()` are updated from events (`UnitDiscover`, `UnitEvade`, `UnitRenegade`, `UnitMorph`). `getStatic*` return the values at match start. Unit, player, bullet and force handles stay the same object for the whole connection, so compare them with `is`.
- `getUnitsInRectangle`, `getUnitsInRadius`, `getClosestUnit` and `getBestUnit` search the unit finder tables the server fills, the same way as the official client.
- `can*` checks (`canIssueCommand`, `canBuildHere`, `canMake`, ...), `getBuildLocation`, `getDamageFrom/To`, `hasPath` and `getRegionAt` are computed in Python from GameData, as in the official client.
- `Player.supplyTotal()` and `supplyUsed()` are in half units, as in BWAPI: twice the supply the game shows, because a Zergling uses 0.5. `UnitType.supplyRequired()` and `supplyProvided()` are in the same units.
- `Unit.getHitPoints()`, `getShields()` and `getEnergy()` are the values the game shows; the server converts them. Only the C ABI columns of section 6 are raw (1/256 units).
- `Game.getEvents()` returns the events of the frame in the order the server sent them. `BwapiModuleRunner` and `dispatch(module, event)` call the AIModule callback of the event. `MenuFrame` has no callback. Every callback does nothing unless overridden:

```python
class AIModule:
    def onStart(self): ...
    def onEnd(self, isWinner): ...             # bool
    def onFrame(self): ...
    def onSendText(self, text): ...            # str
    def onReceiveText(self, player, text): ... # Player, str
    def onPlayerLeft(self, player): ...        # Player
    def onNukeDetect(self, target): ...        # Position
    def onSaveGame(self, gameName): ...        # str
    # Each takes one Unit:
    def onUnitDiscover(self, unit): ...
    def onUnitEvade(self, unit): ...
    def onUnitShow(self, unit): ...
    def onUnitHide(self, unit): ...
    def onUnitCreate(self, unit): ...
    def onUnitDestroy(self, unit): ...
    def onUnitMorph(self, unit): ...
    def onUnitRenegade(self, unit): ...
    def onUnitComplete(self, unit): ...
```

- `getLastEventTime()` is 0, as in the official client.

## 4. Sync and async

- Sync or async is a server mode (`--mode` in section 1). The client protocol is the same; a sync server holds the game until the bot calls `update()`.
- `Client.update_async()`, `BwapiModuleRunner.step_async()` and `run_async()` do not block the asyncio event loop while waiting for the server. Only the pipe wait runs on another thread; `Game` updates and callbacks run on the loop thread.
- Use one `Client` from one thread at a time.

## 5. Commands and drawing

- `Unit.attack`, `move`, `train` and the others, as in the official client, write a unit command into GameData and return `True` when `canIssueCommand` passes. The reason for a failure is `Broodwar.getLastError()`. `True` means the command was handed to the server; check its effect in the values of later frames.
- Latency compensation: for a command issued while latency frames remain, the expected result (order, resources, training queue, ...) is applied to this frame's GameData right away, as in the official client. The server overwrites it with the real values on the next frame. `setLatCom(False)` turns it off.
- `train` or `morph` on a hatchery is taken by one larva that can accept the command.
- The server groups and deduplicates commands (the command optimizer). `setCommandOptimizationLevel` sends the level to the server.
- `printf`, `sendText`, `sendTextEx`, `pauseGame`, `leaveGame`, `setLocalSpeed`, `setFrameSkip`, `setMap`, `setAlliance`, `setVision`, `enableFlag`, `setScreenPosition`, `pingMinimap`, `setRevealAll`, `setGUI` and `setLatCom` go to the server as GameData commands. Format strings use Python `%` formatting.
- `setLocalSpeed(speed)` sets the time per frame in milliseconds, for every game speed: `0` runs frames with no delay and a negative value restores the game's own delays (Fastest is 42). For the highest speed, combine it with `setFrameSkip`.
- `draw*` shapes and text are written as GameData shapes while `isGUIEnabled()`, and the server draws them into the game screen with `bwr_draw`.
- Text colors: the `Text.*` values are BWAPI's control codes, and a string carries one as a character. Pass it to `%c`, as in `Broodwar.drawTextScreen(8, 8, "%chello", Text.Yellow)` or `Broodwar.printf("%cready", Text.Green)`, or build the string first with `"%c" % Text.Yellow` or `chr(Text.Yellow)`. A format string with no arguments is not formatted, so `"%chello"` alone stays as it is. `setTextSize` takes `Text.Size.*`.

## 6. ABI 1.0 binding

```python
import ctypes
from bwrapi import native

lib = native.load()                         # the package's bin/x64 or bin/x86 bwrapi.dll (same bit width as Python)
session = ctypes.c_void_p()
if lib.bwr_open(pid, 0, ctypes.byref(session)) != native.abi.STATUS_OK:   # flags is 0
    raise RuntimeError("cannot open the game")
lib.bwr_subscribe(session, native.abi.SOURCE_UNITS)
frame = ctypes.POINTER(native.abi.Frame)()
if lib.bwr_acquire(session, ctypes.byref(frame)) == native.abi.STATUS_OK:
    table = frame.contents.units           # NULL when the frame does not carry this source
    if table:
        units = table.contents             # column table: rows and column pointers
        print(units.rows)
    lib.bwr_release(session, frame)
lib.bwr_close(session)
```

- `pid` is the game's process id; `python -m bwrapi.launcher doctor` lists the running games.
- A table pointer is NULL for a source the frame does not carry, and `.contents` on it raises `ValueError`, so test the pointer first.
- `BwrText32` and `BwrText256` columns are `c_char` arrays in Python. Read one with `.value` and decode it with `bwrapi.bwapi.gamedata.decode_text`: `decode_text(players.name[row].value)`. (`text` is the member's name in C.) `decode_text` tries UTF-8 and then the system code page, because the game keeps some names in its legacy code page (a Korean client's computer names are CP949), which a plain UTF-8 decode garbles.
- `native.abi` (`bwrapi/_native.py`) holds the constants, structures and function signatures generated from the Rust definitions. Function contracts, status codes and the meaning and units of every column are in the [C ABI reference](C-ABI.md).
- An app that reads a running game without injecting (a HUD, for example) attaches with `bwr_open(pid, 0, …)`. If the game runs the in-game DLL, the session uses the frames the game thread published, and commands (`bwr_submit`) work only in that case.
- Chat: in an in-game session (`bwr_open_self`), turning on `bwr_capture_chat` keeps lines the local player types instead of sending them, and `bwr_next_chat` returns one line at a time as a `BwrChat` (`native.abi.Chat`): its kind (sent or received), the game player number that sent a received line (meaningless for a sent line), and the UTF-8 text. Received lines come from the game's replay recording buffer.

## 7. Differences from C++ BWAPI

The names and meaning of classes, functions and constants follow the official BWAPI 4.4 API; its documentation at [bwapi.github.io](https://bwapi.github.io/) applies. Where Python differs:

- **Format strings** (`printf`, `sendText`, `sendTextEx`, `drawText*`) use Python `%` formatting: `Broodwar.printf("%d minerals", n)`. Without arguments the string is used as it is.
- **Sets**: `Unitset`, `Playerset`, `Forceset`, `Regionset` and `Bulletset` are Python `set` subclasses with the BWAPI methods (`size`, `contains`, `getUnitsInRadius`, ...).
- **Filters**: unit filters are in `Filter` (`Filter.IsWorker`, `Filter.HP`, ...). Combine them with `&`, `|` and `~`; compare value filters with `<`, `==` and the other comparison operators (`Filter.HP < 20`). Python's `and`, `or` and `not` raise `TypeError` on a filter. Wherever a predicate is accepted, a plain function or lambda that takes a unit also works. `Lowest(...)` and `Highest(...)` build `getBestUnit` filters.
- **"None" values** are named `None_`, because `None` is reserved in Python: `UnitTypes.None_`, `Positions.None_`, `TilePositions.None_`, and so on for every type namespace.
- **`Broodwar`** forwards to the `Game` of the connected client. Calling a game method while no client is connected raises `RuntimeError` (`dir()` and `help()` still work), and the match collections exist only from the `MatchStart` frame (section 2).
- **Type checking**: the package ships type information. `Broodwar` is typed as a `Game`, iterating a `Unitset` gives `Unit` values (and so on for the other sets), and `UnitTypes`, `Races`, `Orders`, `Colors`, `Text`, `Filter` and the other namespaces declare each constant.
- **Commands** return `True` when they are handed to the server. Their effect shows in the values of later frames (section 5).

## 8. Package and tests

- `cargo xtask package` puts the generated binding `_native.py` and the native files into `python/bwrapi` and builds the wheel (`py3-none-win_amd64`) into `<out>/dist`. The native files are `bin/x64` and `bin/x86` (per game bit width: `bwrapi.dll`, `bwrapi-inject.exe` and the in-game server `bwrapi_bwapi.dll`; `x86` also has the loaders) plus `bin/LICENSE` and `bin/THIRD_PARTY_NOTICES.md`. `native.load()` loads the `bwrapi.dll` of Python's bit width. `package` builds the ABI and the server first.
- `bwapi/types.pyi`, `filters.pyi` and `input_text.pyi` are generated from those modules. After changing one of them, run `python tests/generate_stubs.py` in `python/`; `--check` reports a stub that is out of date.
- Tests (WSL): in `python/`, `python3 -m pytest -q -p no:cacheprovider --bwapi-source=<upstream root>`. The GameData layout tests compile the official headers with a C++ compiler and compare every field; the client tests check the protocol and the facade against a fake server (in-memory GameData and a queue pipe). On Windows, more tests run with the real transport attached to a server built on a Win32 file mapping and named pipe.
