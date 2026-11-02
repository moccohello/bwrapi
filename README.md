# BWRAPI

[한국어](README.ko.md)

BWRAPI is BWAPI for StarCraft: Remastered 1.23.10: a library for reading the game state and issuing commands, for x64 and x86 games. As BWAPI did for Brood War 1.16.1, a DLL inside the game reads the state every frame and executes commands at the frame boundary.

## What it provides

| API | Use it to | Languages |
|---|---|---|
| **BWAPI 4.4 API** | Write bots with the BWAPI 4.4 names and rules. Existing BWAPI 4.4 bots (AIModule DLLs, `BWAPIClient` programs, TournamentModule DLLs) run without changes. | C++ (official BWAPI 4.4 SDK), Python (`bwrapi.bwapi`) |
| **C ABI 1.0** (`bwrapi.dll`) | Read the game state as column tables, issue commands and get their results; from a module inside the game, also draw on the game screen and capture chat. Reading also works from another process without injecting anything (HUDs, analysis tools). | C/C++ (`bwrapi.h`), .NET (`BwrApi.Client`), Python (`bwrapi.native`) |
| **Launcher** | Inject BWRAPI into a running game and start bots (the role Chaoslauncher had). | `python -m bwrapi.launcher` |

- The same features on x64 and x86 games. BWRAPI attaches to a running game, however it was started (Battle.net or local).
- Sync or async: sync makes the game wait for the bot every frame, as BWAPI does; async lets the game run without waiting.
- The C ABI has 19 data sources: players, units, upgrades, techs, unit counts, bullets, vision, viewport, selection, orders, render, game, forces, start locations, unit availability, terrain, walkability, regions and region neighbors. Command results are not in BWAPI, and reading from another process needs no injection.

## Requirements

- Windows 10/11 x64, StarCraft: Remastered **1.23.10**.
- Python 3.11 or later (Windows x64) for the launcher and Python bots.
- C++ AIModule DLLs and BWAPI client programs are x86 binaries built against BWAPI 4.4, as with BWAPI. The launcher looks at the game's bit width and injects the matching x64 or x86 files.

## Install

```powershell
pip install bwrapi
```

