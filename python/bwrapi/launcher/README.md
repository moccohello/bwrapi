# BWRAPI-BWAPI Launcher

The launcher connects official BWAPI 4.4 consumers (x86 AIModule DLLs, client programs and TournamentModules) to the in-game BWAPI server (`bwrapi_bwapi.dll`).

Terms used below:

- **product**: one configured bot entry in a products JSON file (`product_id`, what to run, where). A module DLL path given on the command line is treated as a product with default settings.
- **profile**: a BWAPI consumer profile name attached to a product. The launcher only reports it; it does not change how anything runs.
- **loader**: the x86 helper process that loads an AIModule DLL (or a TournamentModule with its AI) outside the game and connects it to the in-game server as a BWAPI client. The game process itself never loads the bot DLL.
- **sync / async**: in `sync` mode (as in BWAPI) the game waits for the bot every frame; in `async` mode the game does not wait, and the bot's commands run on the first frame after it responds.

## Commands

Give one of `doctor`, `list` or `run`. The target is a module DLL path or a `product_id` from the products file. `--json` prints the result as JSON, `--log` sets the `run` status JSONL path, `--process-id` names an existing StarCraft PID, `--mode` gives `sync` or `async` in place of the product's `mode`, and `--game` is a `StarCraft.exe` the launcher starts itself.

The sample products file ships with the package as `<site-packages>\bwrapi\launcher\products.sample.json`. To find that folder:

```powershell
python -c "import bwrapi.launcher, pathlib; print(pathlib.Path(bwrapi.launcher.__file__).parent)"
```

Copy the sample somewhere of your own and edit the copy:

```powershell
Copy-Item <that folder>\products.sample.json .\products.json
python -m bwrapi.launcher --config products.json list
python -m bwrapi.launcher --config products.json doctor sample-bwapi-ai
python -m bwrapi.launcher --config products.json run sample-bwapi-ai --process-id 1234
python -m bwrapi.launcher run C:\Bots\bot.dll --mode async
python -m bwrapi.launcher run C:\Bots\bot.dll --game "C:\Program Files (x86)\StarCraft\x86_64\StarCraft.exe"
```

`run` goes through these steps:

1. It passes preflight (the same checks as `doctor`).
2. It picks the game. By default it attaches to a running game. Without a PID it uses the running StarCraft if there is exactly one, and fails otherwise. With `--game`, the launcher starts that executable with `-launch`.
3. It picks the server for the game's bit width (x64 or x86) and writes `mode` into `bwrapi_bwapi.mode` next to that server. The server reads this file every time a client connects. With `async` the game does not wait for the bot; any other value, or no file, means sync, as in BWAPI.
4. It puts `bwrapi.dll` and `bwrapi_bwapi.dll` into the game through a window hook (C ABI `bwr_inject` of the `bwrapi.dll` with the launcher's own bit width). If the game's bit width differs from the launcher's, the `bwrapi-inject.exe` next to the server does the load. Running again on a game that already has them uses the same server. The server DLL stays locked until the game exits, so close the game before rebuilding it.
5. For AIModule and Tournament products, it starts the loader in the product's working folder and waits until it exits. The loader's output goes to `<log>.loader`. A client product ends right after injection, and a separate client process connects with the official `BWAPIClient.connect()`. A Python bot connects with `python -m bwrapi.bwapi.run <bot.py>`; with `--inject` it calls this step itself.

A game started with `--game` runs with the SC:R extended unit limits turned off (1700 units, 100 bullets), so BWAPI unit indexes match the game one to one. Before starting it, the launcher writes `false` to `"Starcraft-Game Extended Unit Limits"` under `"General settings"` in `StarCraft\CSettings.json` in your Documents folder, and restores the original value after the game it started exits (the game may write its settings again while running, so `run` waits for the game to exit even after the loader ends). When such a game exits, the launcher also ends a loader that is still waiting for the next server. Attach mode does not touch this setting.

## Standard consumer settings

Required inputs per `entrypoint` in `products.sample.json`:

- `aimodule` (default): set `module` to the x86 AI DLL. The x86 loader connects the DLL to the server.
- `client`: set `entrypoint: "client"`. There is no product module and no loader.
- `tournament`: set `entrypoint: "tournament"`, `module` and `tournament_module`. The dedicated Tournament loader is selected.

The products JSON has `schema_version` `1` and a `products` array. Product fields are `product_id`, `name`, `profile`, `entrypoint`, `directory`, `module`, `server_x64`, `server_x86`, `loader`, `tournament_module`, `required_files`, `process_id` and `mode` (`sync` by default, or `async`). Copy the sample and change only the values for your setup, such as the real `module` and `directory`. Relative `module` and `directory` paths are resolved against the folder of the products file; relative `server_x64`, `server_x86`, `loader`, `tournament_module` and `required_files` paths against the product's `directory`.

Automatic server and loader selection takes the first location that has both the x64 and the x86 server (and the loader the entrypoint needs), in this order: the `cargo xtask build` output (`Release\bwrapi_bwapi.dll` in `build\host` and `build\host-x86` under the repository root), the installed package's `bwrapi\bin\x64` and `bwrapi\bin\x86` (put there by `cargo xtask package`; this is what a pip install uses), and `%LOCALAPPDATA%\BWRAPI\build\bwrapi-bwapi-x64` and `bwrapi-bwapi-Win32` written by `tools\build-windows-host.ps1`. Each server needs `bwrapi.dll` and `bwrapi-inject.exe` of the same bit width next to it (the build copies them). Use `server_x64`, `server_x86` and `loader` for other locations.

`directory` is the working folder of the AIModule or Tournament loader. Without it, the module's parent folder is used. For a bot that opens `bwapi-data/read` and `bwapi-data/write` as relative paths, set it to the parent of that `bwapi-data` folder.

## Checking a run and reading the results

`doctor` and `run` check that the x64 and x86 server DLLs exist with `bwrapi.dll` next to them, and record the profile for information only. For AIModule and Tournament products they also check the x86 module and loader. Without a target, `doctor` checks the server-only (client) product.

`doctor` also prints the server DLL it selected for each bit width with the path of its `bwrapi_bwapi.log`, and lists the running StarCraft processes with their PID, bit width and whether a BWAPI server is injected and a client connected. Its exit code depends only on the file checks (0 when they pass, 2 otherwise). `run` prints each status event as `[Phase] pid=N reason` and the server log path after injection.

Logs:

- **Launcher run log**: `<directory>\<product_id>.run.jsonl` by default (for a DLL given on the command line, the DLL's folder and file name without extension), or the file given with `--log`. Each line is one status event. The loader's output goes to the same path plus `.loader`.
- **Server log**: `bwrapi_bwapi.log` next to the server DLL that was injected (for a pip install, `<site-packages>\bwrapi\bin\x64\` or `<site-packages>\bwrapi\bin\x86\`, matching the game's bit width). The server appends to it.

Run status `Injected` means the server is in the game, and `Running` means the loader process started. Neither means that the bot connected, that its callbacks succeeded, or that its commands took effect in the game. A connection shows as `[legacy44] client connected mode=sync` (or `mode=async`) in the server log; commands the server could not execute show as `[command] status=not_executed`. Check actual frames, economy and effects by watching the game.
