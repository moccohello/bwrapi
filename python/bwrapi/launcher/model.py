from __future__ import annotations

import json
import os
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import Mapping


class LauncherConfigError(ValueError):
    """Raised when the product list, a product's files or a launcher step is wrong."""


_REPO_ROOT = Path(__file__).resolve().parents[3]
PROFILES_PATH = _REPO_ROOT / "profiles" / "compatibility-profiles.json"
_DEFAULT_PROFILE = "bwapi-4.4.0-win32-binary"
_ENTRYPOINTS = {"aimodule", "client", "tournament"}
_MODES = {"sync", "async"}
# Game bit widths. The in-game server and the bwrapi.dll beside it must match the game's width.
ARCHES = ("x64", "x86")


@dataclass(frozen=True)
class ProductConfig:
    """One BWAPI 4.4 product: its entrypoint, module, loader and the servers it is injected with."""

    product_id: str
    name: str
    profile: str
    directory: Path
    module: Path | None
    server_x64: Path
    server_x86: Path
    loader: Path | None
    required_files: tuple[Path, ...] = ()
    process_id: int | None = None
    entrypoint: str = "aimodule"
    tournament_module: Path | None = None
    mode: str = "sync"

    def bundle_diagnostics(self) -> list[dict[str, str]]:
        diagnostics: list[dict[str, str]] = []

        def require_file(path: Path | None, label: str, suffix: str) -> None:
            if path is None:
                diagnostics.append({"level": "error", "reason": f"{label} path is missing"})
            elif not path.is_file():
                diagnostics.append({"level": "error", "reason": f"{label} not found: {path}"})
            elif path.suffix.lower() != suffix:
                diagnostics.append({"level": "error", "reason": f"{label} must be a {suffix} file: {path}"})

        for arch in ARCHES:
            require_file(self.server(arch), f"{arch} in-game BWAPI server", ".dll")
            if not self.bwrapi_dll(arch).is_file():
                diagnostics.append({"level": "error",
                                    "reason": f"bwrapi.dll not found beside the {arch} server: {self.bwrapi_dll(arch)}"})
        if self.entrypoint == "aimodule":
            require_file(self.module, "AI module", ".dll")
            require_file(self.loader, "x86 loader", ".exe")
        elif self.entrypoint == "tournament":
            require_file(self.module, "AI module", ".dll")
            require_file(self.loader, "x86 tournament loader", ".exe")
            require_file(self.tournament_module, "TournamentModule", ".dll")
        for path in self.required_files:
            if not path.is_file():
                diagnostics.append({"level": "error", "reason": f"required product file not found: {path}"})
        return diagnostics

    def server(self, arch: str) -> Path:
        """The server injected into a game of bit width ``arch``."""
        return self.server_x86 if arch == "x86" else self.server_x64

    def bwrapi_dll(self, arch: str) -> Path:
        """The ABI DLL the server uses; it goes into the game before the server."""
        return self.server(arch).parent / "bwrapi.dll"

    def log_file(self, arch: str) -> Path:
        """The log the ``arch`` server appends to (``client connected mode=...`` and errors)."""
        return self.server(arch).parent / "bwrapi_bwapi.log"

    def mode_file(self, arch: str) -> Path:
        """The sync/async choice the server reads on every client connection."""
        return self.server(arch).parent / "bwrapi_bwapi.mode"

    @property
    def loader_command(self) -> tuple[str, ...] | None:
        if self.entrypoint == "client":
            return None
        assert self.loader is not None and self.module is not None
        command = [str(self.loader), str(self.module)]
        if self.entrypoint == "tournament":
            assert self.tournament_module is not None
            command.append(str(self.tournament_module))
        return tuple(command)