The C++ bundle `bwrapi-windows.zip` and the NuGet package `BwrApi.Client` are on [GitHub Releases](https://github.com/moccohello/bwrapi/releases).

| Package | For | Contents |
|---|---|---|
| Python `bwrapi` (PyPI) | Most users | Launcher, Python BWAPI API, C ABI binding, `bwrapi.dll` and the BWAPI server for each game bit width, x86 loaders |
| `bwrapi-windows.zip` | C/C++ users without Python | `x64\` and `x86\` (`bwrapi.dll`, `bwrapi.lib`, `bwrapi_bwapi.dll`, `bwrapi-inject.exe`), x86 loaders, `include\bwrapi.h`, `examples\`, `README.txt` |
| NuGet `BwrApi.Client` | .NET apps | .NET binding for the C ABI with `bwrapi.dll` (x64 and x86 processes) |

## Quick start

Start StarCraft: Remastered as usual first, and open a game (for a bot, a custom game against the computer). The launcher finds the running game; with more than one running, add `--process-id <PID>`.

### Run an existing BWAPI 4.4 bot (AIModule DLL)

```powershell
python -m bwrapi.launcher run C:\Bots\MyBot.dll
```

- The launcher injects `bwrapi.dll` and the BWAPI server into the game and runs the DLL in the x86 loader process. The loader runs in the DLL's folder; if the bot uses `bwapi-data\read` and `write`, set the product `directory` to their parent folder ([launcher guide](python/bwrapi/launcher/README.md)).
- `python -m bwrapi.launcher doctor C:\Bots\MyBot.dll` checks the files and the running games without starting anything.
- `--mode async`: the game does not wait for the bot.
- `--game "C:\Program Files (x86)\StarCraft\x86_64\StarCraft.exe"`: the launcher starts the game itself with the SC:R extended unit limits turned off (1700 units and 100 bullets, the limits BWAPI expects), and restores the setting when the game ends.

### Run a BWAPI 4.4 client program

Programs that connect through BWAPIClient (C++ `BWAPI::BWAPIClient` or any client of the standard shared memory) only need the server. Add a `client` product, inject the server, then start the program as usual.

```json
{ "schema_version": 1, "products": [ { "product_id": "my-client", "entrypoint": "client" } ] }
```

```powershell
python -m bwrapi.launcher --config products.json run my-client   # injects the server and returns
C:\Bots\MyClientBot.exe                                           # finds the server via Local\bwapi_shared_memory_game_list
```

For async, add `--mode async` to `run`; the client reads the mode when it connects.

### Write a bot in Python

```python
# bot.py
from bwrapi.bwapi import AIModule, Broodwar

class Bot(AIModule):
    def onStart(self):
        Broodwar.sendText("hello")

    def onFrame(self):
        Broodwar.drawTextScreen(10, 10, "frame %d", Broodwar.getFrameCount())
```

```powershell
python -m bwrapi.bwapi.run bot.py --inject             # injects the server and connects
python -m bwrapi.bwapi.run bot.py --inject --mode async
```

The names and rules are those of BWAPI 4.4 C++, so the [BWAPI documentation](https://bwapi.github.io/) applies. Python specifics are in the [Python SDK guide](docs/SDK.md); a fuller example is [python/examples/bot.py](python/examples/bot.py).

### Write a bot in C++

A new C++ bot is a BWAPI 4.4 AIModule DLL, built exactly as for BWAPI:

1. Get BWAPI 4.4.0 from the [BWAPI releases](https://github.com/bwapi/bwapi/releases/tag/v4.4.0): `BWAPI.7z` or `BWAPI_Setup.exe`, or clone the `v4.4.0` tag and use `bwapi\bwapi.sln`. It has the headers and the `ExampleAIModule` project to start from. Prebuilt `.lib` files are not included; add the `BWAPILIB` project as a dependency of your bot, as `ExampleAIModule` does.
2. Build for **Win32 (x86)**, Release. The projects target Visual Studio 2017 (`v141_xp`); in a newer Visual Studio, retarget them to your toolset or install that one.
3. Start the game and run the DLL with `python -m bwrapi.launcher run <your bot>.dll` (needs [the Python package](#install); without Python, follow the `README.txt` in `bwrapi-windows.zip`).
4. The server log shows `client connected`, and the bot runs in the match (see [Logs and troubleshooting](#logs-and-troubleshooting)).

A BWAPIClient program (an `.exe` that connects to the server, like the `ExampleAIClient` project) also needs the `BWAPIClient` project and the `Shared` folder it includes, and runs as in [Run a BWAPI 4.4 client program](#run-a-bwapi-44-client-program).

### Read state and issue commands with the C ABI

```c
#include "bwrapi.h"

BwrSession* s = NULL;
if (bwr_open(pid, 0, &s) != BWR_OK) return;        /* a running game */
bwr_subscribe(s, BWR_SOURCE_UNITS | BWR_SOURCE_PLAYERS);
for (;;) {
    if (bwr_step(s, 1000) != BWR_OK) continue;      /* waits for a newer frame */
    const BwrFrame* f = NULL;
    if (bwr_acquire(s, &f) != BWR_OK) continue;
    /* f->units: a column table (row count and one pointer per column) */
    bwr_release(s, f);
}
```

- The function contracts, status codes and every column with its meaning and units are in the [C ABI reference](docs/C-ABI.md). Complete examples: [read_units.c](crates/ffi/examples/read_units.c) and [submit_move.c](crates/ffi/examples/submit_move.c) (also in the zip's `examples\`).
- Link `x64\bwrapi.lib` or `x86\bwrapi.lib` from the zip and ship the matching `bwrapi.dll` beside your program.
- A reader process must have the game's pointer width: an x64 program reads both x64 and x86 games; an x86 program reads only x86 games and fails with `GONE` on an x64 game.
- `bwr_open(pid, 0, &s)` reads frames published by the in-game DLL when it is in the game, and otherwise reads the game's memory from your process (no injection). The tables are the same either way, but only the in-game DLL reads at the frame boundary; a read from outside can catch a frame the game is still updating.
- Commands (`bwr_submit`, `bwr_poll_result`) need the in-game DLL. Inject it with `bwrapi-inject <pid> <zip>\x64\bwrapi.dll` (use `x86\` for an x86 game), or with any launcher command above, which injects it together with the BWAPI server.
- Drawing (`bwr_draw`) and chat capture (`bwr_capture_chat`) work only in a module running inside the game (a session from `bwr_open_self`), injected with `bwrapi-inject <pid> <module>.dll <export>`. See [In-game modules](docs/C-ABI.md#in-game-modules).
- .NET: [`BwrApi.Client`](dotnet/BwrApi.Client/README.md). Python: `bwrapi.native` ([SDK guide](docs/SDK.md#6-abi-10-binding)).

## Coming from BWAPI 4.4

- **Launcher instead of Chaoslauncher**, and no `bwapi.ini`: there is no auto-menu, so start the match yourself (or let the bot attach to a match in progress). The launcher options and product settings replace the ini entries.
- **AIModule DLLs run in an x86 loader process**, connected to the server inside the game, not inside the game process itself. Bots do not notice the difference; their `bwapi-data` folder is the loader's working folder.
- Bot-facing behavior follows BWAPI 4.4. The differences are listed in [docs/BWAPI-COMPATIBILITY.md](docs/BWAPI-COMPATIBILITY.md).

## Logs and troubleshooting

- `python -m bwrapi.launcher doctor <bot>` checks the files and lists the running games with their PIDs and bit widths.
- The server writes `bwrapi_bwapi.log` next to the server DLL it was injected from (for a pip install, `<site-packages>\bwrapi\bin\x64\` or `x86\`). A connected bot shows as `[legacy44] client connected mode=sync` (or `async`).
- The launcher writes its run log to `<product directory>\<product id>.run.jsonl`, or to `--log <file>`; the loader's output goes to `<that file>.loader`.

## Known limitations

- **Command results**: as in BWAPI, `true` from a BWAPI command means it was handed to the game; check its effect in later frames. The C ABI returns command results.
- **Drawing**: shapes and text are drawn with the game's own renderer, so they look different from BWAPI's drawing into an 8-bit screen buffer.
- **Windows only**: connecting to a game works on Windows only.

## Build from source

Requirements (Windows): Rust via rustup (the version in `rust-toolchain.toml` is installed automatically), Visual Studio 2022 or later with the C++ desktop workload (x64 and x86 MSVC, CMake), the .NET 8 SDK, Python 3.11 or later, and git.

```powershell
git clone https://github.com/moccohello/bwrapi.git
cd bwrapi
cargo xtask build     # bwrapi.dll (x64, x86), generated header and bindings, BWAPI server (x64, x86), x86 loaders, .NET
cargo xtask test      # cargo tests, ctest, pytest, .NET tests
cargo xtask package   # build\dist: wheel, NuGet, bwrapi-windows.zip (and the same files as a folder)
```

- Output goes to `build\` by default; change it with `--out <folder>`. No environment variables are needed.
- The BWAPI 4.4 sources are fetched at a pinned commit into `build\_deps\bwapi-src`. Use `--bwapi-source <folder>` for a local copy.
- On Linux (WSL), `build` and `test` run only cargo and pytest. `package` works on Windows only.

## Documentation

- [docs/C-ABI.md](docs/C-ABI.md): C ABI reference (functions, status codes, sources and columns, commands)
- [docs/SDK.md](docs/SDK.md): Python BWAPI API and C ABI binding
- [docs/BWAPI-COMPATIBILITY.md](docs/BWAPI-COMPATIBILITY.md): what matches BWAPI 4.4 and what differs
- [python/bwrapi/launcher/README.md](python/bwrapi/launcher/README.md): launcher commands and product settings
- [dotnet/BwrApi.Client/README.md](dotnet/BwrApi.Client/README.md): .NET binding
- [llms.txt](llms.txt): a short guide for AI coding agents
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) (Korean): internal structure, for contributors

## Issues and contributing

Open an issue on the project repository. Please include:

- Game version (1.23.10), x64 or x86, how the game was started (Battle.net, local, `--game`), sync or async.
- The kind of bot (AIModule DLL, client, Python, C ABI) and, if possible, a bot and map that reproduce the problem.
- The launcher output (`--json`), its run log and `.loader` file, and `bwrapi_bwapi.log` (see [Logs and troubleshooting](#logs-and-troubleshooting)).
- Expected and actual behavior. If it differs from BWAPI 4.4, check [the compatibility document](docs/BWAPI-COMPATIBILITY.md) first.

See [CONTRIBUTING.md](CONTRIBUTING.md) for building and changing BWRAPI.

## License

[MIT](LICENSE). `bwrapi_bwapi.dll` and the x86 loaders are built from BWAPI 4.4 (LGPL-3.0) at a pinned commit and link it. Source locations and the component list are in [THIRD_PARTY_NOTICES.md](host/package/THIRD_PARTY_NOTICES.md), which is included in every package.
