BWRAPI for Windows - C/C++ bundle

BWRAPI is BWAPI for StarCraft: Remastered 1.23.10, x64 and x86 games. It has
two parts:
  - The C ABI 1.0 (bwrapi.dll, bwrapi.h): read the game state as column
    tables, issue commands and get their results, draw on the game screen and
    capture chat. Reading also works from another process without injecting
    anything.
  - A BWAPI 4.4 server (bwrapi_bwapi.dll) that runs inside the game, so
    existing BWAPI 4.4 bots (AIModule DLLs and client programs) run unchanged.
x86 games have the same features as x64 games, including drawing and the
resource totals of repairs and refunds.

Contents
  x64\, x86\           One folder per game bit width. Use the folder that
                       matches the game (or, for the C ABI, your program).
    bwrapi.dll         The C ABI 1.0 library; also the in-game DLL.
    bwrapi.lib         Import library for linking bwrapi.dll with MSVC.
    bwrapi_bwapi.dll   In-game BWAPI 4.4 server. Keep it beside bwrapi.dll.
    bwrapi-inject.exe  Loads a DLL of its folder into a game of the same width.
  x86\bwrapi-legacy44-loader.exe             Connects an AIModule DLL to the server.
  x86\bwrapi-legacy44-tournament-loader.exe  Loader for an AIModule plus a TournamentModule.
  include\bwrapi.h     C ABI 1.0 header.
  examples\            C examples: read_units.c, submit_move.c.
  LICENSE              MIT.
  THIRD_PARTY_NOTICES.md  BWAPI 4.4 (LGPL-3.0) source location and Rust components.

Using the C ABI from C/C++
  Add include\ to the include path, link x64\bwrapi.lib or x86\bwrapi.lib
  (the width of your program) and ship the matching bwrapi.dll beside your
  program. From a Developer Command Prompt for VS in this folder:
    cl /I include examples\read_units.c x64\bwrapi.lib
    copy x64\bwrapi.dll .
    read_units <game PID>
  - bwr_open(pid, 0, &session) reads a running game. Without the in-game DLL
    it reads the game's memory from your process; with it, it copies the
    frames the game publishes. The tables are the same either way.
  - Commands (bwr_submit, bwr_poll_result) need the in-game DLL. Inject
    bwrapi.dll alone, which installs its frame hook:
      x64\bwrapi-inject <game PID> <this folder>\x64\bwrapi.dll
    (x86\ for an x86 game). Injecting the BWAPI server below also injects it.
  - Drawing (bwr_draw) and chat capture (bwr_capture_chat, bwr_next_chat)
    need a session from bwr_open_self in a module DLL running inside the game,
    loaded with bwr_inject or bwrapi-inject <pid> <module.dll> <export>.
  - Reference (functions, status codes, sources, columns, commands):
    https://github.com/moccohello/bwrapi/blob/main/docs/C-ABI.md

Running BWAPI 4.4 bots without Python
1. Start StarCraft: Remastered as usual (Battle.net or local).
2. Inject the server into the game.
   - With the Python launcher (wheel): python -m bwrapi.launcher run C:\Bots\bot.dll
     (it injects and starts the loader in one step).
   - Manually, from the folder that matches the game (x64\ for an x64 game,
     x86\ for an x86 game):
       bwrapi-inject <game PID> <that folder>\bwrapi_bwapi.dll bwrapi_bwapi_callwndproc
     or call bwrapi.dll of your program's width from a program:
       bwr_inject(<game PID>, "<that folder>\bwrapi_bwapi.dll", "bwrapi_bwapi_callwndproc")
     The path is UTF-8. bwrapi.dll from the same folder is injected first when
     the game does not run it yet. If the game and the program have different
     widths, the bwrapi-inject.exe in that folder does the injection. The
     failure reason is bwr_last_error(NULL). Injecting the same file again
     does not load a second copy.
3. Connect the bot.
   - AIModule DLL: x86\bwrapi-legacy44-loader.exe C:\Bots\bot.dll
     If the bot uses bwapi-data\read and bwapi-data\write, run the loader from
     the parent folder of that bwapi-data.
   - TournamentModule: x86\bwrapi-legacy44-tournament-loader.exe <bot.dll> <tournament.dll>
   - BWAPI 4.4 client program: just run it. It finds the server through the
     standard shared memory (Local\bwapi_shared_memory_game_list).
4. Sync or async: by default the game waits for the bot, as in BWAPI. Create
   a file bwrapi_bwapi.mode beside the injected bwrapi_bwapi.dll containing
   async, and clients that connect after that are served without the game
   waiting for them.

The server log is bwrapi_bwapi.log beside the injected bwrapi_bwapi.dll.
The server DLL stays loaded and locked until the game exits; close the game
to replace it.

More
  README:      https://github.com/moccohello/bwrapi
  Differences from BWAPI 4.4:
               https://github.com/moccohello/bwrapi/blob/main/docs/BWAPI-COMPATIBILITY.md