def load_products(config_path: Path | None) -> dict[str, ProductConfig]:
    """Reads a product list JSON (``schema_version`` 1); no path means no products."""
    if config_path is None:
        return {}
    path = config_path.resolve()
    try:
        raw = json.loads(path.read_text(encoding="utf-8-sig"))
    except OSError as exc:
        raise LauncherConfigError(f"cannot read the product list: {path}") from exc
    except json.JSONDecodeError as exc:
        raise LauncherConfigError(f"invalid product list JSON: {exc}") from exc
    if not isinstance(raw, dict) or raw.get("schema_version") != 1:
        raise LauncherConfigError("schema_version must be 1")
    records = raw.get("products")
    if not isinstance(records, list):
        raise LauncherConfigError("products must be an array")
    products: dict[str, ProductConfig] = {}
    for index, record in enumerate(records):
        product = _parse_product(record, path.parent, index)
        if product.product_id in products:
            raise LauncherConfigError(f"duplicate product_id: {product.product_id}")
        products[product.product_id] = product
    return products


def load_profile_registry(path: Path = PROFILES_PATH) -> list[dict[str, object]]:
    try:
        raw = json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        # An installed wheel has no repository profiles; continue without them.
        return []
    except (OSError, json.JSONDecodeError) as exc:
        raise LauncherConfigError(f"cannot read the compatibility profiles: {path}") from exc
    records = raw.get("profiles") if isinstance(raw, dict) else None
    if not isinstance(records, list) or not all(isinstance(item, dict) for item in records):
        raise LauncherConfigError(f"invalid compatibility profile format: {path}")
    return [item for item in records if isinstance(item, dict)]


def profile_by_id(profile_id: str, path: Path = PROFILES_PATH) -> Mapping[str, object] | None:
    return next((record for record in load_profile_registry(path) if record.get("profile_id") == profile_id), None)


def product_from_module(module: Path, *, process_id: int | None = None) -> ProductConfig:
    """An AIModule product for a module DLL, with the default servers and loader."""
    path = module.resolve()
    return _parse_product(
        {"product_id": path.stem or "bwapi-ai", "module": str(path), "process_id": process_id},
        path.parent,
        0,
    )


def client_product(*, process_id: int | None = None, mode: str = "sync") -> ProductConfig:
    """A client product: only the server goes into the game, and a separate process
    (such as a Python bot) connects to it."""
    record: dict[str, object] = {"product_id": "client", "entrypoint": "client", "mode": mode}
    if process_id is not None:
        record["process_id"] = process_id
    return _parse_product(record, Path.cwd(), 0)


def _parse_product(record: object, base: Path, index: int) -> ProductConfig:
    if not isinstance(record, dict):
        raise LauncherConfigError(f"products[{index}] must be an object")
    allowed = {"product_id", "name", "profile", "entrypoint", "directory", "module", "server_x64", "server_x86",
               "loader", "tournament_module", "required_files", "process_id", "mode"}
    unknown = set(record) - allowed
    if unknown:
        raise LauncherConfigError(f"unknown fields in products[{index}]: {sorted(unknown)}")
    product_id = _text(record.get("product_id"), f"products[{index}].product_id")
    entrypoint = _optional_text(record.get("entrypoint"), "aimodule", f"products[{index}].entrypoint")
    if entrypoint not in _ENTRYPOINTS:
        raise LauncherConfigError(
            f"products[{index}].entrypoint must be one of aimodule, client, tournament"
        )
    mode = _optional_text(record.get("mode"), "sync", f"products[{index}].mode")
    if mode not in _MODES:
        raise LauncherConfigError(f"products[{index}].mode must be sync or async")
    module_value = record.get("module")
    if entrypoint == "client":
        if module_value is not None or record.get("loader") is not None:
            raise LauncherConfigError(f"products[{index}]: the client entrypoint takes no module or loader")
        module = None
    else:
        module = _path(module_value, base, f"products[{index}].module")
    directory = (_path(record.get("directory"), base, f"products[{index}].directory")
                 if record.get("directory") else module.parent if module is not None else base.resolve())
    name = _optional_text(record.get("name"), product_id, f"products[{index}].name")
    profile = _optional_text(record.get("profile"), _DEFAULT_PROFILE, f"products[{index}].profile")
    required = _strings(record.get("required_files", []), f"products[{index}].required_files")

    def artifact(field: str, default: Path) -> Path:
        value = record.get(field)
        return _path(value, directory, f"products[{index}].{field}") if value is not None else default

    default_x64, default_x86, default_loader, default_tournament_loader = _default_artifacts(entrypoint)
    server_x64 = artifact("server_x64", default_x64)
    server_x86 = artifact("server_x86", default_x86)
    if entrypoint == "aimodule":
        loader = artifact("loader", default_loader)
    elif entrypoint == "tournament":
        if record.get("loader") is not None:
            raise LauncherConfigError(f"products[{index}]: the tournament entrypoint selects its own loader; remove loader")
        loader = default_tournament_loader
    else:
        loader = None
    tournament_value = record.get("tournament_module")
    if entrypoint == "tournament":
        tournament_module = _path(tournament_value, directory, f"products[{index}].tournament_module")
    else:
        if tournament_value is not None:
            raise LauncherConfigError(
                f"products[{index}].tournament_module is only valid with the tournament entrypoint"
            )
        tournament_module = None
    process_id = record.get("process_id")
    if process_id is not None and (not isinstance(process_id, int) or isinstance(process_id, bool) or process_id <= 0):
        raise LauncherConfigError(f"products[{index}].process_id must be a positive integer")
    return ProductConfig(
        product_id=product_id,
        name=name,
        profile=profile,
        directory=directory,
        module=module,
        server_x64=server_x64,
        server_x86=server_x86,
        loader=loader,
        required_files=tuple(
            _path(item, directory, f"products[{index}].required_files") for item in required
        ),
        process_id=process_id,
        entrypoint=entrypoint,
        tournament_module=tournament_module,
        mode=mode,
    )


