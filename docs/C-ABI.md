<!-- Generated from crates/ffi and the engine table definitions. Do not edit.
     Regenerate from the repository root: cargo run -p bwrapi-ffi --bin bwrapi-gen -- --doc docs/C-ABI.md -->

# BWRAPI C ABI 1.0 reference

`bwrapi.h` declares the C ABI of `bwrapi.dll`, one header for x64 and x86 games. The same definitions generate this page, the header, the Python declarations (`bwrapi/_native.py`) and the .NET declarations (`Native.g.cs`), so their comments match it.

Contents: [Overview](#overview) · [Functions](#functions) · [Status codes](#status-codes) · [Frames](#frames) · [Sources](#sources) · [Commands](#commands) · [Chat](#chat) · [Drawing](#drawing)

## Overview

**Sessions.** `bwr_open(pid)` opens a running game from any process; `bwr_open_self` opens the calling process from code running inside the game. A session is used by one thread at a time. A call that can fail returns a `BwrStatus`; `bwr_last_error` gives the message of a failure.

**Sources and frames.** `bwr_subscribe` selects sources (`BWR_SOURCE_*`); each is one column table. `bwr_acquire` reads them into a frame: a header and one table pointer per source, NULL when the frame does not carry it. A table is `rows` plus one array per column; cell `i` of every column belongs to row `i`. The frame stays valid and unchanged until `bwr_release`. TERRAIN, WALKABILITY, REGIONS and REGION_NEIGHBORS are read once per match and shared by its frames.

**Values.** Positions are map pixels unless a column says build tiles (32 pixels) or walk tiles (8 pixels); unit x/y are the unit's center. Unit types, orders, techs, upgrades and bullet types are the game's ids, which are BWAPI's numbering. Unit ids are `(slot + 1) | uniqueness << 16`, the same for a unit's whole life; 0 is no unit. A value the game does not have reads its type's sentinel: unsigned columns the type maximum (`UINT8_MAX`, `UINT16_MAX`, `UINT32_MAX`, `UINT64_MAX`), signed columns the minimum (`INT16_MIN`, `INT32_MIN`), text empty. Flag columns are `uint8_t` 0 or 1, 255 when unknown.

**Step loop.** From another process: `bwr_step` waits for the next game frame, `bwr_acquire` reads it, `bwr_release` returns it. Inside the game, `bwr_set_frame_callback` runs a callback once per game frame on the game thread while the game waits, and the callback makes the same calls.

**Commands.** They need a game running the in-game DLL (`bwrapi.dll` inside the game, for example loaded with `bwr_inject`). `bwr_submit` queues `BwrCommand`s and returns their ids at once; they run at the next frame boundary, or right after the frame callback that queued them. `bwr_poll_result` reads each id's `BwrResult`. `BWR_RESULT_OK` means the game's command functions ran; whether the order changes the game shows in later frames.

**Chat.** `bwr_capture_chat` keeps the lines the local player types (instead of sending them) and the lines the game receives; `bwr_next_chat` takes them. Needs a `bwr_open_self` session in a game running the frame hook.

**Drawing.** `bwr_draw` draws a list of BWAPI shapes into every screen update until the next call. Needs a `bwr_open_self` session in a game running the frame hook.

**In the game or outside.** Reading works from another process without injecting anything. When the game runs the in-game DLL, a `bwr_open` session copies the frames its game thread publishes, with the same contents. Commands, frame callbacks, chat and drawing need the in-game DLL.

## Functions

### `bwr_abi_version`

```c
uint32_t bwr_abi_version(void);
```

ABI version of this `bwrapi.dll`: `major << 16 | minor`. Compare the major number with the header's `BWR_ABI_MAJOR`.

### `bwr_open`

```c
BwrStatus bwr_open(uint32_t pid, uint32_t flags, BwrSession** out);
```

Opens a session on game `pid`. When the game runs the in-game DLL the session copies the frames its game thread publishes and `bwr_submit` queues on its command ring; otherwise it reads the game memory from this process and commands are refused. The tables are the same either way; frames the in-game DLL publishes are taken at the frame boundary, while reading from this process can see a frame the game is still updating (`BWR_FRAME_TORN` only reports a frame number that changed during the read). `flags` is reserved; pass 0. No match needs to run yet. Fails with `BWR_GONE` when the process cannot be opened and `BWR_UNSUPPORTED_BUILD` when its build has no layout; `bwr_last_error(NULL)` gives the message. The session is used by one thread at a time and closed with `bwr_close`.

Safety: `out` must be valid for one pointer write.

### `bwr_open_self`

```c
BwrStatus bwr_open_self(BwrSession** out);
```

Opens a session on the calling process: the game thread reads its own memory at the frame boundary instead of through another process, and `bwr_submit` queues on the in-game command ring when this process runs one. This is the only session `bwr_set_frame_callback`, `bwr_capture_chat` and `bwr_draw` accept. The session type and lifetime rules are those of `bwr_open`; failures read with `bwr_last_error(NULL)`.

Safety: `out` must be valid for one pointer write.

### `bwr_close`

```c
void bwr_close(BwrSession* session);
```

Closes `session`; NULL does nothing. Frames acquired from it stay valid until they are released.

Safety: `session` is NULL or from `bwr_open`/`bwr_open_self`, and not used afterwards.

### `bwr_subscribe`

```c
BwrStatus bwr_subscribe(BwrSession* session, uint64_t sources);
```

Selects the sources (`BWR_SOURCE_*` bits) the following `bwr_acquire` calls read; a new session reads none. Fewer sources make a faster read.

Safety: `session` is from `bwr_open` or `bwr_open_self`.

### `bwr_step`

```c
BwrStatus bwr_step(BwrSession* session, uint32_t timeout_ms);
```

Waits up to `timeout_ms` for a game frame newer than the one this session acquired last, or a new match; `bwr_acquire` then reads it. Returns `BWR_OK` at once when there already is one, `BWR_TIMEOUT` when none came, `BWR_NOT_IN_GAME` outside a match and `BWR_GONE` when the process exited. The game may have stepped more than one frame since; compare `game_frame`. A frame callback runs once per game frame and needs no step.

Safety: `session` is from `bwr_open` or `bwr_open_self`.

### `bwr_acquire`

```c
BwrStatus bwr_acquire(BwrSession* session, const BwrFrame** out);
```

Reads the subscribed sources of the current game frame into a new frame and writes it to `*out` (NULL on failure). The frame and its tables stay valid and unchanged until `bwr_release`; several frames may be held at once. `sources` names the tables it carries: a subscribed source that could not be read is missing and its table pointer is NULL. Does not wait for a new game frame (see `bwr_step`). On a game running the in-game DLL the frame is the copy its game thread published at the frame boundary; right after a subscription change the call waits briefly for the new sources. Fails with `BWR_NOT_IN_GAME` outside a match and `BWR_GONE` when the process exited.

Safety: `session` is from `bwr_open` or `bwr_open_self`; `out` is valid for one pointer write.

### `bwr_release`

```c
void bwr_release(BwrSession* session, const BwrFrame* frame);
```

Releases a frame from `bwr_acquire`; NULL does nothing. Any thread may call it, also after `bwr_close`: `session` is not read.

Safety: `frame` is NULL or from `bwr_acquire` and not released before.

### `bwr_last_error`

```c
const char* bwr_last_error(const BwrSession* session);
```

Message of the session's last failure, or of the calling thread's last `bwr_open`, `bwr_open_self` or `bwr_inject` failure when `session` is NULL; empty when there was none. UTF-8, valid until the next call on the same session or thread.

Safety: `session` is NULL or from `bwr_open`/`bwr_open_self`.

### `bwr_set_frame_callback`

```c
BwrStatus bwr_set_frame_callback(BwrSession* session, void (*callback)(void* user), void* user);
```

Registers `callback` on `session`, which must come from `bwr_open_self`: the frame hook calls it once per game frame on the game thread, and the game waits until it returns. Inside the callback the session may `bwr_acquire`, `bwr_release`, `bwr_submit` and `bwr_poll_result`; what `bwr_submit` queues there runs at the same frame boundary, right after the callback returns. `NULL` removes the callback; a later registration replaces the earlier one. Fails with `BWR_INVALID` for a `bwr_open` session and with `BWR_NOT_IN_GAME` when this process runs no frame hook, so nothing would call it.

Safety: `session` is from `bwr_open` or `bwr_open_self`; the callback, when not NULL, stays valid until a later `bwr_set_frame_callback` on this process.

### `bwr_submit`

```c
BwrStatus bwr_submit(BwrSession* session, const BwrCommand* commands, uint32_t count, uint64_t* out_ids);
```

Queues `count` commands and returns at once; the ids written to `out_ids` (one per command) name them in `bwr_poll_result`. Needs a game running the in-game DLL (else `BWR_NO_COMMANDS`) and an acquired frame in a match (else `BWR_NOT_IN_GAME`). `BWR_INVALID` queues none: NULL `commands` or `out_ids`, `count` 0, a `size` other than `sizeof(BwrCommand)`, a unit kind with `unit_count` 0, or NULL `units` with `unit_count` above 0. Units are named by the ids of the last acquired frame. Commands queue on the game's command ring and run at its next frame boundary; inside a frame callback that is this very boundary. What the game thread finds wrong with a command (fields its kind does not take, more than 24 units) ends in its result, not in the status. A command the ring did not take gets id 0.

Safety: `session` is from `bwr_open` or `bwr_open_self`; `commands` holds `count` commands whose `units` arrays are valid; `out_ids` is valid for `count` writes.

### `bwr_poll_result`

```c
BwrStatus bwr_poll_result(BwrSession* session, uint64_t id, BwrResult* out);
```

Current result of a command id from `bwr_submit`, without waiting. A queued command reads `BWR_RESULT_PENDING` until the frame boundary executes it; id 0 reads `BWR_RESULT_UNAVAILABLE`. The game keeps the results of the last 1024 command ids; an older id reads `BWR_RESULT_PENDING` again. Fails with `BWR_NO_COMMANDS` when the game runs no in-game DLL.

Safety: `session` is from `bwr_open` or `bwr_open_self`; `out` is valid for one write.

### `bwr_inject`

```c
BwrStatus bwr_inject(uint32_t pid, const char* module, const char* export_name);
```

Loads the module DLL at `module` (UTF-8 path) into running game `pid` through a window hook: `export_name` names the module's `WH_CALLWNDPROC` hook procedure, which runs on the game thread and installs the module there. The hook is removed after its first message, so a module that stays must pin itself in memory. The `bwrapi.dll` beside the module is loaded first when the game does not run one yet. Loading the same file again does not load a second copy. The module must have the game's pointer width; a game of the other width is loaded by the `bwrapi-inject.exe` beside the module. Fails with `BWR_INVALID` for a NULL or non-UTF-8 argument and with `BWR_GONE` when the load fails; the message is read with `bwr_last_error(NULL)`.

Safety: `module` and `export_name` are NUL-terminated strings.

### `bwr_capture_chat`

```c
BwrStatus bwr_capture_chat(BwrSession* session, uint32_t enable);
```

Turns the in-game chat capture on (`enable` 1) or off (0) on `session`, which must come from `bwr_open_self`. While it is on, a line the local player types into the chat box is not sent but kept, and the lines the game records as received are kept with their sending player; `bwr_next_chat` takes them. Turning it off drops what was not taken. Fails with `BWR_NOT_IN_GAME` when this process runs no frame hook.

Safety: `session` is from `bwr_open` or `bwr_open_self`.

### `bwr_next_chat`

```c
BwrStatus bwr_next_chat(BwrSession* session, BwrChat* out);
```

Takes the oldest kept chat line into `out`; `out->kind` is 0 when none waits.

Safety: `session` is from `bwr_open` or `bwr_open_self`; `out` is valid for one write.

### `bwr_draw`

```c
BwrStatus bwr_draw(BwrSession* session, const BwrDrawList* list);
```

Draws `list` into the game screen on `session`, which must come from `bwr_open_self`: every screen update the game draws until the next call shows the list, above the game screen and its console and below the cursor. The list is copied before the call returns; `NULL` or an empty list draws nothing. A game whose screen updates are off (`SET_GUI` 0) draws nothing until they are on again. Fails with `BWR_INVALID` when a text shape names bytes outside the text, and with `BWR_NOT_IN_GAME` when this process runs no frame hook.

Safety: `session` is from `bwr_open` or `bwr_open_self`; `list` is NULL or holds `count` shapes and `text_bytes` bytes of text.

## Status codes

What a call returns; `bwr_last_error` gives the message of a failure.

| Name | Value | Meaning |
|---|---|---|
| `BWR_OK` | 0 | Success. |
| `BWR_NOT_IN_GAME` | 1 | No match is running; for `bwr_submit`, no frame of a match was acquired; for the in-game calls, this process runs no frame hook. |
| `BWR_TIMEOUT` | 2 | `bwr_step` saw no newer game frame before its timeout. |
| `BWR_GONE` | 3 | The game process exited or cannot be read; `bwr_open` on a system other than Windows; a failed `bwr_inject`. |
| `BWR_INVALID` | 4 | A bad argument: a NULL pointer, a wrong `size` field, a malformed command, or a call that needs a `bwr_open_self` session. |
| `BWR_UNSUPPORTED_BUILD` | 5 | The game build has no layout in this `bwrapi.dll`. |
| `BWR_NO_COMMANDS` | 6 | The game runs no in-game DLL, so nothing executes commands. |

## Frames

### `BwrFrame`

Frame header followed by one table pointer per source, in source bit order; a pointer is NULL when the frame does not carry that source. A table is `{ size_t rows; const T* column...; }`: one array of `rows` cells per column, in the column order of its source.

| Field | Type | Meaning |
|---|---|---|
| `size` | `uint32_t` | `sizeof(BwrFrame)`. |
| `flags` | `uint32_t` | `BWR_FRAME_*` bits. |
| `sequence` | `uint64_t` | Increases by one with every frame this session acquires. |
| `sources` | `uint64_t` | `BWR_SOURCE_*` bits of the tables this frame carries: the subscribed sources that could be read. |
| `match_id` | `uint32_t` | Increases when a new match starts. |
| `game_frame` | `uint32_t` | Game frame number; `UINT32_MAX` when unknown. |
| `map_width` | `uint16_t` | Map width in build tiles; 0 unless the frame was read with `BWR_SOURCE_VISION`. |
| `map_height` | `uint16_t` | Map height in build tiles; 0 as `map_width`. |
| `local_player` | `uint8_t` | Local player id; 255 when unknown (replays, observers). |
| `reserved` | `uint8_t[3]` | Always 0. |
| `players` .. `region_neighbors` | `const Bwr*Table*` | One table pointer per source, in source bit order; NULL when the frame does not carry the source. |

### Frame flags

| Name | Value | Meaning |
|---|---|---|
| `BWR_FRAME_TORN` | 2 | The game frame advanced while the frame was read, twice in a row; rows may mix two game frames. Only a read from another process without the in-game DLL can be torn. |
| `BWR_FRAME_REPLAY` | 4 | The frame is from a replay. |

### Text

`BwrText32` and `BwrText256` are NUL-padded text of 32 and 256 bytes, in their `text` member (.NET: `ToString()`; Python: a `c_char` array, read `.value`).

## In-game modules

`bwr_open_self`, `bwr_set_frame_callback`, `bwr_draw` and `bwr_capture_chat` work only in a DLL running inside the game. Load one with `bwrapi-inject <pid> <module>.dll <export>` or `bwr_inject`; the `bwrapi.dll` beside it is injected first when the game does not run one yet.

`<export>` is a `WH_CALLWNDPROC` hook procedure, called on the game thread. It pins its own DLL so the module stays loaded, opens a session with `bwr_open_self`, subscribes, sets the frame callback once, and passes the message on with `CallNextHookEx`. A complete module: [frame_module.rs](../crates/ffi/examples/frame_module.rs).

## Sources

`bwr_subscribe` takes the `BWR_SOURCE_*` bits; source bit `i` is table `i` of `BwrFrame`.

| Bit | Source | Table | Rows |
|---|---|---|---|
| 0 | [`BWR_SOURCE_PLAYERS`](#bwr_source_players) | `BwrPlayers` at `frame->players` | One row per active player, player ids 0..7; empty, closed and left computer slots are left out, and the neutral player (11) has no row. |
| 1 | [`BWR_SOURCE_UNITS`](#bwr_source_units) | `BwrUnits` at `frame->units` | One row per live unit of the game's unit pool, in pool slot order. Unit id columns other than `id` are 0 for no unit and unknown when the game's link points outside the unit pool. `*_timer` columns are the game's raw timer values, as BWAPI reports them. |
| 2 | [`BWR_SOURCE_UPGRADES`](#bwr_source_upgrades) | `BwrUpgrades` at `frame->upgrades` | Upgrade state per active player and upgrade type; upgrades a player cannot get (maximum level 0) and rows with a level above the maximum or a maximum above 3 are left out. |
| 3 | [`BWR_SOURCE_TECHS`](#bwr_source_techs) | `BwrTechs` at `frame->techs` | Tech state per active player and tech type. |
| 4 | [`BWR_SOURCE_UNIT_COUNTS`](#bwr_source_unit_counts) | `BwrUnitCounts` at `frame->unit_counts` | Unit counts per player and type; rows with every count zero are left out. |
| 5 | [`BWR_SOURCE_BULLETS`](#bwr_source_bullets) | `BwrBullets` at `frame->bullets` | One row per active bullet: projectiles and weapon effects. |
| 6 | [`BWR_SOURCE_VISION`](#bwr_source_vision) | `BwrVision` at `frame->vision` | One row per build tile, row-major (`y * map_width + x`). |
| 7 | [`BWR_SOURCE_VIEWPORT`](#bwr_source_viewport) | `BwrViewport` at `frame->viewport` | One row: the local screen and the cursor. |
| 8 | [`BWR_SOURCE_SELECTION`](#bwr_source_selection) | `BwrSelection` at `frame->selection` | The local player's current selection, in selection order. |
| 9 | [`BWR_SOURCE_ORDERS`](#bwr_source_orders) | `BwrOrders` at `frame->orders` | Queued orders after each unit's current order, in queue order. |
| 10 | [`BWR_SOURCE_RENDER`](#bwr_source_render) | `BwrRender` at `frame->render` | Each live unit's sprite: where it is drawn, in map pixels (it differs from the unit position while the sprite is offset, for example lifting), and who sees it. |
| 11 | [`BWR_SOURCE_GAME`](#bwr_source_game) | `BwrGame` at `frame->game` | One row: the running game. |
| 12 | [`BWR_SOURCE_FORCES`](#bwr_source_forces) | `BwrForces` at `frame->forces` | The map's forces (teams). |
| 13 | [`BWR_SOURCE_START_LOCATIONS`](#bwr_source_start_locations) | `BwrStartLocations` at `frame->start_locations` | Start locations of the map in slot order; unused and repeated ones are left out. |
| 14 | [`BWR_SOURCE_UNIT_AVAILABILITY`](#bwr_source_unit_availability) | `BwrUnitAvailability` at `frame->unit_availability` | Whether each active player may build each unit type. |
| 15 | [`BWR_SOURCE_TERRAIN`](#bwr_source_terrain) | `BwrTerrain` at `frame->terrain` | Map tiles that do not change during a match, one row per build tile, row-major (`y * map_width + x`). Read once per match and shared by every frame. |
| 16 | [`BWR_SOURCE_WALKABILITY`](#bwr_source_walkability) | `BwrWalkability` at `frame->walkability` | One row per walk tile (8 pixels), row-major over `map_width*4` columns and `map_height*4` rows. Read once per match. |
| 17 | [`BWR_SOURCE_REGIONS`](#bwr_source_regions) | `BwrRegions` at `frame->regions` | Pathing regions; read once per match. |
| 18 | [`BWR_SOURCE_REGION_NEIGHBORS`](#bwr_source_region_neighbors) | `BwrRegionNeighbors` at `frame->region_neighbors` | Neighbor lists of all regions, one neighbor per row; `Regions.neighbor_start` and `neighbor_count` select a region's rows. Read once per match. |

### BWR_SOURCE_PLAYERS

One row per active player, player ids 0..7; empty, closed and left computer slots are left out, and the neutral player (11) has no row.

`BwrPlayers` table at `frame->players`.

| Column | Type | Meaning |
|---|---|---|
| `id` | `uint8_t` | Game player id 0..7. |
| `race` | `uint8_t` | 0 Zerg, 1 Terran, 2 Protoss; other values are the game's race byte (BWAPI Races numbering). |
| `minerals` | `int32_t` | Current minerals. |
| `gas` | `int32_t` | Current gas. |
| `name` | `BwrText32` | Player name as the game stores it: UTF-8, or the client's legacy code page for names it localizes (a Korean client's computer names are CP949). Empty when unknown. |
| `player_type` | `uint8_t` | Game player type byte (BWAPI PlayerTypes numbering). |
| `force` | `uint8_t` | Force 1..4; 0 none. |
| `color` | `uint8_t` | Game color index. |
| `supply_used_zerg` | `uint32_t` | Zerg supply used, in half units (twice the supply the game shows). |
| `supply_used_terran` | `uint32_t` | Terran supply used, in half units. |
| `supply_used_protoss` | `uint32_t` | Protoss supply used, in half units. |
| `supply_total_zerg` | `uint32_t` | Zerg supply total, in half units. |
| `supply_total_terran` | `uint32_t` | Terran supply total, in half units. |
| `supply_total_protoss` | `uint32_t` | Protoss supply total, in half units. |
| `start_x` | `int16_t` | Start location x in build tiles: the top-left tile of the start resource depot; unknown when the player has none or the slot is unaligned, off the map or repeated. |
| `start_y` | `int16_t` | Start location y in build tiles. |
| `victory` | `uint8_t` | Raw victory status byte: 3 victory; 1, 2, 4 and 6 defeat (as BWAPI reads it, for players 0..7). |
| `left_game` | `uint8_t` | 1 when the player left the game. |
| `gathered_minerals` | `int32_t` | Minerals gathered this match. |
| `gathered_gas` | `int32_t` | Gas gathered this match. |
| `unit_score` | `int32_t` | The game's unit score. |
| `kill_score` | `int32_t` | The game's kill score. |
| `building_score` | `int32_t` | The game's building score. |
| `razing_score` | `int32_t` | The game's razing score. |
| `custom_score` | `int32_t` | Score the map's triggers set. |
| `allies` | `uint16_t` | Bit `p` set: allied with player `p`. |
| `participating` | `uint8_t` | BWAPI match-start participation: the player owns a unit, or a trigger whose conditions can be met gives or creates units for it. Read on game frame 0 only, while the start units and trigger lists are still the match's own; unknown on later frames and for trigger modes it cannot read. |
| `repaired_minerals` | `int32_t` | Minerals paid for repairs this match; unknown without the in-game DLL. |
| `repaired_gas` | `int32_t` | Gas paid for repairs this match; unknown without the in-game DLL. |
| `refunded_minerals` | `int32_t` | Minerals got back from cancels this match; unknown without the in-game DLL. |
| `refunded_gas` | `int32_t` | Gas got back from cancels this match; unknown without the in-game DLL. |

### BWR_SOURCE_UNITS

One row per live unit of the game's unit pool, in pool slot order. Unit id columns other than `id` are 0 for no unit and unknown when the game's link points outside the unit pool. `*_timer` columns are the game's raw timer values, as BWAPI reports them.

`BwrUnits` table at `frame->units`.

| Column | Type | Meaning |
|---|---|---|
| `id` | `uint64_t` | Unit id, `(slot + 1) \| uniqueness << 16`; stable for the unit's life. |
| `unit_type` | `uint16_t` | Unit type (BWAPI UnitTypes numbering). |
| `owner` | `uint8_t` | Owning player id. |
| `x` | `int16_t` | Center x in map pixels. |
| `y` | `int16_t` | Center y in map pixels. |
| `hit_points` | `int32_t` | Hit points in 1/256 units; the shown value is this / 256, rounded up. |
| `shields` | `int32_t` | Shields in 1/256 units, rounded up for the shown value; 0 for types without shields. |
| `energy` | `uint16_t` | Energy in 1/256 units, rounded up for the shown value; unknown for types without energy. |
| `resources` | `uint16_t` | Remaining minerals or gas; unknown for non-resource types. |
| `order` | `uint8_t` | Current order (BWAPI Orders numbering). |
| `order_target` | `uint64_t` | Unit the current order targets; 0 none. |
| `order_target_x` | `int16_t` | Target position x of the current order, map pixels. |
| `order_target_y` | `int16_t` | Target position y of the current order, map pixels. |
| `secondary_order` | `uint8_t` | Secondary order (BWAPI Orders numbering); unknown when out of range. |
| `status_flags` | `uint32_t` | Raw CUnit status flags. |
| `detected_by` | `uint32_t` | Bit `p` set: detected by player `p`. |
| `completed` | `uint8_t` | 1 when the unit is completed; a morphing unit reads 0. |
| `burrowed` | `uint8_t` | 1 when burrowed. |
| `cloaked` | `uint8_t` | 1 when cloaked and not burrowed. |
| `in_air` | `uint8_t` | 1 for an air unit or a lifted building. |
| `hallucination` | `uint8_t` | 1 for a hallucination. |
| `interruptible` | `uint8_t` | 1 when the unit accepts new orders. |
| `powered` | `uint8_t` | 0 for a Protoss building without power; 1 otherwise. |
| `invincible` | `uint8_t` | Status bit: the unit cannot be damaged. |
| `loaded` | `uint8_t` | 1 inside a transport or a Bunker; unknown when that carrier cannot be resolved. |
| `transport` | `uint64_t` | Transport or Bunker carrying the unit; 0 none. |
| `addon` | `uint64_t` | Addon of a Terran building; 0 none. |
| `hatchery` | `uint64_t` | Hatchery of a larva; 0 none. |
| `build_type` | `uint16_t` | Unit type being built, trained or morphed into; 228 (None) when nothing. |
| `remaining_build_frames` | `uint32_t` | Frames left to construct or morph this unit. |
| `researching` | `uint8_t` | 1 while the unit researches a tech. |
| `upgrading` | `uint8_t` | 1 while the unit researches an upgrade. |
| `spider_mines` | `uint8_t` | Spider mines of a Vulture; unknown for other types. |
| `lockdown_timer` | `uint8_t` | Lockdown timer. |
| `maelstrom_timer` | `uint8_t` | Maelstrom timer. |
| `stasis_timer` | `uint8_t` | Stasis Field timer. |
| `irradiate_timer` | `uint8_t` | Irradiate timer. |
| `plague_timer` | `uint8_t` | Plague timer. |
| `ensnare_timer` | `uint8_t` | Ensnare timer. |
| `defense_matrix_timer` | `uint8_t` | Defensive Matrix timer. |
| `remove_timer` | `uint16_t` | Time left before a temporary unit (a hallucination, a Broodling) is removed; 0 for a lasting unit. |
| `rally_x` | `int16_t` | Rally point x in map pixels, for unit types that rally; unknown otherwise or while no rally point is set. |
| `rally_y` | `int16_t` | Rally point y in map pixels; unknown as `rally_x`. |
| `rally_unit` | `uint64_t` | Unit the rally point follows; 0 none, unknown for types that do not rally. |
| `gas_building` | `uint64_t` | Refinery/Extractor/Assimilator a gas worker gathers from; 0 none. |
| `larva_frames` | `uint32_t` | Frames until a hatchery spawns its next larva; 0 with three larvae; unknown for other types. |
| `remaining_train_frames` | `uint32_t` | Frames left on the unit at the front of the training queue. |
| `train_queue_0` | `uint16_t` | Training queue slot 0 (front), a unit type; unknown past the queue's end. |
| `train_queue_1` | `uint16_t` | Training queue slot 1, a unit type; unknown past the queue's end. |
| `train_queue_2` | `uint16_t` | Training queue slot 2, a unit type; unknown past the queue's end. |
| `train_queue_3` | `uint16_t` | Training queue slot 3, a unit type; unknown past the queue's end. |
| `train_queue_4` | `uint16_t` | Training queue slot 4, a unit type; unknown past the queue's end. |
| `tech` | `uint8_t` | Tech being researched (BWAPI TechTypes numbering); unknown when none. |
| `remaining_research_frames` | `uint32_t` | Frames left on the research; unknown when none. |
| `upgrade` | `uint8_t` | Upgrade in progress (BWAPI UpgradeTypes numbering); unknown when none. |
| `remaining_upgrade_frames` | `uint32_t` | Frames left on the upgrade; unknown when none. |
| `build_unit` | `uint64_t` | Unit this unit is constructing (a building, an addon, a nydus exit); 0 none. |
| `move_target` | `uint64_t` | Unit the movement heads for; 0 none. |
| `move_target_x` | `int16_t` | Movement target x in map pixels. |
| `move_target_y` | `int16_t` | Movement target y in map pixels. |
| `sub_unit` | `uint64_t` | Turret of a Siege Tank or Goliath; 0 none. |
| `carrier` | `uint64_t` | Carrier of an Interceptor; 0 none; unknown for every other type. |
| `nydus_exit` | `uint64_t` | Exit of a Nydus Canal; 0 none. |
| `power_up` | `uint64_t` | Flag or mineral chunk a completed worker carries; 0 none; unknown for every other unit. |
| `order_timer` | `uint16_t` | Order timer. |
| `stim_timer` | `uint8_t` | Stim Packs timer. |
| `storm_timer` | `uint8_t` | Psionic Storm timer. |
| `defense_matrix_damage` | `uint16_t` | Damage the Defensive Matrix still absorbs, in 1/256 units. |
| `ground_cooldown` | `uint8_t` | Frames until the ground weapon can fire again. |
| `air_cooldown` | `uint8_t` | Frames until the air weapon can fire again. |
| `spell_cooldown` | `uint8_t` | Spell cooldown. |
| `kill_count` | `uint16_t` | Units this unit killed. |
| `acid_spores` | `uint8_t` | Acid spores on the unit. |
| `scarabs` | `uint8_t` | Scarabs a Reaver holds; unknown for other types. |
| `interceptors` | `uint8_t` | Interceptors a Carrier has, docked or out; unknown for other types. |
| `resource_group` | `uint8_t` | Resource group of a resource container; unknown for other types. |
| `carry` | `uint8_t` | Resource a worker carries: 0 none, 1 gas, 2 minerals; unknown for other types. |
| `last_attacker` | `uint8_t` | Player that last attacked the unit; unknown when none. |
| `recently_attacked` | `uint8_t` | 1 while the unit was recently attacked (the game flashes it on the minimap). |
| `button_set` | `uint16_t` | Game button set id. |
| `previous_hit_points` | `uint16_t` | Hit points at the previous update, in 1/256 units. |
| `being_healed` | `uint8_t` | 1 while being healed. |
| `gathering` | `uint8_t` | 1 while a worker gathers; unknown for other types. |
| `being_gathered` | `uint8_t` | A worker gathers from or waits in line at this resource container; unknown for every other type. |
| `blind` | `uint8_t` | 1 when blinded by Optical Flare. |
| `parasited` | `uint8_t` | 1 when parasited. |
| `has_nuke` | `uint8_t` | 1 when a Nuclear Silo holds a ready nuke. |
| `stuck` | `uint8_t` | 1 when the unit is stuck and the game moves it to a legal spot. |
| `moving` | `uint8_t` | 1 while moving. |
| `accelerating` | `uint8_t` | 1 while accelerating. |
| `braking` | `uint8_t` | 1 while braking. |
| `facing` | `uint16_t` | Facing direction 0..255, 1/256 turn per step; BWAPI's angle is `(facing - 64) * pi / 128`. |
| `velocity_x` | `int32_t` | Velocity x in 1/256 pixel per frame. |
| `velocity_y` | `int32_t` | Velocity y in 1/256 pixel per frame. |

### BWR_SOURCE_UPGRADES

Upgrade state per active player and upgrade type; upgrades a player cannot get (maximum level 0) and rows with a level above the maximum or a maximum above 3 are left out.

`BwrUpgrades` table at `frame->upgrades`.

| Column | Type | Meaning |
|---|---|---|
| `player` | `uint8_t` | Player id. |
| `upgrade_type` | `uint16_t` | Upgrade type (BWAPI UpgradeTypes numbering). |
| `level` | `uint8_t` | Current level. |
| `max_level` | `uint8_t` | Highest level the player can reach. |
| `in_progress` | `uint8_t` | 1 while the next level is being researched. |
| `remaining_frames` | `uint32_t` | Frames left while in progress, 0 when not; unknown when no building is seen working on it. |

### BWR_SOURCE_TECHS

Tech state per active player and tech type.

`BwrTechs` table at `frame->techs`.

| Column | Type | Meaning |
|---|---|---|
| `player` | `uint8_t` | Player id. |
| `tech_type` | `uint16_t` | Tech type (BWAPI TechTypes numbering). |
| `researched` | `uint8_t` | 1 when researched. |
| `available` | `uint8_t` | 1 when the player may research it. |
| `in_progress` | `uint8_t` | 1 while being researched. |
| `remaining_frames` | `uint32_t` | Frames left while in progress, 0 when not; unknown when no building is seen working on it. |

### BWR_SOURCE_UNIT_COUNTS

Unit counts per player and type; rows with every count zero are left out.

`BwrUnitCounts` table at `frame->unit_counts`.

| Column | Type | Meaning |
|---|---|---|
| `player` | `uint8_t` | Player id. |
| `unit_type` | `uint16_t` | Unit type (BWAPI UnitTypes numbering). |
| `all` | `uint32_t` | Units of the type the player has, incomplete ones included. |
| `completed` | `uint32_t` | Completed units of the type the player has. |
| `killed` | `uint32_t` | Units of the type the player killed. |
| `dead` | `uint32_t` | Units of the type the player lost. |

### BWR_SOURCE_BULLETS

One row per active bullet: projectiles and weapon effects.

`BwrBullets` table at `frame->bullets`.

| Column | Type | Meaning |
|---|---|---|
| `id` | `uint64_t` | Bullet id: its pool slot + 1. |
| `bullet_type` | `uint16_t` | Bullet type (BWAPI BulletTypes numbering). |
| `owner` | `uint8_t` | Player of the source unit; unknown without one. |
| `x` | `int16_t` | Position x in map pixels. |
| `y` | `int16_t` | Position y in map pixels. |
| `source` | `uint64_t` | Unit that fired it; 0 none. |
| `target` | `uint64_t` | Unit it flies at; 0 none. |
| `remove_timer` | `uint8_t` | Time left before the bullet is removed. |
| `visible_to` | `uint32_t` | Bit `p` set: visible to player `p`. |
| `target_x` | `int16_t` | Target position x in map pixels. |
| `target_y` | `int16_t` | Target position y in map pixels. |
| `velocity_x` | `int32_t` | Raw velocity x (1/256 pixel per frame). |
| `velocity_y` | `int32_t` | Raw velocity y (1/256 pixel per frame). |
| `direction` | `uint8_t` | Raw direction (0..255, 0 = up). |

### BWR_SOURCE_VISION

One row per build tile, row-major (`y * map_width + x`).

`BwrVision` table at `frame->vision`.

| Column | Type | Meaning |
|---|---|---|
| `visible` | `uint16_t` | Bit `p` set: tile visible to player `p` (players 0..7). |
| `explored` | `uint16_t` | Bit `p` set: tile explored by player `p` (players 0..7). |
| `creep` | `uint8_t` | Creep on the tile now, the map's own creep included. |
| `occupied` | `uint8_t` | A building stands on the tile. |

### BWR_SOURCE_VIEWPORT

One row: the local screen and the cursor.

`BwrViewport` table at `frame->viewport`.

| Column | Type | Meaning |
|---|---|---|
| `x` | `int32_t` | Left edge of the local screen in map pixels. |
| `y` | `int32_t` | Top edge of the local screen in map pixels. |
| `mouse_x` | `int32_t` | Cursor x in game screen pixels; unknown where the build has no cursor address. |
| `mouse_y` | `int32_t` | Cursor y in game screen pixels; unknown as `mouse_x`. |

### BWR_SOURCE_SELECTION

The local player's current selection, in selection order.

`BwrSelection` table at `frame->selection`.

| Column | Type | Meaning |
|---|---|---|
| `unit` | `uint64_t` | Selected unit id. |

### BWR_SOURCE_ORDERS

Queued orders after each unit's current order, in queue order.

`BwrOrders` table at `frame->orders`.

| Column | Type | Meaning |
|---|---|---|
| `unit` | `uint64_t` | Unit whose queue holds the order. |
| `index` | `uint8_t` | Position in the queue, from 0. |
| `order` | `uint8_t` | Queued order (BWAPI Orders numbering). |
| `unit_type` | `uint16_t` | Unit type to build, for build orders. |
| `x` | `int16_t` | Target position x in map pixels. |
| `y` | `int16_t` | Target position y in map pixels. |
| `target` | `uint64_t` | Target unit; 0 none. |

### BWR_SOURCE_RENDER

Each live unit's sprite: where it is drawn, in map pixels (it differs from the unit position while the sprite is offset, for example lifting), and who sees it.

`BwrRender` table at `frame->render`.

| Column | Type | Meaning |
|---|---|---|
| `unit` | `uint64_t` | Unit whose sprite this is. |
| `x` | `int16_t` | Drawn position x in map pixels. |
| `y` | `int16_t` | Drawn position y in map pixels. |
| `visible_to` | `uint16_t` | Bit `p` set: the sprite is visible to player `p`. |
| `animation` | `uint8_t` | Iscript animation the sprite's main image plays (0 init, 2/3 ground/air attack init, 5/6 ground/air attack repeat, ...). |
| `frame_set` | `uint16_t` | Frame set the main image shows. |

### BWR_SOURCE_GAME

One row: the running game.

`BwrGame` table at `frame->game`.

| Column | Type | Meaning |
|---|---|---|
| `game_type` | `uint8_t` | BWAPI GameTypes numbering; unknown when the build has no address for it. |
| `unit_slots` | `uint16_t` | Records the CUnit pool of this game holds. |
| `multiplayer` | `uint8_t` | 1 in a network match. |
| `battle_net` | `uint8_t` | 1 in a Battle.net game. |
| `paused` | `uint8_t` | 1 while paused. |
| `latency` | `uint8_t` | Raw latency setting (0 low, 1 high, 2 extra high). |
| `latency_frames` | `uint32_t` | Command latency in game frames. |
| `turns_sent` | `uint32_t` | Turns the game has sent; it sends one right before each frame it steps and keeps sending while paused. |
| `last_turn_qpc` | `uint64_t` | QPC of the last turn send: the in-game watcher's stamp of the latest `turns_sent` change it saw. Unknown without the watcher or while its count is not this row's. |
| `frame_ms` | `uint32_t` | Milliseconds per game frame at the current speed. |
| `elapsed_seconds` | `int32_t` | Game time in seconds, as the game counts it. |
| `random_seed` | `uint32_t` | Random seed the match started with. |
| `replay_frames` | `uint32_t` | Frame count of the loaded replay. |
| `countdown_seconds` | `uint32_t` | Seconds left on the map's countdown timer. |
| `save_tick` | `uint32_t` | Tick count (ms) of the local player's last save, 0 before the first. |
| `save_name` | `BwrText32` | Name of the local player's last save. |
| `map_file` | `BwrText256` | Map file name without its directory. |
| `map_path` | `BwrText256` | Map file path as the game holds it. |
| `map_title` | `BwrText256` | Map title. |

### BWR_SOURCE_FORCES

The map's forces (teams).

`BwrForces` table at `frame->forces`.

| Column | Type | Meaning |
|---|---|---|
| `id` | `uint8_t` | 1..4. |
| `name` | `BwrText32` | Force name. |

### BWR_SOURCE_START_LOCATIONS

Start locations of the map in slot order; unused and repeated ones are left out.

`BwrStartLocations` table at `frame->start_locations`.

| Column | Type | Meaning |
|---|---|---|
| `x` | `int16_t` | Top-left build tile x of the start resource depot. |
| `y` | `int16_t` | Top-left build tile y of the start resource depot. |

### BWR_SOURCE_UNIT_AVAILABILITY

Whether each active player may build each unit type.

`BwrUnitAvailability` table at `frame->unit_availability`.

| Column | Type | Meaning |
|---|---|---|
| `player` | `uint8_t` | Player id. |
| `unit_type` | `uint16_t` | Unit type (BWAPI UnitTypes numbering). |
| `available` | `uint8_t` | 1 when the player may build the type. |

### BWR_SOURCE_TERRAIN

Map tiles that do not change during a match, one row per build tile, row-major (`y * map_width + x`). Read once per match and shared by every frame.

`BwrTerrain` table at `frame->terrain`. Read once per match and shared by its frames.

| Column | Type | Meaning |
|---|---|---|
| `buildable` | `uint8_t` | 1 when buildings may be placed on the tile. |
| `ground_height` | `uint8_t` | 0 low, 1 low doodad, 2 high, 3 high doodad, 4 very high, 5 very high doodad. |
| `creep` | `uint8_t` | Creep when the match tables were first read: the match start for a session attached from frame 0, otherwise that moment. |
| `region` | `uint16_t` | Region of the tile (`Regions.id`), or of its first part when the tile is split. |
| `region2` | `uint16_t` | Region of the second part of a split tile; equals `region` otherwise. |
| `split_mask` | `uint16_t` | Mini-tiles (4x4, bit `y*4+x`) that belong to `region2`. |

### BWR_SOURCE_WALKABILITY

One row per walk tile (8 pixels), row-major over `map_width*4` columns and `map_height*4` rows. Read once per match.

`BwrWalkability` table at `frame->walkability`. Read once per match and shared by its frames.

| Column | Type | Meaning |
|---|---|---|
| `walkable` | `uint8_t` | 1 when ground units can walk on the walk tile. |

### BWR_SOURCE_REGIONS

Pathing regions; read once per match.

`BwrRegions` table at `frame->regions`. Read once per match and shared by its frames.

| Column | Type | Meaning |
|---|---|---|
| `id` | `uint16_t` | Region id, as `Terrain.region` names it. |
| `group` | `uint16_t` | Regions in one group connect by ground. |
| `center_x` | `int16_t` | Center x in map pixels. |
| `center_y` | `int16_t` | Center y in map pixels. |
| `left` | `int16_t` | Left edge of the bounding box, map pixels. |
| `top` | `int16_t` | Top edge of the bounding box, map pixels. |
| `right` | `int16_t` | Right edge of the bounding box, map pixels. |
| `bottom` | `int16_t` | Bottom edge of the bounding box, map pixels. |
| `accessible` | `uint8_t` | 1 when ground units can stand in the region. |
| `higher_ground` | `uint8_t` | 1 for higher ground (BWAPI Region isHigherGround). |
| `priority` | `uint8_t` | Defense priority (BWAPI Region defensePriority). |
| `neighbor_start` | `uint32_t` | First row of this region's neighbors in `RegionNeighbors`. |
| `neighbor_count` | `uint16_t` | Number of this region's rows in `RegionNeighbors`. |

### BWR_SOURCE_REGION_NEIGHBORS

Neighbor lists of all regions, one neighbor per row; `Regions.neighbor_start` and `neighbor_count` select a region's rows. Read once per match.

`BwrRegionNeighbors` table at `frame->region_neighbors`. Read once per match and shared by its frames.

| Column | Type | Meaning |
|---|---|---|
| `region` | `uint16_t` | Neighboring region id. |

## Commands

### `BwrCommand`

One command for `bwr_submit`. `kind` is a `BWR_COMMAND_*` value; `x`/`y` are pixels (build tiles for BUILD, LAND and PLACE_COP); `type_id` and `slot` are read only by the kinds that take them; `units` lists the acting unit ids. A field a kind does not read is ignored.

| Field | Type | Meaning |
|---|---|---|
| `size` | `uint32_t` | `sizeof(BwrCommand)`. |
| `kind` | `uint32_t` | `BWR_COMMAND_*`. |
| `flags` | `uint32_t` | `BWR_COMMAND_FLAG_*` bits. |
| `type_id` | `uint32_t` | Unit type (BUILD, BUILD_ADDON, TRAIN, MORPH), tech type (RESEARCH, USE_TECH*) or upgrade type (UPGRADE), in BWAPI numbering; 1 for SEND_TEXT to allies. |
| `slot` | `uint32_t` | Training queue slot of CANCEL_TRAIN_SLOT. |
| `x` | `int32_t` | Map pixels, build tiles or a value, as the kind says. |
| `y` | `int32_t` | Map pixels, build tiles or a value, as the kind says. |
| `target` | `uint64_t` | Target unit id; 0 none. |
| `units` | `const uint64_t*` | Acting unit ids from the last acquired frame; 1..24 for unit kinds. |
| `unit_count` | `uint32_t` | Ids at `units`; 0 for game kinds. |
| `reserved` | `uint32_t` | Not read; set 0. |
| `text` | `const char*` | NUL-terminated UTF-8 text of the text kinds, NULL otherwise. |

### Command flags

`BwrCommand.flags` bits.

| Name | Value | Meaning |
|---|---|---|
| `BWR_COMMAND_FLAG_QUEUED` | 1 | Adds the order after the unit's queued orders; only the kinds marked "Queues" take it. |
| `BWR_COMMAND_FLAG_TILE` | 2 | Marks x/y as build tiles for the caller's own bookkeeping; the kind alone decides how x/y are read. |

### Unit commands

`BwrCommand.kind` values of the unit commands, in kind order (BWAPI's UnitCommandTypes). Each names the fields it reads besides `units` (1..24 acting unit ids); x/y are map pixels (0..65535) unless they say build tiles. "Queues" marks the kinds that take `BWR_COMMAND_FLAG_QUEUED`, "one unit" the kinds that take exactly one acting unit.

| Name | Value | Meaning |
|---|---|---|
| `BWR_COMMAND_MOVE` | 0 | x, y. Queues. |
| `BWR_COMMAND_SMART_TARGET_UNIT` | 1 | target; a right click on it. Queues. |
| `BWR_COMMAND_ATTACK_UNIT` | 2 | target. Queues. |
| `BWR_COMMAND_ATTACK_POSITION` | 3 | x, y. Queues. |
| `BWR_COMMAND_SET_RALLY_POSITION` | 4 | x, y. |
| `BWR_COMMAND_BUILD` | 5 | type_id (building unit type); x, y the top-left build tile. One unit. |
| `BWR_COMMAND_BUILD_ADDON` | 6 | type_id (addon unit type). One unit. |
| `BWR_COMMAND_TRAIN` | 7 | type_id (unit type). |
| `BWR_COMMAND_RESEARCH` | 8 | type_id (tech type). One unit. |
| `BWR_COMMAND_UPGRADE` | 9 | type_id (upgrade type). One unit. |
| `BWR_COMMAND_LIFT` | 10 | No fields. |
| `BWR_COMMAND_LAND` | 11 | x, y the top-left build tile. One unit. |
| `BWR_COMMAND_STOP` | 12 | No fields. Queues. |
| `BWR_COMMAND_HOLD_POSITION` | 13 | No fields. Queues. |
| `BWR_COMMAND_MORPH` | 14 | type_id (unit type). |
| `BWR_COMMAND_SET_RALLY_UNIT` | 15 | target. |
| `BWR_COMMAND_PATROL` | 16 | x, y. Queues. |
| `BWR_COMMAND_FOLLOW` | 17 | target. Queues. |
| `BWR_COMMAND_GATHER` | 18 | target, a mineral field or gas building. Queues. |
| `BWR_COMMAND_RETURN_CARGO` | 19 | No fields. Queues. |
| `BWR_COMMAND_REPAIR` | 20 | target. Queues. |
| `BWR_COMMAND_BURROW` | 21 | No fields. |
| `BWR_COMMAND_UNBURROW` | 22 | No fields. |
| `BWR_COMMAND_CLOAK` | 23 | No fields. |
| `BWR_COMMAND_DECLOAK` | 24 | No fields. |
| `BWR_COMMAND_SIEGE` | 25 | No fields. |
| `BWR_COMMAND_UNSIEGE` | 26 | No fields. |
| `BWR_COMMAND_LOAD` | 27 | target, the transport or Bunker to enter or the unit to take in. Queues. |
| `BWR_COMMAND_UNLOAD` | 28 | target, the passenger. |
| `BWR_COMMAND_UNLOAD_ALL` | 29 | No fields. Queues. |
| `BWR_COMMAND_UNLOAD_ALL_POSITION` | 30 | x, y. Queues. |
| `BWR_COMMAND_RIGHT_CLICK_POSITION` | 31 | x, y. Queues. |
| `BWR_COMMAND_HALT_CONSTRUCTION` | 32 | No fields. |
| `BWR_COMMAND_CANCEL_CONSTRUCTION` | 33 | No fields. |
| `BWR_COMMAND_CANCEL_ADDON` | 34 | No fields. |
| `BWR_COMMAND_CANCEL_TRAIN` | 35 | Cancels the last unit of the training queue. |
| `BWR_COMMAND_CANCEL_TRAIN_SLOT` | 36 | slot, the training queue slot from 0. |
| `BWR_COMMAND_CANCEL_MORPH` | 37 | No fields. |
| `BWR_COMMAND_CANCEL_RESEARCH` | 38 | No fields. |
| `BWR_COMMAND_CANCEL_UPGRADE` | 39 | No fields. |
| `BWR_COMMAND_USE_TECH` | 40 | type_id, a tech without a target: 0 Stim Packs, 5 Tank Siege Mode, 9 Personnel Cloaking, 10 Cloaking Field, 11 Burrowing. |
| `BWR_COMMAND_USE_TECH_POSITION` | 41 | type_id (tech type), x, y. |
| `BWR_COMMAND_USE_TECH_UNIT` | 42 | type_id (tech type), target. Archon Warp (23) and Dark Archon Meld (28) may leave target 0 to merge the acting units. |
| `BWR_COMMAND_PLACE_COP` | 43 | x, y the top-left build tile. One unit. |

### Game commands

Kinds of the commands to the game itself, after the unit kinds. They name no units (`unit_count` 0, `target` 0); text is UTF-8 of at most 191 bytes.

| Name | Value | Meaning |
|---|---|---|
| `BWR_COMMAND_PAUSE` | 100 | Pauses the game. |
| `BWR_COMMAND_RESUME` | 101 | Resumes a paused game. |
| `BWR_COMMAND_RESTART` | 102 | Restarts the match. |
| `BWR_COMMAND_LEAVE` | 103 | Leaves the match. |
| `BWR_COMMAND_SET_SPEED` | 104 | x, the frame delay in ms for every game speed; negative restores the game's own delays. |
| `BWR_COMMAND_SET_FRAME_SKIP` | 105 | x, logical frames per screen update, at least 1. |
| `BWR_COMMAND_PRINT_TEXT` | 106 | text, a line shown in the game's message area. |
| `BWR_COMMAND_SEND_TEXT` | 107 | text, a chat line from the local player; type_id 1 sends it to allies only, 0 to everyone. |
| `BWR_COMMAND_SET_SCREEN_POSITION` | 108 | x, y, the map pixels of the screen's top-left corner. |
| `BWR_COMMAND_SET_ALLIES` | 109 | x, a player id 0..7; y, 0 not allied, 1 allied, 2 allied victory. Matches only, never the local player. |
| `BWR_COMMAND_SET_VISION` | 110 | x, a player id 0..7; y, 1 shares the local player's vision with it, 0 stops. In a replay it shows or hides that player's vision instead. |
| `BWR_COMMAND_PING_MINIMAP` | 111 | x, y, the map pixels of the ping. |
| `BWR_COMMAND_SET_REVEAL_ALL` | 112 | x, 1 reveals the whole map of a replay, 0 restores its vision. Replays only. |
| `BWR_COMMAND_SET_MAP` | 113 | text, the map file the next restart loads; also outside a match. |
| `BWR_COMMAND_SET_GUI` | 114 | x, 0 stops the game's screen updates, 1 lets them run again. |
| `BWR_COMMAND_USE_CHEAT` | 115 | x, the whole set of the game's cheat flags to hold; single-player matches only. |

### `BwrResult`

What became of a submitted command. `actions` and `reserved` are 0.

| Field | Type | Meaning |
|---|---|---|
| `id` | `uint64_t` | The command id. |
| `state` | `uint32_t` | `BWR_RESULT_*`. |
| `frame_hint` | `uint32_t` | Game frame of the frame the command was submitted against, normally the submitter's last acquired one; 0 when the result was set before the command reached the game thread. |
| `actions` | `uint32_t` | 0. |
| `reserved` | `uint32_t` | 0. |

### Result states

`BwrResult.state` values.

| Name | Value | Meaning |
|---|---|---|
| `BWR_RESULT_PENDING` | 0 | Not executed yet: queued for the next frame boundary, or an id older than the last 1024. |
| `BWR_RESULT_OK` | 1 | The game's command functions ran. Whether the order changes the game shows in later frames. |
| `BWR_RESULT_REJECTED` | 2 | The command does not fit its kind (a missing or out-of-range field, more than 24 units, a kind that cannot queue, a type the kind cannot use), or the game refuses it here (cheats in a network match or replay, alliances in a replay or with oneself, reveal-all outside a replay, a map file that cannot be opened). |
| `BWR_RESULT_UNAVAILABLE` | 3 | Nothing could execute it: no match ran at the frame boundary (or a `LEAVE` before it at the same boundary ended the match), the game build has no command functions, the queue stayed full for 250 ms, or the id is 0. |
| `BWR_RESULT_NOT_EXECUTED` | 4 | A named unit is no longer the unit the caller saw (it died, or its slot holds a new unit), or a named player is not in the game; no part of the command ran. |
| `BWR_RESULT_PANIC` | 5 | Reserved. |
| `BWR_RESULT_UNKNOWN` | 6 | Reserved. |

## Chat

### `BwrChat`

One chat line from `bwr_next_chat`: NUL-terminated UTF-8, cut to fit.

| Field | Type | Meaning |
|---|---|---|
| `kind` | `uint32_t` | `BWR_CHAT_*`; 0 when no line waits. |
| `player` | `uint32_t` | The game player number a received line came from; meaningless for a sent line. |
| `text` | `char[256]` | NUL-terminated UTF-8. |

### Chat kinds

`BwrChat.kind` values; 0 is no line.

| Name | Value | Meaning |
|---|---|---|
| `BWR_CHAT_SENT` | 1 | A line the local player typed, kept instead of sent. |
| `BWR_CHAT_RECEIVED` | 2 | A line the game recorded as received. |

## Drawing

### `BwrShape`

One shape of `BwrDrawList`: BWAPI's shape, with its corners, radii and third triangle corner in `x1`..`extra2` as BWAPI keeps them, its color, and for text the size and the UTF-8 bytes `text_offset .. text_offset + text_len` of the list's text.

| Field | Type | Meaning |
|---|---|---|
| `kind` | `uint8_t` | `BWR_SHAPE_*`. |
| `coordinates` | `uint8_t` | `BWR_COORDINATES_*`. |
| `solid` | `uint8_t` | 1 fills a box, triangle, circle or ellipse; 0 draws its outline. |
| `text_size` | `uint8_t` | Text size, 0 small .. 3 huge. |
| `x1` | `int32_t` | First corner, center or position x. |
| `y1` | `int32_t` | First corner, center or position y. |
| `x2` | `int32_t` | Second corner x of a box, triangle or line. |
| `y2` | `int32_t` | Second corner y of a box, triangle or line. |
| `extra1` | `int32_t` | Third triangle corner x, or the x radius of a circle or ellipse. |
| `extra2` | `int32_t` | Third triangle corner y, or the y radius of an ellipse. |
| `rgba` | `uint32_t` | Color bytes R, G, B, A in memory order. |
| `text_offset` | `uint32_t` | First byte of a text shape's UTF-8 in the list's `text`. |
| `text_len` | `uint32_t` | Bytes of a text shape's UTF-8. |

### Shape kinds

`BwrShape.kind` values, BWAPI's shape types.

| Name | Value | Meaning |
|---|---|---|
| `BWR_SHAPE_TEXT` | 1 | Text at (`x1`, `y1`). |
| `BWR_SHAPE_BOX` | 2 | Rectangle from (`x1`, `y1`) to (`x2`, `y2`). |
| `BWR_SHAPE_TRIANGLE` | 3 | Corners (`x1`, `y1`), (`x2`, `y2`), (`extra1`, `extra2`). |
| `BWR_SHAPE_CIRCLE` | 4 | Center (`x1`, `y1`), radius `extra1`. |
| `BWR_SHAPE_ELLIPSE` | 5 | Center (`x1`, `y1`), radii `extra1` (x) and `extra2` (y). |
| `BWR_SHAPE_DOT` | 6 | One pixel at (`x1`, `y1`). |
| `BWR_SHAPE_LINE` | 7 | From (`x1`, `y1`) to (`x2`, `y2`). |

### Coordinates

`BwrShape.coordinates` values, BWAPI's coordinate types.

| Name | Value | Meaning |
|---|---|---|
| `BWR_COORDINATES_SCREEN` | 1 | Pixels from the top-left of the game screen. |
| `BWR_COORDINATES_MAP` | 2 | Map pixels. |
| `BWR_COORDINATES_MOUSE` | 3 | Pixels from the cursor. |

### `BwrDrawList`

The shapes `bwr_draw` draws, in drawing order, their text, and the RGBA of each text color code (BWAPI's Text codes; a text starts in code 2).

| Field | Type | Meaning |
|---|---|---|
| `size` | `uint32_t` | `sizeof(BwrDrawList)`. |
| `count` | `uint32_t` | Shapes at `shapes`. |
| `shapes` | `const BwrShape*` | The shapes, in drawing order. |
| `text` | `const char*` | UTF-8 bytes the text shapes point into; need not be NUL-terminated. |
| `text_bytes` | `uint32_t` | Bytes at `text`. |
| `text_colors` | `uint32_t[32]` | RGBA of each text color code. |
