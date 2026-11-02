from __future__ import annotations

import json
from collections.abc import Mapping
from dataclasses import dataclass
from importlib.resources import files
from operator import index
from types import MappingProxyType
from typing import Any


STATIC_DATA_FILE = "static_data_scalar.json"
UNIT_TYPE_HELPER_FILE = "unit_type_helpers.json"
STATIC_DATA_KINDS = ("UnitType", "WeaponType", "TechType", "UpgradeType")


@dataclass(frozen=True, slots=True)
class BwapiStaticRecord:
    """Scalar initial values of BWAPI 4.4."""

    kind: str
    id: int
    symbol: str
    fields: Mapping[str, int]
    diff_bucket: str

    def get(self, field: str, default: int | None = None) -> int | None:
        return self.fields.get(field, default)


@dataclass(frozen=True, slots=True)
class BwapiUnitTypeHelperRecord:
    """Initial values of the BWAPI 4.4 UnitType helpers."""

    id: int
    symbol: str
    fields: Mapping[str, int | bool]

    def get(self, field: str, default: int | bool | None = None) -> int | bool | None:
        return self.fields.get(field, default)


def static_record(kind: str, type_id: int) -> BwapiStaticRecord | None:
    records = BWAPI_STATIC_DATA_BY_KIND.get(kind)
    if records is None:
        return None
    return records.get(int(type_id))


def static_field(
    kind: str,
    type_id: int,
    field: str,
    default: int | None = None,
) -> int | None:
    record = static_record(kind, type_id)
    if record is None:
        return default
    return record.get(field, default)


def unit_type_static_data(type_id: int) -> BwapiStaticRecord | None:
    return static_record("UnitType", type_id)


def weapon_type_static_data(type_id: int) -> BwapiStaticRecord | None:
    return static_record("WeaponType", type_id)


def tech_type_static_data(type_id: int) -> BwapiStaticRecord | None:
    return static_record("TechType", type_id)


def upgrade_type_static_data(type_id: int) -> BwapiStaticRecord | None:
    return static_record("UpgradeType", type_id)


def tech_research_time(tech_type_id: int) -> int | None:
    return static_field("TechType", tech_type_id, "researchTime")


def tech_mineral_price(tech_type_id: int) -> int | None:
    return static_field("TechType", tech_type_id, "mineralPrice")


def tech_gas_price(tech_type_id: int) -> int | None:
    return static_field("TechType", tech_type_id, "gasPrice")


def tech_energy_cost(tech_type_id: int) -> int | None:
    return static_field("TechType", tech_type_id, "energyUsed")


def upgrade_time(upgrade_type_id: int, level: int = 1) -> int | None:
    if isinstance(level, bool) or level < 1:
        raise ValueError("upgrade level must be a positive integer")
    base = static_field("UpgradeType", upgrade_type_id, "upgradeTimeBase")
    factor = static_field("UpgradeType", upgrade_type_id, "upgradeTimeFactor")
    if base is None or factor is None:
        return None
    return base + factor * (int(level) - 1)


def upgrade_gas_price(upgrade_type_id: int, level: int = 1) -> int | None:
    level = index(level)
    base = static_field("UpgradeType", upgrade_type_id, "gasPriceBase")
    factor = static_field("UpgradeType", upgrade_type_id, "gasPriceFactor")
    if base is None or factor is None:
        return None
    return base + factor * max(0, level - 1)


def unit_type_helper_data(type_id: int) -> BwapiUnitTypeHelperRecord | None:
    return BWAPI_UNIT_TYPE_HELPERS.get(int(type_id))


def unit_type_helper_field(
    type_id: int,
    field: str,
    default: int | bool | None = None,
) -> int | bool | None:
    record = unit_type_helper_data(type_id)
    if record is None:
        return default
    return record.get(field, default)


def _load_payload() -> dict[str, Any]:
    data_path = files(__package__).joinpath(STATIC_DATA_FILE)
    return json.loads(data_path.read_text(encoding="utf-8"))


def _load_unit_type_helper_payload() -> dict[str, Any]:
    data_path = files(__package__).joinpath(UNIT_TYPE_HELPER_FILE)
    return json.loads(data_path.read_text(encoding="utf-8"))


def _load_records(
    payload: dict[str, Any],
) -> Mapping[str, Mapping[int, BwapiStaticRecord]]:
    by_kind: dict[str, Mapping[int, BwapiStaticRecord]] = {}
    for kind in STATIC_DATA_KINDS:
        records = {}
        for raw in payload["records"].get(kind, []):
            record = BwapiStaticRecord(
                kind=kind,
                id=int(raw["id"]),
                symbol=str(raw["symbol"]),
                fields=MappingProxyType(
                    {field: int(value) for field, value in raw["fields"].items()}
                ),
                diff_bucket=str(raw["diff_bucket"]),
            )
            records[record.id] = record
        by_kind[kind] = MappingProxyType(records)
    return MappingProxyType(by_kind)


def _load_unit_type_helpers(
    payload: dict[str, Any],
) -> Mapping[int, BwapiUnitTypeHelperRecord]:
    records = {}
    for raw in payload["records"]:
        record = BwapiUnitTypeHelperRecord(
            id=int(raw["id"]),
            symbol=str(raw["symbol"]),
            fields=MappingProxyType(dict(raw["fields"])),
        )
        records[record.id] = record
    return MappingProxyType(records)


def _freeze_json(value: Any) -> Any:
    if isinstance(value, dict):
        return MappingProxyType(
            {key: _freeze_json(nested) for key, nested in value.items()}
        )
    if isinstance(value, list):
        return tuple(_freeze_json(item) for item in value)
    return value


_PAYLOAD = _load_payload()
_UNIT_TYPE_HELPER_PAYLOAD = _load_unit_type_helper_payload()
STATIC_DATA_PROVENANCE = _freeze_json(_PAYLOAD["provenance"])
BWAPI_STATIC_DATA_BY_KIND = _load_records(_PAYLOAD)
UNIT_TYPE_HELPER_PROVENANCE = _freeze_json(_UNIT_TYPE_HELPER_PAYLOAD["provenance"])
BWAPI_UNIT_TYPE_HELPERS = _load_unit_type_helpers(_UNIT_TYPE_HELPER_PAYLOAD)


__all__ = [
    "BWAPI_STATIC_DATA_BY_KIND",
    "BWAPI_UNIT_TYPE_HELPERS",
    "STATIC_DATA_KINDS",
    "STATIC_DATA_PROVENANCE",
    "BwapiStaticRecord",
    "BwapiUnitTypeHelperRecord",
    "UNIT_TYPE_HELPER_PROVENANCE",
    "static_field",
    "static_record",
    "tech_energy_cost",
    "tech_gas_price",
    "tech_mineral_price",
    "tech_type_static_data",
    "tech_research_time",
    "unit_type_helper_data",
    "unit_type_helper_field",
    "unit_type_static_data",
    "upgrade_gas_price",
    "upgrade_time",
    "upgrade_type_static_data",
    "weapon_type_static_data",
]
