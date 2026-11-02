# Contributing

Issues and pull requests are welcome. Use the issue templates for bugs and bot compatibility reports.

## Build and test

```
cargo xtask build
cargo xtask test
cargo xtask package
```

On Windows these build the in-game DLLs and servers for x64 and x86, run the Rust, C++, Python and .NET tests, and produce the wheel, the NuGet package and the C++ zip under `build\dist`. On Linux, `build` and `test` run the Rust and Python tests; `package` needs Windows. See [README.md](README.md#build-from-source) for the requirements and the BWAPI source option.

Python tests alone (from `python/`): `python -m pytest -q -p no:cacheprovider --bwapi-source=<BWAPI 4.4 source root>`; Windows-only transport tests run with Windows Python.

## Changes

- Keep each pull request to one change with its tests and the documentation it affects.
- Module addresses and the unit, sprite and image layouts live in `crates/layout`, for both x64 and x86; the BWGame record offsets are in `crates/engine/src/decode/bwgame.rs`. New offsets go in `crates/layout`.
- The C ABI header, the .NET and Python declarations and [docs/C-ABI.md](docs/C-ABI.md) are generated from the Rust definitions; change the definitions and regenerate.
- Bot-facing behavior follows the official BWAPI 4.4; differences are listed in [docs/BWAPI-COMPATIBILITY.md](docs/BWAPI-COMPATIBILITY.md).
- Unit tests and builds do not prove in-game behavior; say in the pull request what you checked in a real game.

User documentation is in English. The internal design notes ([docs/ARCHITECTURE.md](docs/ARCHITECTURE.md), `docs/memory/`, [AGENTS.md](AGENTS.md)) are in Korean.
