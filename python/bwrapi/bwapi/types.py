from __future__ import annotations

from operator import index
from collections import deque
from types import SimpleNamespace
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from typing import Self

from .position import TilePosition

from ._type_data import TYPE_DATA
from .containers import SetContainer
from .static_data import tech_gas_price


class Type(int):
    """An immutable BWAPI type value with Type.h range normalization and name lookup."""

    set = SetContainer
    list = deque

    def __new__(cls, value=None) -> Self:
        enum = TYPE_DATA[cls.__name__]["enum"]
        value = enum["None"] if value is None else index(value)
        return int.__new__(cls, value if 0 <= value <= enum["Unknown"] else enum["Unknown"])

    def getID(self) -> int:
        return int(self)

    def isValid(self) -> bool:
        return 0 <= self <= TYPE_DATA[type(self).__name__]["enum"]["Unknown"]

    def getName(self) -> str:
        return TYPE_DATA[type(self).__name__]["names"][self]

    def toString(self) -> str:
        return self.getName()

    def c_str(self) -> str:
        return self.getName()

    def __str__(self) -> str:
        return self.getName()

    def __repr__(self) -> str:
        cls = type(self)
        name = _REPR_NAMES.get(cls, {}).get(int(self))
        return f"{cls.__name__}s.{name}" if name else f"{cls.__name__}({int(self)})"

    @classmethod
    def getType(cls, name: str) -> Self:
        normalize = lambda s: "".join(c.lower() for c in s if c != "_" and not c.isspace())
        needle = normalize(name)
        for type_id, candidate in enumerate(TYPE_DATA[cls.__name__]["names"]):
            if normalize(candidate) == needle:
                return cls(type_id)
        return cls(TYPE_DATA[cls.__name__]["enum"]["Unknown"])

    def _field(self, name):
        return TYPE_DATA[type(self).__name__][name][self]


