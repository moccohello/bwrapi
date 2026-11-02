"""BWRAPI Python package: the BWAPI 4.4 client (`bwrapi.bwapi`), the ABI 1.0 binding (`bwrapi.native`) and the launcher."""

from bwrapi._generated.version import __version__

__all__ = ["__version__", "bwapi", "launcher", "native"]
