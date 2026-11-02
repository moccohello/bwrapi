"""BWAPI 4.4 for Python: a GameData client of the BWAPI server inside the game.

`connect()` (or the `BWAPIClient` object, as in C++) attaches to the server,
`Broodwar` is the connected `Game`, and `BwapiModuleRunner` or
`python -m bwrapi.bwapi.run` drives an `AIModule` from the server's events.
"""

from .aimodule import AIModule
from .api_info import BWAPI_getRevision, BWAPI_isDebug, Tournament
from .api_namespaces import *
from .api_namespaces import __all__ as _api_namespace_exports
from .bullet import Bullet
from .client import BWAPIClient, Client, ServerConnectionError, connect
from .containers import Bulletset, Forceset, Playerset, Regionset, SetContainer, Unitset
from .filters import BestFilter, BestUnitFilter, CompareFilter, Filter, Highest, Lowest, UnaryFilter, UnitFilter
from .force import Force
from .game import Event, Game
from .global_game import Broodwar, GameWrapper, _get_current_game, endl, ends, flush
from .input_text import *
from .input_text import __all__ as _input_text_exports
from .interface import Interface, InterfaceEvent
from .orders import OrderId, order_id_name
from .player import Player
from .profile import BWAPI_CLIENT_VERSION, BWAPI_PROFILE_ID
from .region import Region
from .runner import BwapiModuleRunner, dispatch
from .static_data import (
    BWAPI_STATIC_DATA_BY_KIND,
    BWAPI_UNIT_TYPE_HELPERS,
    STATIC_DATA_KINDS,
    STATIC_DATA_PROVENANCE,
    UNIT_TYPE_HELPER_PROVENANCE,
    BwapiStaticRecord,
    BwapiUnitTypeHelperRecord,
    static_field,
    static_record,
    tech_energy_cost,
    tech_gas_price,
    tech_mineral_price,
    tech_research_time,
    tech_type_static_data,
    unit_type_helper_data,
    unit_type_helper_field,
    unit_type_static_data,
    upgrade_gas_price,
    upgrade_time,
    upgrade_type_static_data,
    weapon_type_static_data,
)
from .std_stream import bwerr, bwout, err, out
from .surface import (
    BWAPI_AIMODULE_METHODS,
    BWAPI_BULLET_METHODS,
    BWAPI_GAME_METHODS,
    BWAPI_PLAYER_METHODS,
    BWAPI_SURFACE_METHODS,
    BWAPI_TYPE_METHODS,
    BWAPI_UNIT_METHODS,
)
from .type_ids import (
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
from .types import *
from .types import __all__ as _type_exports
from .unit import Unit
from .unit_command import UnitCommand

BulletInterface = Bullet
ForceInterface = Force
PlayerInterface = Player
RegionInterface = Region
UnitInterface = Unit


def __getattr__(name: str):
    if name == "BroodwarPtr":
        return _get_current_game()
    raise AttributeError(name)


__all__ = [
    "AIModule", "BWAPIClient", "BWAPI_AIMODULE_METHODS", "BWAPI_BULLET_METHODS", "BWAPI_CLIENT_VERSION",
    "BWAPI_GAME_METHODS", "BWAPI_PLAYER_METHODS", "BWAPI_PROFILE_ID", "BWAPI_STATIC_DATA_BY_KIND",
    "BWAPI_SURFACE_METHODS", "BWAPI_TYPE_METHODS", "BWAPI_UNIT_METHODS", "BWAPI_UNIT_TYPE_HELPERS",
    "BWAPI_getRevision", "BWAPI_isDebug", "BestFilter", "BestUnitFilter", "Broodwar", "BroodwarPtr", "Bullet",
    "BulletInterface", "BulletTypeId", "Bulletset", "BwapiModuleRunner", "BwapiStaticRecord",
    "BwapiUnitTypeHelperRecord", "Client", "CompareFilter", "CoordinateTypeId", "DamageTypeId", "ErrorId", "Event",
    "EventTypeId", "ExplosionTypeId", "Filter", "FlagId", "Force", "ForceInterface", "Forceset", "Game",
    "GameTypeId", "GameWrapper", "Highest", "Interface", "InterfaceEvent", "LatencyId", "Lowest", "OrderId",
    "Player", "PlayerInterface", "PlayerTypeId", "Playerset", "RaceId", "Region",
    "RegionInterface", "Regionset", "ServerConnectionError", "SetContainer", "STATIC_DATA_KINDS",
    "STATIC_DATA_PROVENANCE", "TechTypeId", "Tournament", "UNIT_TYPE_HELPER_PROVENANCE", "UnaryFilter", "Unit",
    "UnitCommand", "UnitCommandTypeId", "UnitFilter", "UnitInterface", "UnitSizeTypeId", "UnitTypeId",
    "Unitset", "UpgradeTypeId", "WeaponTypeId", "bullet_type_name", "bwerr", "bwout", "connect",
    "coordinate_type_name", "damage_type_name", "dispatch", "endl", "ends", "err", "error_id_name",
    "event_type_name", "explosion_type_name", "flag_name", "flush", "game_type_name", "latency_name",
    "order_id_name", "out", "player_type_name", "race_id_name",
    "static_field", "static_record", "tech_energy_cost", "tech_gas_price", "tech_mineral_price",
    "tech_research_time", "tech_type_name", "tech_type_static_data", "unit_command_type_name",
    "unit_size_type_name", "unit_type_helper_data", "unit_type_helper_field", "unit_type_name",
    "unit_type_static_data", "upgrade_gas_price", "upgrade_time", "upgrade_type_name",
    "upgrade_type_static_data", "weapon_type_name", "weapon_type_static_data",
]
__all__ += _type_exports + _input_text_exports + _api_namespace_exports
