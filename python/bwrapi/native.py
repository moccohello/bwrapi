"""ABI 1.0 of `bwrapi.dll` for Python: the generated declarations and the loaded library.

`_native` holds the constants, structures and function signatures generated
from the Rust definitions (`cargo xtask package` writes it beside this module).
This module only loads the library and applies those signatures, so the
binding carries no logic of its own:

    from bwrapi import native
    lib = native.load()
    session = ctypes.c_void_p()
    lib.bwr_open(pid, 0, ctypes.byref(session))
    lib.bwr_subscribe(session, native.abi.SOURCE_UNITS)
"""

from __future__ import annotations

import ctypes
from pathlib import Path

try:
    from . import _native as abi
except ImportError as error:  # a source checkout before `cargo xtask package`
    raise ImportError("bwrapi._native is missing: cargo xtask package generates the ABI 1.0 binding") from error

BIN = Path(__file__).resolve().parent / "bin"


def library_path() -> Path:
    """The `bwrapi.dll` of this Python's pointer width the package carries, beside the in-game server of that width."""
    return BIN / ("x64" if ctypes.sizeof(ctypes.c_void_p) == 8 else "x86") / "bwrapi.dll"


def load(path: str | Path | None = None) -> ctypes.CDLL:
    """Loads `bwrapi.dll` (``path``, else `library_path()`) with the ABI 1.0 signatures applied."""
    library = ctypes.CDLL(str(path or library_path()))
    for name, (restype, argtypes) in abi.SIGNATURES.items():
        function = getattr(library, name)
        function.restype = restype
        function.argtypes = argtypes
    return library
