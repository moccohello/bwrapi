"""Console output for the command-line entry points."""

from __future__ import annotations

import sys


def make_output_safe() -> None:
    """Paths and Windows messages can hold characters the console encoding lacks; print them escaped."""
    for stream in (sys.stdout, sys.stderr):
        reconfigure = getattr(stream, "reconfigure", None)
        if reconfigure is not None:
            reconfigure(errors="backslashreplace")
