"""Location of the BWAPI 4.4 upstream source.

The default is `<repo root>/build/_deps/bwapi-src` (containing the `bwapi/` source
folder); conftest's `--bwapi-source` option selects another location. Tests that
read the upstream source are skipped when it is missing.
"""
from pathlib import Path

import pytest

REPO = Path(__file__).resolve().parents[3] / "build" / "_deps" / "bwapi-src"
SOURCE = REPO / "bwapi"
INCLUDE = SOURCE / "include"

_SKIP_REASON = "needs the BWAPI 4.4 upstream source (with bwapi/include)"


def _skip_mark(root: Path):
    return pytest.mark.skipif(not (root / "bwapi" / "include").is_dir(),
                              reason=_SKIP_REASON)


requires_upstream = _skip_mark(REPO)


def set_source_root(root: Path) -> None:
    """Lets conftest recompute the module state from the `--bwapi-source` value."""
    global REPO, SOURCE, INCLUDE, requires_upstream
    REPO = Path(root)
    SOURCE = REPO / "bwapi"
    INCLUDE = SOURCE / "include"
    requires_upstream = _skip_mark(REPO)
