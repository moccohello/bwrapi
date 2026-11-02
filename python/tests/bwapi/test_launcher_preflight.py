from __future__ import annotations

import io
import json
import sys
from dataclasses import replace
from pathlib import Path
from types import SimpleNamespace

import pytest

from bwrapi.launcher import __main__ as cli
from bwrapi.launcher import model, preflight, process


@pytest.fixture(autouse=True)
def no_live_games(monkeypatch):
    """The suite never looks at a StarCraft that happens to be running."""
    monkeypatch.setattr(process, "_running_starcraft_pids", lambda: [])


def test_output_survives_a_path_the_console_encoding_lacks(monkeypatch):
    stdout = io.TextIOWrapper(io.BytesIO(), encoding="cp1252")
    monkeypatch.setattr(sys, "stdout", stdout)
    assert cli.main(["doctor", "C:\\\ud55c\uae00\\bot.dll"]) == 2
    stdout.flush()
    assert b"\\ud55c\\uae00" in stdout.buffer.getvalue()


@pytest.fixture
def product(tmp_path):
    for name in ("bot.dll", "loader.exe"):
        (tmp_path / name).write_bytes(name.encode())
    for arch in model.ARCHES:
        (tmp_path / arch).mkdir()
        for name in ("bwrapi_bwapi.dll", "bwrapi.dll"):
            (tmp_path / arch / name).write_bytes(name.encode())
    return model.ProductConfig(
        product_id="test", name="test", profile="bwapi-4.4.0-win32-binary",
        directory=tmp_path, module=tmp_path / "bot.dll",
        server_x64=tmp_path / "x64" / "bwrapi_bwapi.dll", server_x86=tmp_path / "x86" / "bwrapi_bwapi.dll",
        loader=tmp_path / "loader.exe", process_id=123,
    )


def _count_preflight_runs(monkeypatch):
    calls = []
    original = preflight.inspect_product

    def checked(*args, **kwargs):
        calls.append((args, kwargs))
        return original(*args, **kwargs)

    monkeypatch.setattr(preflight, "inspect_product", checked)
    return calls


def test_doctor_accepts_bundle_with_bwrapi_dll_next_to_server(product):
    result = preflight.inspect_product(product)
    assert result.ok, result.to_dict()


@pytest.mark.parametrize("arch", model.ARCHES)
def test_missing_bwrapi_dll_next_to_server_is_rejected(product, tmp_path, arch):
    (tmp_path / arch / "bwrapi.dll").unlink()
    result = preflight.inspect_product(product)
    assert not result.ok
    assert any(item["code"] == "bundle" and not item["ok"] for item in result.checks)


def _mock_runner(monkeypatch, product, arch="x64"):
    monkeypatch.setattr(process, "os", SimpleNamespace(name="nt"))
    monkeypatch.setattr(process, "_game_arch", lambda _pid: arch)
    monkeypatch.setattr(cli, "load_products", lambda _: {product.product_id: product})


@pytest.mark.parametrize("arch", model.ARCHES)
def test_run_injects_server_of_game_width_with_mode_before_starting_loader(product, monkeypatch, capsys, arch):
    _mock_runner(monkeypatch, product, arch)
    calls = _count_preflight_runs(monkeypatch)
    order = []
    monkeypatch.setattr(process, "_inject", lambda item, pid, game_arch, _timeout: order.append(
        ("inject", pid, game_arch, item.mode_file(game_arch).read_text())))

    def spawn(command, **kwargs):
        order.append(("loader", tuple(command)))
        return SimpleNamespace(pid=234, wait=lambda: 0)

    monkeypatch.setattr(process.subprocess, "Popen", spawn)
    assert cli.main(["run", "test", "--json", "--mode", "async"]) == 0
    assert json.loads(capsys.readouterr().out)["exit_code"] == 0
    assert order == [("inject", 123, arch, "async"),
                     ("loader", (str(product.loader), str(product.module)))]
    assert len(calls) == 1
    calls.clear()
    assert process.run_product(product) == 0
    assert len(calls) == 1
    assert product.mode_file(arch).read_text() == "sync"


def test_started_game_reads_legacy_limits_and_user_setting_returns(product, monkeypatch, tmp_path):
    _mock_runner(monkeypatch, product)
    settings = tmp_path / "StarCraft" / "CSettings.json"
    settings.parent.mkdir()
    original = {"other": 1, process.SETTINGS_SECTION: {"kept": 2, process.EXTENDED_LIMITS_SETTING: True}}
    settings.write_text(json.dumps(original), encoding="utf-8")
    monkeypatch.setattr(process, "_documents", lambda: tmp_path)
    seen = []

    def extended() -> bool:
        return json.loads(settings.read_text())[process.SETTINGS_SECTION][process.EXTENDED_LIMITS_SETTING]

    monkeypatch.setattr(process, "_inject", lambda _item, pid, _arch, _timeout: seen.append(("inject", pid, extended())))
    loader = SimpleNamespace(pid=78, poll=lambda: seen.append(("loader", 78, extended())) or 0)
    game = SimpleNamespace(pid=77, poll=lambda: None, wait=lambda: seen.append(("game exit", 77, extended())) or 0)
    monkeypatch.setattr(process.subprocess, "Popen",
                        lambda command, **kwargs: game if "-launch" in command else loader)
    assert process.run_product(product, game=tmp_path / "StarCraft.exe") == 0
    # The game may write its settings while it runs, so they return only after it exits.
    assert seen == [("inject", 77, False), ("loader", 78, False), ("game exit", 77, False)]
    assert json.loads(settings.read_text()) == original


