import pytest

from bwrapi.bwapi import (
    BWAPI_STATIC_DATA_BY_KIND,
    BWAPI_UNIT_TYPE_HELPERS,
    STATIC_DATA_PROVENANCE,
    TechTypeId,
    UNIT_TYPE_HELPER_PROVENANCE,
    UnitTypeId,
    UpgradeTypeId,
    WeaponTypeId,
    static_field,
    tech_research_time,
    tech_type_static_data,
    unit_type_helper_data,
    unit_type_helper_field,
    unit_type_static_data,
    upgrade_time,
    upgrade_type_static_data,
    weapon_type_static_data,
)


def test_static_data_covers_pinned_official_scalar_tables() -> None:
    assert STATIC_DATA_PROVENANCE["canonical_source"] == "pinned official BWAPI 4.4 BWAPILIB/Source"
    assert {
        kind: len(records)
        for kind, records in BWAPI_STATIC_DATA_BY_KIND.items()
    } == {
        "UnitType": 234,
        "WeaponType": 132,
        "TechType": 47,
        "UpgradeType": 63,
    }
    assert all(
        record.diff_bucket == "official_4_4"
        for records in BWAPI_STATIC_DATA_BY_KIND.values()
        for record in records.values()
    )


def test_unit_type_static_data_uses_pinned_bwapi44_scalar_fields() -> None:
    scv = unit_type_static_data(UnitTypeId.TERRAN_SCV)

    assert scv is not None
    assert scv.id == 7
    assert scv.symbol == "Terran_SCV"
    assert scv.fields["mineralPrice"] == 50
    assert scv.fields["gasPrice"] == 0
    assert scv.fields["buildTime"] == 300
    assert scv.fields["maxHitPoints"] == 60
    assert scv.fields["supplyRequired"] == 2


def test_weapon_tech_and_upgrade_static_data_are_available_by_canonical_id() -> None:
    gauss = weapon_type_static_data(WeaponTypeId.GAUSS_RIFLE)
    nuke = tech_type_static_data(TechTypeId.NUCLEAR_STRIKE)
    unknown = tech_type_static_data(TechTypeId.UNKNOWN)
    shells = upgrade_type_static_data(UpgradeTypeId.U_238_SHELLS)

    assert gauss is not None
    assert gauss.fields["damageAmount"] == 6
    assert gauss.fields["damageCooldown"] == 15
    assert gauss.fields["maxRange"] == 128

    assert nuke is not None
    assert nuke.id == 45
    assert nuke.symbol == "Nuclear_Strike"
    assert not hasattr(nuke, "candidate_source_id")

    assert unknown is not None
    assert unknown.id == 46
    assert unknown.symbol == "Unknown"

    assert shells is not None
    assert shells.fields["mineralPriceBase"] == 150
    assert shells.fields["gasPriceBase"] == 150
    assert shells.fields["upgradeTimeBase"] == 1500


def test_static_data_exposes_tech_and_upgrade_duration_helpers() -> None:
    assert tech_research_time(TechTypeId.STIM_PACKS) == 1200
    assert upgrade_time(UpgradeTypeId.TERRAN_INFANTRY_WEAPONS) == 4000
    assert upgrade_time(UpgradeTypeId.TERRAN_INFANTRY_WEAPONS, level=2) == 4480
    assert upgrade_time(UpgradeTypeId.U_238_SHELLS) == 1500
    assert tech_research_time(99999) is None
    assert upgrade_time(99999) is None

    with pytest.raises(ValueError, match="positive integer"):
        upgrade_time(UpgradeTypeId.TERRAN_INFANTRY_WEAPONS, level=0)


def test_pinned_official_records_replace_legacy_candidate_gaps() -> None:
    assert static_field("UnitType", UnitTypeId.HERO_DARK_TEMPLAR, "supplyRequired") == 2
    assert static_field("UnitType", UnitTypeId.POWERUP_FLAG, "maxHitPoints") == 100000
    assert static_field("UnitType", UnitTypeId.UNKNOWN, "maxHitPoints") == 0
    assert tech_type_static_data(TechTypeId.FEEDBACK).diff_bucket == "official_4_4"
    assert static_field("WeaponType", WeaponTypeId.PLATFORM_LASER_BATTERY, "damageAmount") == 7
    assert static_field("UpgradeType", UpgradeTypeId.UPGRADE_60, "maxRepeats") == 0


