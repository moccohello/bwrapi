"""Preflight checks and the launcher for BWAPI 4.4 products (AIModule, Client, Tournament).

`python -m bwrapi.launcher doctor|list|run` is the command line; `run_product`
and `inspect_product` are the same steps as functions.
"""

from .model import LauncherConfigError, ProductConfig, load_products
from .preflight import PreflightResult, inspect_product
from .process import PHASES, run_product

__all__ = [
    "LauncherConfigError",
    "ProductConfig",
    "load_products",
    "PreflightResult",
    "inspect_product",
    "PHASES",
    "run_product",
]
