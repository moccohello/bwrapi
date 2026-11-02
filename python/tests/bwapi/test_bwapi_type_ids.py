from bwrapi.bwapi import (
    BulletTypeId,
    CoordinateTypeId,
    DamageTypeId,
    ErrorId,
    EventTypeId,
    ExplosionTypeId,
    FlagId,
    GameTypeId,
    LatencyId,
    PlayerTypeId,
    RaceId,
    TechTypeId,
    UnitCommandTypeId,
    UnitSizeTypeId,
    UnitTypeId,
    UpgradeTypeId,
    WeaponTypeId,
)
from bwrapi.bwapi.type_ids import (
    bullet_type_name,
    coordinate_type_name,
    damage_type_name,
    error_id_name,
    event_type_name,
    explosion_type_name,
    flag_name,
    game_type_name,
    latency_name,
    player_type_name,
    race_id_name,
    tech_type_name,
    unit_command_type_name,
    unit_size_type_name,
    unit_type_name,
    upgrade_type_name,
    weapon_type_name,
)


def test_common_unit_type_ids_match_bwapi_reference_values() -> None:
    assert len(UnitTypeId) == 235
    assert int(UnitTypeId.SPELL_SCANNER_SWEEP) == 33
    assert int(UnitTypeId.TERRAN_MEDIC) == 34
    assert int(UnitTypeId.ZERG_LURKER_EGG) == 97
    assert int(UnitTypeId.ZERG_LURKER) == 103
    assert int(UnitTypeId.TERRAN_COMMAND_CENTER) == 106
    assert int(UnitTypeId.TERRAN_BARRACKS) == 111
    assert int(UnitTypeId.TERRAN_FACTORY) == 113
    assert int(UnitTypeId.TERRAN_STARPORT) == 114
    assert int(UnitTypeId.TERRAN_MACHINE_SHOP) == 120
    assert int(UnitTypeId.TERRAN_ENGINEERING_BAY) == 122
    assert int(UnitTypeId.TERRAN_ARMORY) == 123
    assert int(UnitTypeId.PROTOSS_PYLON) == 156
    assert int(UnitTypeId.ALL_UNITS) == 229
    assert int(UnitTypeId.UNKNOWN) == 233
    assert int(UnitTypeId.MAX) == 234
    assert unit_type_name(106) == "TERRAN_COMMAND_CENTER"
    assert unit_type_name(156) == "PROTOSS_PYLON"
    assert unit_type_name(111) == "TERRAN_BARRACKS"
    assert unit_type_name(9999) is None


def test_common_tech_type_ids_match_bwapi_reference_values() -> None:
    assert len(TechTypeId) == 39
    assert int(TechTypeId.STIM_PACKS) == 0
    assert int(TechTypeId.SCANNER_SWEEP) == 4
    assert int(TechTypeId.SPAWN_BROODLINGS) == 13
    assert int(TechTypeId.HALLUCINATION) == 20
    assert int(TechTypeId.DEFENSIVE_MATRIX) == 6
    assert int(TechTypeId.RESTORATION) == 24
    assert int(TechTypeId.OPTICAL_FLARE) == 30
    assert int(TechTypeId.HEALING) == 34
    assert int(TechTypeId.NUCLEAR_STRIKE) == 45
    assert int(TechTypeId.MAX) == 47
    assert tech_type_name(4) == "SCANNER_SWEEP"
    assert tech_type_name(30) == "OPTICAL_FLARE"


def test_common_upgrade_type_ids_match_bwapi_reference_values() -> None:
    assert len(UpgradeTypeId) == 54
    assert int(UpgradeTypeId.TERRAN_INFANTRY_ARMOR) == 0
    assert int(UpgradeTypeId.TERRAN_INFANTRY_WEAPONS) == 7
    assert int(UpgradeTypeId.TERRAN_VEHICLE_WEAPONS) == 8
    assert int(UpgradeTypeId.TERRAN_SHIP_WEAPONS) == 9
    assert int(UpgradeTypeId.PROTOSS_GROUND_WEAPONS) == 13
    assert int(UpgradeTypeId.U_238_SHELLS) == 16
    assert int(UpgradeTypeId.CHARON_BOOSTERS) == 54
    assert int(UpgradeTypeId.NONE) == 61
    assert int(UpgradeTypeId.MAX) == 63
    assert upgrade_type_name(7) == "TERRAN_INFANTRY_WEAPONS"
    assert upgrade_type_name(54) == "CHARON_BOOSTERS"


def test_bwapi_weapon_and_command_type_ids_match_reference_values() -> None:
    assert len(WeaponTypeId) == 104
    assert int(WeaponTypeId.GAUSS_RIFLE) == 0
    assert int(WeaponTypeId.YAMATO_GUN) == 30
    assert int(WeaponTypeId.PSIONIC_STORM) == 84
    assert int(WeaponTypeId.NONE) == 130
    assert int(WeaponTypeId.MAX) == 132
    assert weapon_type_name(84) == "PSIONIC_STORM"

    assert len(UnitCommandTypeId) == 47
    assert int(UnitCommandTypeId.MOVE) == 10
    assert int(UnitCommandTypeId.GATHER) == 15
    assert int(UnitCommandTypeId.LIFT) == 24
    assert int(UnitCommandTypeId.RIGHT_CLICK_UNIT) == 31
    assert int(UnitCommandTypeId.USE_TECH_UNIT) == 42
    assert int(UnitCommandTypeId.MAX) == 46
    assert unit_command_type_name(31) == "RIGHT_CLICK_UNIT"


