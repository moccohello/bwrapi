"""The generated stubs match what the package really exposes."""

import subprocess
import sys
from pathlib import Path


def test_generated_stubs_are_current():
    python_root = Path(__file__).resolve().parents[2]
    result = subprocess.run(
        [sys.executable, str(python_root / "tests" / "generate_stubs.py"), "--check"],
        cwd=python_root,
        capture_output=True,
        text=True,
    )
    assert result.returncode == 0, result.stdout + result.stderr