class UnitType(Type):
    def getRace(self) -> Race: return Race(self._field("unitRace"))
    def whatBuilds(self) -> tuple[UnitType, int]:
        builder = UnitType(self._field("whatBuilds"))
        return builder, 0 if builder == 228 else 2 if self in (63, 68) else 1

    def requiredUnits(self) -> dict[UnitType, int]:
        result = {}
        if self in TYPE_DATA["UnitType"]["unitTypeSet"]:
            builder, count = self.whatBuilds()
            if builder != 228:
                result[builder] = count
        for unit, required, count in TYPE_DATA["UnitType"]["requirements"]:
            if unit == self:
                result[UnitType(required)] = count
        return result

    def requiredTech(self) -> TechType: return TechType(32 if self in (97, 103) else 44)
    def cloakingTech(self) -> TechType:
        return TechType(10 if self in (1, 16, 51, 99, 100, 104) else 9 if self in (8, 21) else 44)
    def abilities(self) -> SetContainer[TechType]: return SetContainer(TechType(x) for x in self._field("unitTechs"))
    def upgrades(self) -> SetContainer[UpgradeType]: return SetContainer(UpgradeType(x) for x in self._field("upgrades"))
    def armorUpgrade(self) -> UpgradeType: return UpgradeType(self._field("armorUpgrade"))
    def maxHitPoints(self) -> int: return self._field("defaultMaxHP")
    def maxShields(self) -> int: return self._field("defaultMaxSP")
    def maxEnergy(self) -> int: return (250 if self.isHero() else 200) if self.isSpellcaster() else 0
    def armor(self) -> int: return self._field("defaultArmorAmount")
    def mineralPrice(self) -> int: return self._field("defaultOreCost")
    def gasPrice(self) -> int: return self._field("defaultGasCost")
    def buildTime(self) -> int: return self._field("defaultTimeCost")
    def supplyRequired(self) -> int: return self._field("unitSupplyRequired")
    def supplyProvided(self) -> int: return self._field("unitSupplyProvided")
    def spaceRequired(self) -> int: return self._field("unitSpaceRequired")
    def spaceProvided(self) -> int: return self._field("unitSpaceProvided")
    def buildScore(self) -> int: return self._field("unitBuildScore")
    def destroyScore(self) -> int: return self._field("unitDestroyScore")
    def size(self) -> UnitSizeType: return UnitSizeType(self._field("unitSize"))
    def tileWidth(self) -> int: return self._field("unitDimensions")[0]
    def tileHeight(self) -> int: return self._field("unitDimensions")[1]
    def tileSize(self) -> TilePosition: return TilePosition(self.tileWidth(), self.tileHeight())
    def dimensionLeft(self) -> int: return self._field("unitDimensions")[2]
    def dimensionUp(self) -> int: return self._field("unitDimensions")[3]
    def dimensionRight(self) -> int: return self._field("unitDimensions")[4]
    def dimensionDown(self) -> int: return self._field("unitDimensions")[5]
    def width(self) -> int: return self.dimensionLeft() + 1 + self.dimensionRight()
    def height(self) -> int: return self.dimensionUp() + 1 + self.dimensionDown()
    def seekRange(self) -> int: return self._field("seekRangeTiles") * 32
    def sightRange(self) -> int: return self._field("sightRangeTiles") * 32
    def groundWeapon(self) -> WeaponType: return WeaponType(self._field("groundWeapon"))
    def airWeapon(self) -> WeaponType: return WeaponType(self._field("airWeapon"))
    def maxGroundHits(self) -> int: return self._field("groundWeaponHits")
    def maxAirHits(self) -> int: return self._field("airWeaponHits")
    def topSpeed(self) -> float: return self._field("unitTopSpeeds")
    def acceleration(self) -> int: return self._field("unitAcceleration")
    def haltDistance(self) -> int: return self._field("unitHaltDistance")
    def turnRadius(self) -> int: return self._field("unitTurnRadius")
    def _flag(self, name) -> bool: return bool(self._field("unitFlags") & TYPE_DATA["UnitType"]["flagBits"][name])
    def canProduce(self) -> bool: return self._flag("ProducesUnits")
    def canAttack(self) -> bool:
        if self in (72, 82, 83, 81, 14): return True
        if self == 184: return False
        return self.airWeapon() != 130 or self.groundWeapon() != 130
    def canMove(self) -> bool: return self._flag("AutoAttackAndMove")
    def isFlyer(self) -> bool: return self._flag("Flyer")
    def regeneratesHP(self) -> bool: return self._flag("RegeneratesHP")
    def isSpellcaster(self) -> bool: return self._flag("Spellcaster")
    def hasPermanentCloak(self) -> bool: return self._flag("PermanentCloak")
    def isInvincible(self) -> bool: return self._flag("Invincible")
    def isOrganic(self) -> bool: return self._flag("OrganicUnit")
    def isMechanical(self) -> bool: return self._flag("Mechanical")
    def isRobotic(self) -> bool: return self._flag("RoboticUnit")
    def isDetector(self) -> bool: return self._flag("Detector")
    def isResourceContainer(self) -> bool: return self._flag("ResourceContainer")
    def isResourceDepot(self) -> bool: return self._flag("ResourceDepot")
    def isRefinery(self) -> bool: return self in (110, 149, 157)
    def isWorker(self) -> bool: return self._flag("Worker")
    def requiresPsi(self) -> bool: return self._flag("RequiresPsi")
    def requiresCreep(self) -> bool: return self._flag("CreepBuilding")
    def isTwoUnitsInOneEgg(self) -> bool: return self._flag("TwoUnitsIn1Egg")
    def isBurrowable(self) -> bool: return self._flag("Burrowable")
    def isCloakable(self) -> bool: return self._flag("Cloakable")
    def isBuilding(self) -> bool: return self._flag("Building")
    def isAddon(self) -> bool: return self._flag("Addon")
    def isFlyingBuilding(self) -> bool: return self._flag("FlyingBuilding")
    def isNeutral(self) -> bool: return self.getRace() == 7 and (self.isCritter() or self.isResourceContainer() or self.isSpell())
    def isHero(self) -> bool: return self._flag("Hero") or self in (74, 15)
    def isPowerup(self) -> bool: return self in (128, 129) or 215 <= self < 228
    def isBeacon(self) -> bool: return self in (194, 195, 196)
    def isFlagBeacon(self) -> bool: return self in (197, 198, 199)
    def isSpecialBuilding(self) -> bool: return self.isBuilding() and self.whatBuilds()[1] == 0 and self != 130
    def isSpell(self) -> bool: return self in (202, 105, 33)
    def producesCreep(self) -> bool: return self.producesLarva() or self in (143, 144, 146)
    def producesLarva(self) -> bool: return self in (131, 132, 133)
    def isMineralField(self) -> bool: return self in (176, 177, 178)
    def isCritter(self) -> bool: return self in (89, 90, 93, 94, 95, 96)
    def canBuildAddon(self) -> bool: return self in (106, 113, 114, 116)
    def buildsWhat(self) -> SetContainer[UnitType]: return SetContainer(UnitType(x) for x in self._field("buildsWhat"))
    def researchesWhat(self) -> SetContainer[TechType]: return SetContainer(TechType(x) for x in self._field("researchesWhat"))
    def upgradesWhat(self) -> SetContainer[UpgradeType]: return SetContainer(UpgradeType(x) for x in self._field("upgradesWhat"))
    def isSuccessorOf(self, unit_type) -> bool:
        return self == unit_type or (unit_type == 131 and self in (132, 133)) or (unit_type == 132 and self == 133) or (unit_type == 141 and self == 137)