def test_static_data_lookup_helpers_are_null_safe() -> None:
    assert unit_type_static_data(99999) is None
    assert static_field("UnitType", UnitTypeId.TERRAN_SCV, "mineralPrice") == 50
    assert static_field("UnitType", UnitTypeId.TERRAN_SCV, "unknown", -1) == -1
    assert static_field("UnitType", 99999, "mineralPrice", -1) == -1
    assert static_field("Nope", UnitTypeId.TERRAN_SCV, "mineralPrice", -1) == -1


def test_static_data_mappings_are_immutable() -> None:
    scv = unit_type_static_data(UnitTypeId.TERRAN_SCV)
    assert scv is not None

    with pytest.raises(TypeError):
        BWAPI_STATIC_DATA_BY_KIND["UnitType"][7] = scv
    with pytest.raises(TypeError):
        scv.fields["mineralPrice"] = 0
    with pytest.raises(TypeError):
        STATIC_DATA_PROVENANCE["diff_bucket"] = "candidate_differs"


def test_unit_type_helper_table_has_official_source_provenance() -> None:
    assert len(BWAPI_UNIT_TYPE_HELPERS) == int(UnitTypeId.MAX)
    assert UNIT_TYPE_HELPER_PROVENANCE == STATIC_DATA_PROVENANCE


def test_unit_type_helper_builder_edges_match_bwapi_logic() -> None:
    scv = unit_type_helper_data(UnitTypeId.TERRAN_SCV)
    depot = unit_type_helper_data(UnitTypeId.TERRAN_SUPPLY_DEPOT)
    archon = unit_type_helper_data(UnitTypeId.PROTOSS_ARCHON)
    none = unit_type_helper_data(UnitTypeId.NONE)

    assert scv is not None
    assert scv.fields["whatBuildsUnitTypeId"] == int(UnitTypeId.TERRAN_COMMAND_CENTER)
    assert scv.fields["whatBuildsAmount"] == 1

    assert depot is not None
    assert depot.fields["whatBuildsUnitTypeId"] == int(UnitTypeId.TERRAN_SCV)
    assert depot.fields["whatBuildsAmount"] == 1
    assert depot.fields["tileWidth"] == 3
    assert depot.fields["tileHeight"] == 2

    assert archon is not None
    assert archon.fields["whatBuildsUnitTypeId"] == int(UnitTypeId.PROTOSS_HIGH_TEMPLAR)
    assert archon.fields["whatBuildsAmount"] == 2

    assert none is not None
    assert none.fields["whatBuildsUnitTypeId"] == int(UnitTypeId.NONE)
    assert none.fields["whatBuildsAmount"] == 0


def test_unit_type_helper_requires_creep_edges() -> None:
    creep_colony = unit_type_helper_data(UnitTypeId.ZERG_CREEP_COLONY)
    sunken = unit_type_helper_data(UnitTypeId.ZERG_SUNKEN_COLONY)
    depot = unit_type_helper_data(UnitTypeId.TERRAN_SUPPLY_DEPOT)

    assert creep_colony is not None
    assert creep_colony.fields["requiresCreep"] is True
    assert sunken is not None
    assert sunken.fields["requiresCreep"] is True
    assert depot is not None
    assert depot.fields["requiresCreep"] is False


