"""`python -m bwrapi.bwapi.run mybot.py`: run a Python AIModule against the in-game BWAPI server.

The bot file defines an `AIModule` subclass (`Bot`, or the only subclass in the
file). The command connects to the server in a running game and calls the
module until the game closes. `--inject` first puts the server shipped with
this package into the running game through the launcher; without it the
server must already be in the game.
"""

from __future__ import annotations

import argparse
import asyncio
import importlib.util
import math
import sys
from pathlib import Path

from .._console import make_output_safe
from .aimodule import AIModule
from .client import ServerConnectionError, connect
from .runner import BwapiModuleRunner



def load_module(path: Path, class_name: str | None = None) -> AIModule:
    """Imports the bot file at ``path`` and returns an instance of its AIModule class
    (``class_name``, else ``Bot``, else the file's only AIModule subclass)."""
    if not path.is_file():
        raise SystemExit(f"bot file not found: {path}")
    spec = importlib.util.spec_from_file_location(path.stem, path)
    if spec is None or spec.loader is None:
        raise SystemExit(f"cannot read the bot file: {path}")
    source = importlib.util.module_from_spec(spec)
    sys.modules[path.stem] = source
    spec.loader.exec_module(source)
    if class_name is not None:
        bot = getattr(source, class_name, None)
        if not (isinstance(bot, type) and issubclass(bot, AIModule)):
            raise SystemExit(f"class {class_name} not found in {path}, or it is not an AIModule subclass")
        return bot()
    candidates = [value for value in vars(source).values()
                  if isinstance(value, type) and issubclass(value, AIModule) and value is not AIModule
                  and value.__module__ == source.__name__]
    bot = getattr(source, "Bot", None) or (candidates[0] if len(candidates) == 1 else None)
    if isinstance(bot, type) and issubclass(bot, AIModule):
        return bot()
    if candidates:
        names = ", ".join(value.__name__ for value in candidates)
        raise SystemExit(f"several AIModule subclasses in {path} ({names}): pick one with --class")
    raise SystemExit(f"no AIModule subclass found in {path}: define one (for example class Bot(AIModule))")


def _inject(process_id: int | None, mode: str) -> None:
    from bwrapi.launcher.model import LauncherConfigError, client_product
    from bwrapi.launcher.process import run_product

    def report(event: dict[str, object]) -> None:
        if event.get("phase") == "Injected":
            print(f"BWAPI server is in StarCraft pid={event.get('pid')} ({event.get('arch')}); "
                  f"server log: {event.get('log')}", file=sys.stderr, flush=True)

    product = client_product(process_id=process_id, mode=mode)
    try:
        run_product(product, on_status=report)
    except LauncherConfigError as error:
        raise SystemExit(str(error)) from error


def main(argv: list[str] | None = None) -> int:
    """Runs the command line; returns the process exit code."""
    make_output_safe()
    parser = argparse.ArgumentParser(prog="python -m bwrapi.bwapi.run",
                                     description="Run a Python AIModule against the in-game BWAPI server.")
    parser.add_argument("bot", type=Path, help="Python file that defines an AIModule subclass")
    parser.add_argument("--class", dest="class_name",
                        help="bot class name (default: Bot, or the file's only AIModule subclass)")
    parser.add_argument("--process-id", type=int,
                        help="PID of the game (server) to connect to; default: picked from the BWAPI game table")
    parser.add_argument("--wait", type=float, default=math.inf,
                        help="seconds to wait for a server (default: forever)")
    parser.add_argument("--inject", action="store_true",
                        help="inject the BWAPI server into the running game first (through the launcher)")
    parser.add_argument("--mode", choices=("sync", "async"), default="sync",
                        help="server mode with --inject: sync makes the game wait for the bot, async does not")
    parser.add_argument("--asyncio", action="store_true", help="run the bot on an asyncio event loop")
    args = parser.parse_args(argv)

    module = load_module(args.bot.resolve(), args.class_name)
    if args.inject:
        _inject(args.process_id, args.mode)
    hint = "" if args.inject else " (start StarCraft; pass --inject to put the server into it)"
    try:
        client = connect(args.process_id, wait=args.wait,
                         on_wait=lambda message: print(message + hint, file=sys.stderr, flush=True))
    except ServerConnectionError as error:
        print(error, file=sys.stderr)
        return 1
    runner = BwapiModuleRunner(client, module)
    try:
        if args.asyncio:
            asyncio.run(runner.run_async())
        else:
            runner.run()
    except KeyboardInterrupt:
        pass
    finally:
        runner.close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