class TechType(Type):
    def getRace(self) -> Race: return Race(self._field("techRaces"))
    def mineralPrice(self) -> int: return self._field("defaultOreCost")
    def gasPrice(self) -> int: return tech_gas_price(int(self))
    def researchTime(self) -> int: return self._field("defaultTimeCost")
    def energyCost(self) -> int: return self._field("defaultEnergyCost")
    def whatResearches(self) -> UnitType: return UnitType(self._field("whatResearches"))
    def getWeapon(self) -> WeaponType: return WeaponType(self._field("techWeapons"))
    def targetsUnit(self) -> bool: return bool(self._field("techTypeFlags") & 1)
    def targetsPosition(self) -> bool: return bool(self._field("techTypeFlags") & 2)
    def whatUses(self) -> SetContainer[UnitType]: return SetContainer(UnitType(x) for x in self._field("techWhatUses"))
    def getOrder(self) -> Order: return Order(self._field("techOrders"))
    def requiredUnit(self) -> UnitType: return UnitType(132 if self == 32 else 228)


class UpgradeType(Type):
    def getRace(self) -> Race: return Race(self._field("upgradeRaces"))
    def mineralPrice(self, level=1) -> int: return self._field("defaultOreCostBase") + max(0, index(level) - 1) * self.mineralPriceFactor()
    def mineralPriceFactor(self) -> int: return self._field("defaultOreCostFactor")
    # UpgradeType.cpp: the gas price is the mineral price.
    def gasPrice(self, level=1) -> int: return self.mineralPrice(level)
    def gasPriceFactor(self) -> int: return self.mineralPriceFactor()
    def upgradeTime(self, level=1) -> int: return self._field("defaultTimeCostBase") + max(0, index(level) - 1) * self.upgradeTimeFactor()
    def upgradeTimeFactor(self) -> int: return self._field("defaultTimeCostFactor")
    def whatUpgrades(self) -> UnitType: return UnitType(self._field("whatUpgrades"))
    def whatUses(self) -> SetContainer[UnitType]: return SetContainer(UnitType(x) for x in self._field("upgradeWhatUses"))
    def maxRepeats(self) -> int: return self._field("defaultMaxRepeats")
    def whatsRequired(self, level=1) -> UnitType:
        level = index(level)
        return UnitType(TYPE_DATA["UpgradeType"]["requirements"][level - 1][self] if 1 <= level <= 3 else 228)


