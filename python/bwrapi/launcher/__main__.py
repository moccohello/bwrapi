"""`python -m bwrapi.launcher`: check, list and run BWAPI 4.4 bots on StarCraft: Remastered."""

from __future__ import annotations

import argparse
import json
import sys
from dataclasses import replace
from pathlib import Path

from .._console import make_output_safe
from .model import ARCHES, LauncherConfigError, client_product, load_products, product_from_module, profile_by_id
from .preflight import PreflightError, inspect_product
from .process import run_product, running_games

SAMPLE_CONFIG = Path(__file__).resolve().parent / "products.sample.json"

_EPILOG = """commands:
  doctor [target]  check the files a bot needs, list running StarCraft processes
                   and show where the server log is (no target: the server only)
  list             list the products of --config
  run <target>     inject the in-game BWAPI server into StarCraft and start the bot

examples:
  python -m bwrapi.launcher doctor
  python -m bwrapi.launcher doctor C:\\Bots\\bot.dll
  python -m bwrapi.launcher run C:\\Bots\\bot.dll --process-id 1234
  python -m bwrapi.launcher --config products.json run my-bot
"""


def main(argv: list[str] | None = None) -> int:
    """Runs the launcher command line; returns the process exit code."""
    make_output_safe()
    parser = argparse.ArgumentParser(
        prog="python -m bwrapi.launcher",
        description="BWAPI 4.4 bot launcher for StarCraft: Remastered.",
        epilog=_EPILOG,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument("command", choices=("doctor", "list", "run"), help="what to do (see commands below)")
    parser.add_argument("target", nargs="?", help="path of a BWAPI AI module DLL, or a product_id from --config")
    parser.add_argument("--config", type=Path,
                        help=f"product list JSON (sample: {SAMPLE_CONFIG.name} in the launcher package)")
    parser.add_argument("--json", dest="machine", action="store_true", help="print the result as JSON")
    parser.add_argument("--log", type=Path,
                        help="JSONL file for run status events (default: <product directory>/<product_id>.run.jsonl)")
    parser.add_argument("--process-id", type=int,
                        help="PID of the StarCraft process to use (default: the only one running)")
    parser.add_argument("--game", type=Path,
                        help="StarCraft.exe for run to start itself, with the legacy unit limits")
    parser.add_argument("--mode", choices=("sync", "async"),
                        help="server mode instead of the product's: sync makes the game wait for the bot, async does not")
    args = parser.parse_args(argv)

    command = args.command
    target = args.target

    try:
        products = load_products(args.config)
        if command == "list":
            if args.config is None and not args.machine:
                print("No products: pass --config <products.json>.")
                print(f"A sample product list is at {SAMPLE_CONFIG}")
                return 0
            rows = []
            for item in products.values():
                profile = profile_by_id(item.profile)
                rows.append({
                    "product_id": item.product_id,
                    "name": item.name,
                    "profile": item.profile,
                    "entrypoint": item.entrypoint,
                    "profile_status": profile.get("status") if profile else None,
                    "launch_role": profile.get("launch_role") if profile else None,
                })
            _print(rows, machine=args.machine)
            return 0

        if args.process_id is not None and args.process_id <= 0:
            raise LauncherConfigError("--process-id must be a positive integer")
        if not target:
            if command != "doctor":
                raise LauncherConfigError(f"{command} needs a target: a module DLL path or a product_id")
            product = client_product(process_id=args.process_id)
        else:
            product = products.get(target)
            if product is None:
                if Path(target).suffix.lower() != ".dll" and not Path(target).is_file():
                    raise LauncherConfigError(
                        f"no product named {target} and no such file (products come from --config <products.json>)")
                product = product_from_module(Path(target), process_id=args.process_id)
        if args.process_id is not None:
            product = replace(product, process_id=args.process_id)
        if args.mode is not None:
            product = replace(product, mode=args.mode)
        if command == "doctor":
            result = inspect_product(product)
            payload = result.to_dict()
            payload["servers"] = {arch: {"server": str(product.server(arch)), "log": str(product.log_file(arch))}
                                  for arch in ARCHES}
            payload["games"] = running_games()
            _print(payload, machine=args.machine)
            return 0 if result.ok else 2
        if command == "run":
            events: list[dict[str, object]] = []

            def on_status(event: dict[str, object]) -> None:
                events.append(event)
                if not args.machine:
                    _print_event(event)

            code = run_product(product, log_path=args.log, game=args.game, on_status=on_status)
            payload = {"product_id": product.product_id, "exit_code": code, "events": events}
            if args.machine:
                _print(payload, machine=True)
            else:
                print(f"{product.name} ({product.profile}) exit code: {code}")
            return code
        raise LauncherConfigError(f"unsupported command: {command}")
    except PreflightError as exc:
        _print(exc.result.to_dict(), machine=args.machine)
        return 2
    except LauncherConfigError as exc:
        if args.machine:
            print(json.dumps({"ok": False, "error": str(exc)}))
        else:
            print(f"launcher error: {exc}", file=sys.stderr)
        return 2


def _product_line(row: dict[str, object]) -> str:
    title = row["product_id"] if row["name"] == row["product_id"] else f"{row['product_id']} ({row['name']})"
    status = f" status={row['profile_status']}" if row["profile_status"] else ""
    return f"{title}: entrypoint={row['entrypoint']} profile={row['profile']}{status}"


def _print_event(event: dict[str, object]) -> None:
    pid = f" pid={event['pid']}" if event.get("pid") is not None else ""
    print(f"[{event.get('phase')}]{pid} {event.get('reason')}", flush=True)
    if event.get("phase") == "Injected" and event.get("log"):
        print(f"  server log: {event['log']}", flush=True)


def _print(payload: object, *, machine: bool) -> None:
    if machine:
        print(json.dumps(payload, indent=2))
        return
    if isinstance(payload, list):
        if not payload:
            print("No products in the product list.")
        for row in payload:
            print(_product_line(row))
        return
    if isinstance(payload, dict):
        if isinstance(payload.get("checks"), list):
            print(f"product={payload.get('product_id')} profile={payload.get('profile')} ok={payload.get('ok')}")
            for check in payload["checks"]:
                if isinstance(check, dict):
                    # A profile never blocks a run, so it reads as information, not a result.
                    mark = "FAIL" if not check.get("ok") else "INFO" if check.get("code") == "profile_status" else "OK"
                    print(f"[{mark}] {check.get('reason')}")
            _print_environment(payload)
            return
        print(json.dumps(payload, indent=2))
    else:
        print(payload)


def _print_environment(payload: dict[str, object]) -> None:
    servers = payload.get("servers")
    if isinstance(servers, dict):
        for arch, paths in servers.items():
            print(f"server {arch}: {paths['server']}")
            print(f"  log: {paths['log']}")
    games = payload.get("games")
    if not isinstance(games, list):
        return
    if not games:
        print("StarCraft: no running process found")
    for game in games:
        arch = game["arch"] or "unknown width (cannot open the process)"
        if not game["server_injected"]:
            state = "no BWAPI server"
        elif game["client_connected"]:
            state = "BWAPI server injected, client connected"
        else:
            state = "BWAPI server injected, no client connected"
        print(f"StarCraft pid={game['pid']} {arch}: {state}")


if __name__ == "__main__":
    raise SystemExit(main())