def test_unit_type_helper_worker_depot_and_resource_container_classifiers() -> None:
    for type_id in (
        UnitTypeId.TERRAN_SCV,
        UnitTypeId.ZERG_DRONE,
        UnitTypeId.PROTOSS_PROBE,
    ):
        record = unit_type_helper_data(type_id)
        assert record is not None
        assert record.fields["isWorker"] is True
        assert record.fields["isResourceDepot"] is False
        assert record.fields["isResourceContainer"] is False

    for type_id in (
        UnitTypeId.TERRAN_COMMAND_CENTER,
        UnitTypeId.ZERG_HATCHERY,
        UnitTypeId.PROTOSS_NEXUS,
    ):
        record = unit_type_helper_data(type_id)
        assert record is not None
        assert record.fields["isWorker"] is False
        assert record.fields["isResourceDepot"] is True
        assert record.fields["isResourceContainer"] is False

    for type_id in (
        UnitTypeId.RESOURCE_MINERAL_FIELD,
        UnitTypeId.RESOURCE_VESPENE_GEYSER,
    ):
        record = unit_type_helper_data(type_id)
        assert record is not None
        assert record.fields["isWorker"] is False
        assert record.fields["isResourceDepot"] is False
        assert record.fields["isResourceContainer"] is True

    for type_id in (
        UnitTypeId.TERRAN_MARINE,
        UnitTypeId.TERRAN_SUPPLY_DEPOT,
        UnitTypeId.NONE,
        UnitTypeId.UNKNOWN,
    ):
        record = unit_type_helper_data(type_id)
        assert record is not None
        assert record.fields["isWorker"] is False
        assert record.fields["isResourceDepot"] is False
        assert record.fields["isResourceContainer"] is False

    wraith = unit_type_helper_data(UnitTypeId.TERRAN_WRAITH)
    carrier = unit_type_helper_data(UnitTypeId.PROTOSS_CARRIER)
    command_center = unit_type_helper_data(UnitTypeId.TERRAN_COMMAND_CENTER)
    marine = unit_type_helper_data(UnitTypeId.TERRAN_MARINE)
    assert wraith is not None and wraith.fields["isFlyer"] is True
    assert wraith.fields["isBuilding"] is False
    assert wraith.fields["canProduce"] is False
    assert carrier is not None and carrier.fields["canProduce"] is True
    assert carrier.fields["isFlyer"] is True
    assert command_center is not None
    assert command_center.fields["isBuilding"] is True
    assert command_center.fields["isFlyingBuilding"] is True
    assert command_center.fields["canProduce"] is True
    assert marine is not None and marine.fields["isFlyer"] is False
    assert marine.fields["isBuilding"] is False
    depot = unit_type_helper_data(UnitTypeId.TERRAN_SUPPLY_DEPOT)
    assert depot is not None and depot.fields["isBuilding"] is True
    assert depot.fields["canProduce"] is False
    for record in BWAPI_UNIT_TYPE_HELPERS.values():
        for field in ("isBuilding", "isFlyer", "isFlyingBuilding", "canProduce"):
            assert isinstance(record.fields.get(field), bool)


def test_unit_type_helper_table_does_not_reintroduce_unresolved_scalar_fields() -> None:
    dark_templar = unit_type_helper_data(UnitTypeId.HERO_DARK_TEMPLAR)
    powerup_flag = unit_type_helper_data(UnitTypeId.POWERUP_FLAG)

    assert dark_templar is not None
    assert "supplyRequired" not in dark_templar.fields
    assert powerup_flag is not None
    assert "maxHitPoints" not in powerup_flag.fields
    assert unit_type_helper_field(UnitTypeId.TERRAN_SUPPLY_DEPOT, "tileWidth") == 3
    assert unit_type_helper_field(99999, "tileWidth", -1) == -1


def test_unit_type_helper_table_is_immutable() -> None:
    depot = unit_type_helper_data(UnitTypeId.TERRAN_SUPPLY_DEPOT)
    assert depot is not None

    with pytest.raises(TypeError):
        BWAPI_UNIT_TYPE_HELPERS[int(UnitTypeId.TERRAN_SUPPLY_DEPOT)] = depot
    with pytest.raises(TypeError):
        depot.fields["tileWidth"] = 0
    with pytest.raises(TypeError):
        UNIT_TYPE_HELPER_PROVENANCE["generator"] = "manual"