class WeaponType(Type):
    def getTech(self) -> TechType: return TechType(self._field("attachedTech"))
    def whatUses(self) -> UnitType: return UnitType(self._field("whatUses"))
    def damageAmount(self) -> int: return self._field("defaultWpnDamageAmt")
    def damageBonus(self) -> int: return self._field("defaultWpnDamageBonus")
    def damageCooldown(self) -> int: return self._field("wpnDamageCooldowns")
    def damageFactor(self) -> int: return self._field("wpnDamageFactor")
    def upgradeType(self) -> UpgradeType: return UpgradeType(self._field("upgrade"))
    def damageType(self) -> DamageType: return DamageType(self._field("damageType"))
    def explosionType(self) -> ExplosionType: return ExplosionType(self._field("explosionType"))
    def minRange(self) -> int: return self._field("wpnMinRange")
    def maxRange(self) -> int: return self._field("wpnMaxRange")
    def innerSplashRadius(self) -> int: return self._field("wpnSplashRangeInner")
    def medianSplashRadius(self) -> int: return self._field("wpnSplashRangeMid")
    def outerSplashRadius(self) -> int: return self._field("wpnSplashRangeOuter")
    def _flag(self, name) -> bool: return bool(self._field("wpnFlags") & TYPE_DATA["WeaponType"]["flagBits"][name])
    def targetsAir(self) -> bool: return self._flag("TARG_AIR")
    def targetsGround(self) -> bool: return self._flag("TARG_GROUND")
    def targetsMechanical(self) -> bool: return self._flag("TARG_MECH")
    def targetsOrganic(self) -> bool: return self._flag("TARG_ORGANIC")
    def targetsNonBuilding(self) -> bool: return self._flag("TARG_NOBUILD")
    def targetsNonRobotic(self) -> bool: return self._flag("TARG_NOROBOT")
    def targetsTerrain(self) -> bool: return self._flag("TARG_TERRAIN")
    def targetsOrgOrMech(self) -> bool: return self._flag("TARG_ORGMECH")
    def targetsOwn(self) -> bool: return self._flag("TARG_OWN")


class Race(Type):
    def getWorker(self) -> UnitType: return UnitType(self._field("workerTypes"))
    def getResourceDepot(self) -> UnitType: return UnitType(self._field("baseTypes"))
    def getCenter(self) -> UnitType: return self.getResourceDepot()
    def getRefinery(self) -> UnitType: return UnitType(self._field("refineryTypes"))
    def getTransport(self) -> UnitType: return UnitType(self._field("transportTypes"))
    def getSupplyProvider(self) -> UnitType: return UnitType(self._field("supplyTypes"))


class PlayerType(Type):
    def isLobbyType(self) -> bool: return self in (5, 6, 3, 4, 7)
    def isGameType(self) -> bool: return self in (2, 1, 3, 4, 7)


class GameType(Type):
    def __new__(cls, game_type_id=None) -> Self: return super().__new__(cls, game_type_id)
    def isValid(self) -> bool:
        return int(self) in TYPE_DATA["GameType"]["gameTypeSet"]
    @property
    def game_type_id(self) -> int: return self.getID()
class DamageType(Type): pass
class ExplosionType(Type): pass
class UnitSizeType(Type): pass
class BulletType(Type): pass
class Error(Type): pass
class Order(Type): pass
class UnitCommandType(Type): pass


class Color(Type):
    def __new__(cls, red=0, green=None, blue=None) -> Self:
        if green is None and blue is None:
            return super().__new__(cls, red)
        if green is None or blue is None:
            raise TypeError("Color needs all three RGB components")
        rgb = tuple(index(value) & 252 for value in (red, green, blue))
        palette = TYPE_DATA["Color"]["palette"]
        # getRGBIndex: BYTE conversion, 6-bit quantization, reserved colors and 255 skipped, first tie wins.
        best = min((i for i in range(255) if len(palette[i]) == 3 or palette[i][3] == 0),
                   key=lambda i: sum((rgb[c] - palette[i][c]) ** 2 for c in range(3)))
        return super().__new__(cls, best)

    def red(self) -> int: return self._field("palette")[0]
    def green(self) -> int: return self._field("palette")[1]
    def blue(self) -> int: return self._field("palette")[2]