def _default_artifacts(entrypoint: str) -> tuple[Path, Path, Path, Path]:
    """Resolve one coherent x64 server, x86 server and loader set."""

    def build(x64: Path, x86: Path) -> tuple[Path, Path, Path, Path]:
        # VS CMake layout: the server in Release of each width's configure folder, the loaders in the x64 folder.
        return (
            x64 / "Release" / "bwrapi_bwapi.dll",
            x86 / "Release" / "bwrapi_bwapi.dll",
            x64 / "legacy44-loader" / "bwrapi-legacy44-loader.exe",
            x64 / "legacy44-tournament-loader" / "bwrapi-legacy44-tournament-loader.exe",
        )

    # cargo xtask build (<repo root>/build) and tools/build-windows-host.ps1 (%LOCALAPPDATA%) use the
    # VS CMake layout; an installed package's bwrapi/bin has one folder per width (cargo xtask package).
    bin_dir = Path(__file__).resolve().parents[1] / "bin"
    local_app_data = Path(os.environ.get("LOCALAPPDATA", tempfile.gettempdir()))
    local_build = local_app_data / "BWRAPI" / "build"
    candidates = (
        build(_REPO_ROOT / "build" / "host", _REPO_ROOT / "build" / "host-x86"),
        (
            bin_dir / "x64" / "bwrapi_bwapi.dll",
            bin_dir / "x86" / "bwrapi_bwapi.dll",
            bin_dir / "x86" / "bwrapi-legacy44-loader.exe",
            bin_dir / "x86" / "bwrapi-legacy44-tournament-loader.exe",
        ),
        build(local_build / "bwrapi-bwapi-x64", local_build / "bwrapi-bwapi-Win32"),
    )
    for server_x64, server_x86, loader, tournament_loader in candidates:
        required_loader = tournament_loader if entrypoint == "tournament" else loader
        if (server_x64.is_file() and server_x86.is_file()
                and (entrypoint == "client" or required_loader.is_file())):
            return server_x64, server_x86, loader, tournament_loader
    return candidates[0]


def _strings(value: object, label: str) -> list[str]:
    if not isinstance(value, list) or not all(isinstance(item, str) for item in value):
        raise LauncherConfigError(f"{label} must be an array of strings")
    return value


def _text(value: object, label: str) -> str:
    if not isinstance(value, str) or not value.strip():
        raise LauncherConfigError(f"{label} must be a non-empty string")
    return value


def _optional_text(value: object, default: str, label: str) -> str:
    return default if value is None else _text(value, label)


def _path(value: object, base: Path, label: str) -> Path:
    text = _text(value, label)
    path = Path(text)
    return path.resolve() if path.is_absolute() else (base / path).resolve()
