# Third-Party Notices

This file lists, by name, version and license, the external components included in the distributions (Python wheel, NuGet package, C++ zip).
The code of this repository is licensed under the root `LICENSE` (MIT).

## BWAPI 4.4 (LGPL-3.0)

- `bwrapi_bwapi.dll` (the in-game BWAPI server) and the x86 loaders (`bwrapi-legacy44-loader.exe`,
  `bwrapi-legacy44-tournament-loader.exe`) are built from and linked with the BWAPI 4.4 source.
  - server: the BWAPI headers and from `BWAPILIB/Source`: `Color.cpp`, `UnitType.cpp`, `TechType.cpp`, `UpgradeType.cpp`,
    `Race.cpp`, `WeaponType.cpp`, `UnitSizeType.cpp`, `DamageType.cpp`, `ExplosionType.cpp`, `Order.cpp`
  - AIModule loader: `BWAPIClient`, `BWAPILIB`, `AIModuleLoader`
  - Tournament loader: `BWAPIClient`, `BWAPILIB`, with this repository's own loader sources
- BWAPI repository: https://github.com/bwapi/bwapi
- Source used (pinned commit): https://github.com/bwapi/bwapi/tree/d727fed68558c506163048ea889131d8cbb33915
- The loaders are built from a copy of that source modified at build time. The modifications are in this repository's
  `host/cmake/build_legacy44_loader.cmake`, `host/cmake/build_legacy44_tournament_loader.cmake` and
  `host/cmake/stage_legacy44_client.cmake`.
- Full text of the LGPL-3.0: https://www.gnu.org/licenses/lgpl-3.0.txt (GPL-3.0: https://www.gnu.org/licenses/gpl-3.0.txt)
- They can be rebuilt from the same source with the same modifications with `cargo xtask build` in this repository.

## Rust components (bwrapi.dll)

Versions are from `Cargo.lock`; licenses are the expressions each crate declares, copied as is.

MIT OR Apache-2.0:

- windows-sys 0.52.0
- windows-targets 0.52.6
- windows_x86_64_msvc 0.52.6 (x64 `bwrapi.dll`)
- windows_i686_msvc 0.52.6 (x86 `bwrapi.dll`)