def _namespace(cls, set_name, all_name):
    data = TYPE_DATA[cls.__name__]
    namespace = SimpleNamespace(**{name: cls(value) for name, value in data["enum"].items() if name != "MAX"})
    setattr(namespace, "None_", cls())
    setattr(namespace, all_name, lambda: SetContainer(cls(x) for x in data[set_name]))
    return namespace


UnitTypes = _namespace(UnitType, "unitTypeSet", "allUnitTypes")
UnitTypes.allMacroTypes = lambda: SetContainer(UnitType(x) for x in TYPE_DATA["UnitType"]["macroTypeSet"])
UnitTypes.maxUnitWidth = lambda: max(x.width() for x in UnitTypes.allUnitTypes())
UnitTypes.maxUnitHeight = lambda: max(x.height() for x in UnitTypes.allUnitTypes())
TechTypes = _namespace(TechType, "techTypeSet", "allTechTypes")
UpgradeTypes = _namespace(UpgradeType, "upgradeTypeSet", "allUpgradeTypes")
WeaponTypes = _namespace(WeaponType, "normalWeaponTypeSet", "normalWeaponTypes")
WeaponTypes.specialWeaponTypes = lambda: SetContainer(WeaponType(x) for x in TYPE_DATA["WeaponType"]["specialWeaponTypeSet"])
WeaponTypes.allWeaponTypes = lambda: SetContainer((*WeaponTypes.normalWeaponTypes(), *WeaponTypes.specialWeaponTypes(), WeaponType(), WeaponTypes.Unknown))
Races = _namespace(Race, "raceSet", "allRaces")
PlayerTypes = _namespace(PlayerType, "playerTypeSet", "allPlayerTypes")
GameTypes = _namespace(GameType, "gameTypeSet", "allGameTypes")
DamageTypes = _namespace(DamageType, "typeSet", "allDamageTypes")
ExplosionTypes = _namespace(ExplosionType, "explosionTypeSet", "allExplosionTypes")
UnitSizeTypes = _namespace(UnitSizeType, "unitSizeTypeSet", "allUnitSizeTypes")
BulletTypes = _namespace(BulletType, "bulletTypeSet", "allBulletTypes")
Errors = _namespace(Error, "typeSet", "allErrors")
Orders = _namespace(Order, "orderSet", "allOrders")
UnitCommandTypes = _namespace(UnitCommandType, "unitCommandTypeSet", "allUnitCommandTypes")
Colors = SimpleNamespace(**{name: Color(value) for name, value in TYPE_DATA["Color"]["constants"].items()})

# repr() names each value after its first constant in the namespace (`UnitTypes.Terran_Marine`).
_REPR_NAMES: dict[type, dict[int, str]] = {}
for _cls in (UnitType, TechType, UpgradeType, WeaponType, Race, PlayerType, GameType, DamageType, ExplosionType,
             UnitSizeType, BulletType, Error, Order, UnitCommandType, Color):
    _names = _REPR_NAMES[_cls] = {}
    for _name, _value in vars(globals()[_cls.__name__ + "s"]).items():
        if type(_value) is _cls and _name != "None":
            _names.setdefault(int(_value), _name)

__all__ = ["Type", "UnitType", "UnitTypes", "TechType", "TechTypes", "UpgradeType", "UpgradeTypes", "WeaponType", "WeaponTypes", "Race", "Races", "PlayerType", "PlayerTypes", "GameType", "GameTypes", "DamageType", "DamageTypes", "ExplosionType", "ExplosionTypes", "UnitSizeType", "UnitSizeTypes", "BulletType", "BulletTypes", "Error", "Errors", "Order", "Orders", "UnitCommandType", "UnitCommandTypes"]
__all__ += ["Color", "Colors"]
