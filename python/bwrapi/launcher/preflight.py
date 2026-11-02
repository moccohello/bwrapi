"""Generic BWAPI 4.4 launcher correctness checks."""

from __future__ import annotations

from dataclasses import dataclass

from .model import LauncherConfigError, ProductConfig, profile_by_id


@dataclass(frozen=True)
class PreflightResult:
    product_id: str | None
    profile: str | None
    ok: bool
    checks: tuple[dict[str, object], ...]

    def to_dict(self) -> dict[str, object]:
        return {"product_id": self.product_id, "profile": self.profile, "ok": self.ok,
                "checks": list(self.checks)}


class PreflightError(LauncherConfigError):
    """A failed preflight; carries the same result the command line prints."""

    def __init__(self, result: PreflightResult):
        self.result = result
        failed = next((item["reason"] for item in result.checks if not item["ok"]), "preflight failed")
        super().__init__(str(failed))


def inspect_product(product: ProductConfig) -> PreflightResult:
    """Checks that the files ``product`` needs exist (the servers, bwrapi.dll, module, loader)."""
    checks: list[dict[str, object]] = []

    def check(ok: bool, reason: str, code: str) -> None:
        checks.append({"ok": ok, "code": code, "reason": reason})

    diagnostics = product.bundle_diagnostics()
    for item in diagnostics:
        check(item["level"] != "error", item["reason"], "bundle")
    if not diagnostics:
        check(True, "required files found", "bundle")

    # The profile is recorded for information only; it never blocks a run.
    profile = profile_by_id(product.profile)
    if profile is None:
        check(True, f"compatibility profile {product.profile}: not registered", "profile_status")
    else:
        check(True, f"compatibility profile {product.profile}: {profile.get('status')}", "profile_status")

    return PreflightResult(product.product_id, product.profile,
                           all(bool(item["ok"]) for item in checks), tuple(checks))