def test_failed_cli_preflight_keeps_result_json_and_direct_api_rejects(product, monkeypatch, tmp_path, capsys):
    _mock_runner(monkeypatch, product)
    (tmp_path / "x86" / "bwrapi.dll").unlink()
    calls = _count_preflight_runs(monkeypatch)

    def forbidden_spawn(*args, **kwargs):
        pytest.fail("failed preflight must not spawn")

    monkeypatch.setattr(process.subprocess, "Popen", forbidden_spawn)
    assert cli.main(["run", "test", "--json"]) == 2
    payload = json.loads(capsys.readouterr().out)
    assert set(payload) == {"product_id", "profile", "ok", "checks"}
    assert payload["ok"] is False and payload["product_id"] == "test"
    assert any(item["code"] == "bundle" and not item["ok"] for item in payload["checks"])
    assert len(calls) == 1
    with pytest.raises(preflight.PreflightError) as failure:
        process.run_product(product)
    assert failure.value.result.to_dict() == payload


@pytest.mark.parametrize("argv", ([], ["--product", "test"], ["--list"],
                                  ["list", "--machine-readable"]))
def test_cli_rejects_unknown_commands_and_options(argv, monkeypatch):
    def forbidden_load(*args):
        pytest.fail("invalid CLI must not load products")

    monkeypatch.setattr(cli, "load_products", forbidden_load)
    with pytest.raises(SystemExit) as failure:
        cli.main(argv)
    assert failure.value.code == 2


def test_a_target_that_is_no_product_and_no_file_says_so(capsys):
    assert cli.main(["run", "nosuchbot"]) == 2
    assert ("no product named nosuchbot and no such file (products come from --config <products.json>)"
            in capsys.readouterr().err)


def test_list_reports_aimodule_client_tournament_sample(capsys):
    config = Path(__file__).resolve().parents[2] / "bwrapi" / "launcher" / "products.sample.json"
    assert cli.main(["--config", str(config), "list", "--json"]) == 0
    rows = json.loads(capsys.readouterr().out)
    assert {item["entrypoint"] for item in rows} == {"aimodule", "client", "tournament"}
    assert all(item["profile"] == "bwapi-4.4.0-win32-binary" for item in rows)


@pytest.mark.parametrize("profile_id,entrypoint", (
    ("missing-profile", "aimodule"),
    ("bwapi-4.4.0-source", "aimodule"),
    ("bwapi-4.4.0-client-exe", "client"),
    ("bwapi-4.4.0-tournament-module", "tournament"),
))
def test_profile_is_reported_but_does_not_gate_doctor(
        product, monkeypatch, capsys, profile_id, entrypoint):
    monkeypatch.setattr(model, "_default_artifacts",
                        lambda _: (product.server_x64, product.server_x86, product.loader, product.loader))
    record = {"product_id": "test", "profile": profile_id, "entrypoint": entrypoint,
              "process_id": product.process_id}
    if entrypoint != "client":
        record["module"] = str(product.module)
    if entrypoint == "tournament":
        record["tournament_module"] = str(product.module)
    config = product.directory / "products.json"
    config.write_text(json.dumps({"schema_version": 1, "products": [record]}), encoding="utf-8")

    loaded = model.load_products(config)["test"]
    assert loaded.profile == profile_id

    cli.main(["--config", str(config), "doctor", "test", "--json"])
    payload = json.loads(capsys.readouterr().out)
    assert payload["profile"] == profile_id
    reports = [item for item in payload["checks"] if item["code"] == "profile_status"]
    assert len(reports) == 1 and reports[0]["ok"] is True
    assert profile_id in reports[0]["reason"]
    if entrypoint == "aimodule":
        assert payload["ok"] is True, payload


def test_doctor_reports_games_and_the_log_beside_the_selected_server(product, monkeypatch, capsys):
    monkeypatch.setattr(cli, "load_products", lambda _: {product.product_id: product})
    monkeypatch.setattr(process, "_running_starcraft_pids", lambda: [11, 22, 33])
    monkeypatch.setattr(process, "_listed_servers", lambda: {11: True, 22: False})

    def arch(pid):
        if pid == 33:
            raise model.LauncherConfigError("access denied")
        return "x86" if pid == 22 else "x64"

    monkeypatch.setattr(process, "_game_arch", arch)
    assert cli.main(["doctor", "test", "--json"]) == 0
    payload = json.loads(capsys.readouterr().out)
    assert payload["games"] == [
        {"pid": 11, "arch": "x64", "server_injected": True, "client_connected": True},
        {"pid": 22, "arch": "x86", "server_injected": True, "client_connected": False},
        {"pid": 33, "arch": None, "server_injected": False, "client_connected": False},
    ]
    assert payload["servers"]["x86"]["log"] == str(product.server_x86.parent / "bwrapi_bwapi.log")
    assert cli.main(["doctor", "test"]) == 0
    printed = capsys.readouterr().out
    assert "[INFO] compatibility profile" in printed and "[OK] required files found" in printed


@pytest.mark.parametrize("entrypoint", ("client", "tournament"))
def test_preflight_accepts_client_and_tournament_entrypoints(product, entrypoint):
    direct = replace(product, entrypoint=entrypoint,
                     module=None if entrypoint == "client" else product.module,
                     loader=None if entrypoint == "client" else product.loader,
                     tournament_module=product.module if entrypoint == "tournament" else None)
    result = preflight.inspect_product(direct)
    assert result.ok, result.to_dict()


@pytest.mark.skipif(process.os.name != "nt", reason="process bit width is read on Windows")
def test_game_arch_reads_this_process_as_its_own_width():
    assert process._game_arch(process.os.getpid()) == process.OWN_ARCH
