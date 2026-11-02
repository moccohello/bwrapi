"""Compares the ctypes GameData layout with the official BWAPI 4.4 headers field by field.

The headers are compiled with a C++ compiler to get sizeof and offsetof directly. Header
fields are only int, bool, double and unsigned short, so this platform's result matches
the server's (MSVC) layout.
"""

from __future__ import annotations

import ctypes
import shutil
import subprocess

import pytest

import upstream_root
from bwrapi.bwapi import gamedata

_STRUCTS = {
    "BWAPI::GameInstance": gamedata.GameInstance,
    "BWAPI::GameTable": gamedata.GameTable,
    "BWAPI::ForceData": gamedata.ForceData,
    "BWAPI::PlayerData": gamedata.PlayerData,
    "BWAPI::UnitData": gamedata.UnitData,
    "BWAPI::BulletData": gamedata.BulletData,
    "BWAPI::RegionData": gamedata.RegionData,
    "BWAPIC::Position": gamedata.Point,
    "BWAPIC::Event": gamedata.Event,
    "BWAPIC::Shape": gamedata.Shape,
    "BWAPIC::Command": gamedata.Command,
    "BWAPIC::UnitCommand": gamedata.UnitCommand,
    "BWAPI::unitFinder": gamedata.UnitFinder,
    "BWAPI::GameData": gamedata.GameData,
}


def _compiler() -> str | None:
    return shutil.which("g++") or shutil.which("clang++")


def test_game_data_matches_official_headers(tmp_path):
    include = upstream_root.INCLUDE
    if not (include / "BWAPI" / "Client" / "GameData.h").is_file():
        pytest.skip("needs the BWAPI 4.4 upstream headers")
    compiler = _compiler()
    if compiler is None:
        pytest.skip("needs a C++ compiler")
    lines = [
        "#include <cstddef>",
        "#include <cstdio>",
        "#include <BWAPI/Client/GameData.h>",
        "#include <BWAPI/Client/GameTable.h>",
        "int main() {",
    ]
    for cname, struct in _STRUCTS.items():
        lines.append(f'  std::printf("{cname} sizeof %zu\\n", sizeof({cname}));')
        for field, _ in struct._fields_:
            lines.append(f'  std::printf("{cname} {field} %zu\\n", offsetof({cname}, {field}));')
    lines.append("  return 0;\n}")
    source = tmp_path / "layout.cpp"
    source.write_text("\n".join(lines), encoding="utf-8")
    binary = tmp_path / "layout"
    subprocess.run([compiler, "-std=c++17", "-w", f"-I{include}", str(source), "-o", str(binary)],
                   check=True, capture_output=True)
    output = subprocess.run([str(binary)], check=True, capture_output=True, text=True).stdout

    mismatches = []
    for line in output.splitlines():
        cname, field, value = line.split()
        struct = _STRUCTS[cname]
        actual = ctypes.sizeof(struct) if field == "sizeof" else getattr(struct, field).offset
        if actual != int(value):
            mismatches.append(f"{cname}.{field}: header {value}, ctypes {actual}")
    assert not mismatches
