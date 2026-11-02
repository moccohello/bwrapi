"""Shared pytest setup: the option that locates the BWAPI 4.4 upstream source."""
from __future__ import annotations

import sys
from pathlib import Path

_PACKAGE_ROOT = Path(__file__).resolve().parent
_REPO_ROOT = _PACKAGE_ROOT.parent
_UPSTREAM_TESTS = _PACKAGE_ROOT / "tests" / "bwapi"


def pytest_addoption(parser):
    parser.addoption(
        "--bwapi-source",
        type=Path,
        default=_REPO_ROOT / "build" / "_deps" / "bwapi-src",
        help="BWAPI 4.4 upstream source root (containing bwapi/). Default: <repo root>/build/_deps/bwapi-src",
    )


def pytest_configure(config):
    sys.path.insert(0, str(_UPSTREAM_TESTS))
    import upstream_root

    upstream_root.set_source_root(Path(config.getoption("--bwapi-source")))