def test_bwapi_damage_explosion_size_and_bullet_type_ids_match_reference_values() -> None:
    assert len(DamageTypeId) == 8
    assert int(DamageTypeId.INDEPENDENT) == 0
    assert int(DamageTypeId.EXPLOSIVE) == 1
    assert int(DamageTypeId.NORMAL) == 3
    assert int(DamageTypeId.IGNORE_ARMOR) == 4
    assert int(DamageTypeId.MAX) == 7
    assert damage_type_name(3) == "NORMAL"

    assert len(ExplosionTypeId) == 27
    assert int(ExplosionTypeId.NONE) == 0
    assert int(ExplosionTypeId.RADIAL_SPLASH) == 2
    assert int(ExplosionTypeId.EMP_SHOCKWAVE) == 8
    assert int(ExplosionTypeId.YAMATO_GUN) == 15
    assert int(ExplosionTypeId.AIR_SPLASH) == 24
    assert int(ExplosionTypeId.MAX) == 26
    assert explosion_type_name(24) == "AIR_SPLASH"

    assert len(UnitSizeTypeId) == 7
    assert int(UnitSizeTypeId.INDEPENDENT) == 0
    assert int(UnitSizeTypeId.SMALL) == 1
    assert int(UnitSizeTypeId.MEDIUM) == 2
    assert int(UnitSizeTypeId.LARGE) == 3
    assert int(UnitSizeTypeId.MAX) == 6
    assert unit_size_type_name(2) == "MEDIUM"

    assert len(BulletTypeId) == 42
    assert int(BulletTypeId.MELEE) == 0
    assert int(BulletTypeId.FUSION_CUTTER_HIT) == 141
    assert int(BulletTypeId.GAUSS_RIFLE_HIT) == 142
    assert int(BulletTypeId.INVISIBLE) == 172
    assert int(BulletTypeId.OPTICAL_FLARE_GRENADE) == 201
    assert int(BulletTypeId.CORROSIVE_ACID_HIT) == 205
    assert int(BulletTypeId.NONE) == 209
    assert int(BulletTypeId.UNKNOWN) == 210
    assert int(BulletTypeId.MAX) == 211
    assert bullet_type_name(157) == "PSIONIC_STORM"


def test_bwapi_event_coordinate_flag_and_latency_ids_match_reference_values() -> None:
    assert len(EventTypeId) == 19
    assert int(EventTypeId.MATCH_START) == 0
    assert int(EventTypeId.MATCH_FRAME) == 2
    assert int(EventTypeId.UNIT_CREATE) == 12
    assert int(EventTypeId.UNIT_COMPLETE) == 17
    assert int(EventTypeId.NONE) == 18
    assert event_type_name(13) == "UNIT_DESTROY"

    assert len(CoordinateTypeId) == 4
    assert int(CoordinateTypeId.NONE) == 0
    assert int(CoordinateTypeId.SCREEN) == 1
    assert int(CoordinateTypeId.MAP) == 2
    assert int(CoordinateTypeId.MOUSE) == 3
    assert coordinate_type_name(2) == "MAP"

    assert len(FlagId) == 3
    assert int(FlagId.COMPLETE_MAP_INFORMATION) == 0
    assert int(FlagId.USER_INPUT) == 1
    assert int(FlagId.MAX) == 2
    assert flag_name(1) == "USER_INPUT"

    assert len(LatencyId) == 7
    assert int(LatencyId.SINGLE_PLAYER) == 2
    assert int(LatencyId.LAN_LOW) == 5
    assert int(LatencyId.BATTLENET_LOW) == 14
    assert int(LatencyId.BATTLENET_HIGH) == 24
    assert latency_name(19) == "BATTLENET_MEDIUM"


def test_bwapi_session_and_error_type_ids_match_reference_values() -> None:
    assert int(RaceId.ZERG) == 0
    assert int(RaceId.TERRAN) == 1
    assert int(RaceId.PROTOSS) == 2
    assert int(RaceId.RANDOM) == 6
    assert int(RaceId.UNKNOWN) == 8
    assert race_id_name(1) == "TERRAN"

    assert int(PlayerTypeId.COMPUTER) == 1
    assert int(PlayerTypeId.PLAYER) == 2
    assert int(PlayerTypeId.NEUTRAL) == 7
    assert int(PlayerTypeId.OBSERVER) == 9
    assert int(PlayerTypeId.MAX) == 13
    assert player_type_name(9) == "OBSERVER"

    assert int(GameTypeId.MELEE) == 2
    assert int(GameTypeId.USE_MAP_SETTINGS) == 10
    assert int(GameTypeId.TOP_VS_BOTTOM) == 15
    assert int(GameTypeId.UNKNOWN) == 33
    assert game_type_name(15) == "TOP_VS_BOTTOM"

    assert int(ErrorId.UNIT_DOES_NOT_EXIST) == 0
    assert int(ErrorId.INSUFFICIENT_MINERALS) == 11
    assert int(ErrorId.INVALID_PARAMETER) == 25
    assert int(ErrorId.NONE) == 26
    assert int(ErrorId.MAX) == 28
    assert error_id_name(11) == "INSUFFICIENT_MINERALS"
