"""BWAPI Player over a GameData PlayerData row (PlayerShared.cpp, Player.cpp)."""

from __future__ import annotations

from typing import TYPE_CHECKING

from .containers import Unitset
from .gamedata import decode_text
from .interface import Interface
from .position import TilePosition
from .types import Color, PlayerType, Race, TechType, UnitType, UpgradeType, WeaponType

if TYPE_CHECKING:
    from .force import Force
    from .game import Game

_ALL_UNITS = 229  # UnitTypes::AllUnits


class Player(Interface):
    """One of the 12 BWAPI player slots of a Game."""

    __slots__ = ("game", "_id", "_d", "_units")

    def __init__(self, game: Game, player_id: int) -> None:
        self.game = game
        self._id = player_id
        self._d = game._data.players[player_id]
        self._units = Unitset()
        self._client_info = None
        self._interface_events = None

    def __repr__(self) -> str:
        return f"Player({self._id}, {self.getName()!r})"

    def getID(self) -> int: return self._id
    def getName(self) -> str: return decode_text(self._d.name)
    def getUnits(self) -> Unitset: return self._units
    def getRace(self) -> Race: return Race(self._d.race)
    def getType(self) -> PlayerType: return PlayerType(self._d.type)
    def getForce(self) -> Force | None: return self.game.getForce(self._d.force)

    def isAlly(self, player: Player | None) -> bool:
        return player is not None and self._d.isAlly[player.getID()]

    def isEnemy(self, player: Player | None) -> bool:
        return player is not None and self._d.isEnemy[player.getID()]

    def isNeutral(self) -> bool: return self._d.isNeutral
    def getStartLocation(self) -> TilePosition: return TilePosition(self._d.startLocationX, self._d.startLocationY)
    def isVictorious(self) -> bool: return self._d.isVictorious
    def isDefeated(self) -> bool: return self._d.isDefeated
    def leftGame(self) -> bool: return self._d.leftGame
    def isObserver(self) -> bool: return not self._d.isParticipating

    def minerals(self) -> int: return self._d.minerals
    def gas(self) -> int: return self._d.gas
    def gatheredMinerals(self) -> int: return self._d.gatheredMinerals
    def gatheredGas(self) -> int: return self._d.gatheredGas
    def repairedMinerals(self) -> int: return self._d.repairedMinerals
    def repairedGas(self) -> int: return self._d.repairedGas
    def refundedMinerals(self) -> int: return self._d.refundedMinerals
    def refundedGas(self) -> int: return self._d.refundedGas

    def spentMinerals(self) -> int:
        d = self._d
        return d.gatheredMinerals + d.refundedMinerals - d.minerals - d.repairedMinerals

    def spentGas(self) -> int:
        d = self._d
        return d.gatheredGas + d.refundedGas - d.gas - d.repairedGas

    def supplyTotal(self, race=None) -> int:
        race = self._supply_race(race)
        return self._d.supplyTotal[race] if 0 <= race < 3 else 0

    def supplyUsed(self, race=None) -> int:
        race = self._supply_race(race)
        return self._d.supplyUsed[race] if 0 <= race < 3 else 0

    def _supply_race(self, race) -> int:
        race = Race(race)
        return int(self.getRace() if race == Race() else race)

    def _count(self, counts, unit) -> int:
        unit = int(UnitType(unit))
        return counts[unit] if 0 <= unit < len(counts) else 0

    def allUnitCount(self, unit=_ALL_UNITS) -> int: return self._count(self._d.allUnitCount, unit)
    def visibleUnitCount(self, unit=_ALL_UNITS) -> int: return self._count(self._d.visibleUnitCount, unit)
    def completedUnitCount(self, unit=_ALL_UNITS) -> int: return self._count(self._d.completedUnitCount, unit)
    def deadUnitCount(self, unit=_ALL_UNITS) -> int: return self._count(self._d.deadUnitCount, unit)
    def killedUnitCount(self, unit=_ALL_UNITS) -> int: return self._count(self._d.killedUnitCount, unit)
    def incompleteUnitCount(self, unit=_ALL_UNITS) -> int: return self.allUnitCount(unit) - self.completedUnitCount(unit)

    def getUnitScore(self) -> int: return self._d.totalUnitScore
    def getKillScore(self) -> int: return self._d.totalKillScore
    def getBuildingScore(self) -> int: return self._d.totalBuildingScore
    def getRazingScore(self) -> int: return self._d.totalRazingScore
    def getCustomScore(self) -> int: return self._d.customScore
    def getColor(self) -> Color: return Color(self._d.color)

    def getUpgradeLevel(self, upgrade) -> int:
        upgrade = UpgradeType(upgrade)
        return self._d.upgradeLevel[upgrade] if upgrade.isValid() else 0

    def getMaxUpgradeLevel(self, upgrade) -> int:
        upgrade = UpgradeType(upgrade)
        return self._d.maxUpgradeLevel[upgrade] if upgrade.isValid() else 0

    def hasResearched(self, tech) -> bool:
        tech = TechType(tech)
        return tech.isValid() and self._d.hasResearched[tech]

    def isResearching(self, tech) -> bool:
        tech = TechType(tech)
        return tech.isValid() and self._d.isResearching[tech]

    def isUpgrading(self, upgrade) -> bool:
        upgrade = UpgradeType(upgrade)
        return upgrade.isValid() and self._d.isUpgrading[upgrade]

    def isResearchAvailable(self, tech) -> bool:
        tech = TechType(tech)
        return tech.isValid() and self._d.isResearchAvailable[tech]

    def isUnitAvailable(self, unit) -> bool:
        unit = UnitType(unit)
        return unit.isValid() and self._d.isUnitAvailable[unit]

    # ---- Player.cpp: values derived from types and upgrades -----------------------------------
    def maxEnergy(self, unit) -> int:
        unit = UnitType(unit)
        upgrade = {71: 44, 60: 47, 63: 49, 67: 40, 1: 21, 12: 23, 9: 19, 8: 22, 34: 51, 46: 32, 45: 31}.get(unit)
        return unit.maxEnergy() + (50 if upgrade is not None and self.getUpgradeLevel(upgrade) > 0 else 0)

    def topSpeed(self, unit) -> float:
        unit = UnitType(unit)
        speed = unit.topSpeed()
        upgrade = {2: 17, 42: 26, 37: 27, 38: 29, 65: 34, 69: 37, 84: 39, 70: 42, 39: 53}.get(unit)
        if upgrade is not None and self.getUpgradeLevel(upgrade) > 0:
            speed = max(853 / 256, speed + 427 / 256 if unit == 70 else speed * 1.5)
        return speed

    def weaponMaxRange(self, weapon) -> int:
        weapon = WeaponType(weapon)
        upgrade, bonus = {0: (16, 32), 38: (30, 32), 66: (33, 64), 8: (54, 96)}.get(weapon, (None, 0))
        return weapon.maxRange() + (bonus if upgrade is not None and self.getUpgradeLevel(upgrade) > 0 else 0)

    def sightRange(self, unit) -> int:
        unit = UnitType(unit)
        upgrade = {1: 20, 42: 25, 84: 38, 70: 41}.get(unit)
        return 352 if upgrade is not None and self.getUpgradeLevel(upgrade) > 0 else unit.sightRange()

    def weaponDamageCooldown(self, unit) -> int:
        unit = UnitType(unit)
        cooldown = unit.groundWeapon().damageCooldown()
        if unit == 37 and self.getUpgradeLevel(28) > 0:
            cooldown = min(max(cooldown // 2, 5), 250)
        return cooldown

    def armor(self, unit) -> int:
        unit = UnitType(unit)
        armor = unit.armor() + self.getUpgradeLevel(unit.armorUpgrade())
        if (unit == 39 and self.getUpgradeLevel(52) > 0) or unit == 48:
            armor += 2
        return armor

    def damage(self, weapon) -> int:
        weapon = WeaponType(weapon)
        return (weapon.damageAmount() + self.getUpgradeLevel(weapon.upgradeType()) * weapon.damageBonus()) * weapon.damageFactor()

    def getTextColor(self) -> int:
        return {
            111: 8,   # Red -> BrightRed
            165: 14,  # Blue
            159: 15,  # Teal
            164: 16,  # Purple
            156: 17,  # Orange
            19: 21,   # Brown
            255: 22,  # White -> PlayerWhite
            135: 23,  # Yellow -> PlayerYellow
            117: 24,  # Green -> DarkGreen
            136: 25,  # player 10
            134: 27,  # player 11
            51: 28,   # player 12
        }.get(self.getColor(), 2)

    def hasUnitTypeRequirement(self, unit, amount: int = 1) -> bool:
        unit = UnitType(unit)
        if unit == 228:  # None
            return True
        if unit == 131:  # Zerg_Hatchery
            return self.completedUnitCount(131) + self.allUnitCount(132) + self.allUnitCount(133) >= amount
        if unit == 132:  # Zerg_Lair
            return self.completedUnitCount(132) + self.allUnitCount(133) >= amount
        if unit == 141:  # Zerg_Spire
            return self.completedUnitCount(141) + self.allUnitCount(137) >= amount
        return self.completedUnitCount(unit) >= amount
