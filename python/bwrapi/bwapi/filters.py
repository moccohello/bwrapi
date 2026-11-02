from __future__ import annotations

import operator
from types import SimpleNamespace
from typing import TYPE_CHECKING, Callable

if TYPE_CHECKING:
    from .unit import Unit


class UnaryFilter:
    """A unit predicate: combine with `&`, `|` and `~`; call it with a unit to get a bool."""

    def __init__(self, predicate: Callable[[Unit], object] | None) -> None: self.predicate = predicate
    def __call__(self, value: Unit) -> bool: return bool(self.predicate(value))
    def isValid(self) -> bool: return self.predicate is not None
    def __and__(self, other) -> UnaryFilter: return UnaryFilter(lambda value: self(value) and other(value))
    def __or__(self, other) -> UnaryFilter: return UnaryFilter(lambda value: self(value) or other(value))
    def __invert__(self) -> UnaryFilter: return UnaryFilter(lambda value: not self(value)) if self.isValid() else UnaryFilter(None)
    def __bool__(self) -> bool: raise TypeError("combine filters with the &, | and ~ operators")


class CompareFilter:
    """A unit value: compare it to get a `UnaryFilter` (`Filter.HP < 20`), or call it with a unit."""

    def __init__(self, predicate: Callable[[Unit], object] | None) -> None: self.predicate = predicate
    def __call__(self, value: Unit): return self.predicate(value)
    def isValid(self) -> bool: return self.predicate is not None
    def _compare(self, other, op):
        return UnaryFilter(lambda value: op(self(value), other(value) if callable(other) else other))
    def _arithmetic(self, other, op):
        return CompareFilter(lambda value: int(op(self(value), other(value) if callable(other) else other)))
    def __eq__(self, other) -> UnaryFilter: return self._compare(other, operator.eq)
    def __ne__(self, other) -> UnaryFilter: return self._compare(other, operator.ne)
    def __lt__(self, other) -> UnaryFilter: return self._compare(other, operator.lt)
    def __le__(self, other) -> UnaryFilter: return self._compare(other, operator.le)
    def __gt__(self, other) -> UnaryFilter: return self._compare(other, operator.gt)
    def __ge__(self, other) -> UnaryFilter: return self._compare(other, operator.ge)
    def __add__(self, other) -> CompareFilter: return self._arithmetic(other, operator.add)
    def __sub__(self, other) -> CompareFilter: return self._arithmetic(other, operator.sub)
    def __mul__(self, other) -> CompareFilter: return self._arithmetic(other, operator.mul)
    def __and__(self, other) -> CompareFilter: return self._arithmetic(other, operator.and_)
    def __or__(self, other) -> CompareFilter: return self._arithmetic(other, operator.or_)
    def __xor__(self, other) -> CompareFilter: return self._arithmetic(other, operator.xor)
    def __truediv__(self, other) -> CompareFilter: return self._arithmetic(other, lambda a, b: 2147483647 if b == 0 else _divide(a, b))
    def __mod__(self, other) -> CompareFilter: return self._arithmetic(other, lambda a, b: 0 if b == 0 else a - _divide(a, b) * b)
    def __bool__(self) -> bool: raise TypeError("compare a comparison filter with a value before using it")


def _divide(a, b):
    if isinstance(a, int) and isinstance(b, int):
        quotient = abs(a) // abs(b)
        return -quotient if (a < 0) != (b < 0) else quotient
    return a / b


class BestFilter:
    """Picks the better of two units; `Lowest` and `Highest` build it for `getBestUnit`."""

    def __init__(self, predicate: Callable[[Unit, Unit], Unit]) -> None: self.predicate = predicate
    def __call__(self, first: Unit, second: Unit) -> Unit: return self.predicate(first, second)
    def __and__(self, other) -> BestFilter: return BestFilter(lambda first, second: other(self(first, second)))


def Lowest(predicate: Callable[[Unit], object]) -> BestFilter:
    return BestFilter(lambda a, b: b if predicate(b) < predicate(a) else a)


def Highest(predicate: Callable[[Unit], object]) -> BestFilter:
    return BestFilter(lambda a, b: b if predicate(b) > predicate(a) else a)


UnitFilter = UnaryFilter
BestUnitFilter = BestFilter


def matches(unit: Unit, predicate) -> bool:
    return predicate is None or (isinstance(predicate, UnaryFilter) and not predicate.isValid()) or bool(predicate(unit))


def _player_stat(unit, stat):
    player = unit.getPlayer()
    return 0 if player is None else getattr(player, stat)(unit.getType())


def _percent(unit, value, maximum):
    maximum = maximum(unit)
    return _divide(value(unit) * 100, maximum) if maximum else 0


Filter = SimpleNamespace()

Filter.IsTransport = UnaryFilter(lambda u: u.getType().spaceProvided() > 0  and  u.getType() != 125)
Filter.CanProduce = UnaryFilter(lambda u: u.getType().canProduce())
Filter.CanAttack = UnaryFilter(lambda u: u.getType().canAttack())
Filter.CanMove = UnaryFilter(lambda u: u.getType().canMove())
Filter.IsFlyer = UnaryFilter(lambda u: u.getType().isFlyer())
Filter.IsFlying = UnaryFilter(lambda u: u.isFlying())
Filter.RegeneratesHP = UnaryFilter(lambda u: u.getType().regeneratesHP())
Filter.IsSpellcaster = UnaryFilter(lambda u: u.getType().isSpellcaster())
Filter.HasPermanentCloak = UnaryFilter(lambda u: u.getType().hasPermanentCloak())
Filter.IsOrganic = UnaryFilter(lambda u: u.getType().isOrganic())
Filter.IsMechanical = UnaryFilter(lambda u: u.getType().isMechanical())
Filter.IsRobotic = UnaryFilter(lambda u: u.getType().isRobotic())
Filter.IsDetector = UnaryFilter(lambda u: u.getType().isDetector())
Filter.IsResourceContainer = UnaryFilter(lambda u: u.getType().isResourceContainer())
Filter.IsResourceDepot = UnaryFilter(lambda u: u.getType().isResourceDepot())
Filter.IsRefinery = UnaryFilter(lambda u: u.getType().isRefinery())
Filter.IsWorker = UnaryFilter(lambda u: u.getType().isWorker())
Filter.RequiresPsi = UnaryFilter(lambda u: u.getType().requiresPsi())
Filter.RequiresCreep = UnaryFilter(lambda u: u.getType().requiresCreep())
Filter.IsBurrowable = UnaryFilter(lambda u: u.getType().isBurrowable())
Filter.IsCloakable = UnaryFilter(lambda u: u.getType().isCloakable())
Filter.IsBuilding = UnaryFilter(lambda u: u.getType().isBuilding())
Filter.IsAddon = UnaryFilter(lambda u: u.getType().isAddon())
Filter.IsFlyingBuilding = UnaryFilter(lambda u: u.getType().isFlyingBuilding())
Filter.IsNeutral = UnaryFilter(lambda u: u.getType().isNeutral())
Filter.IsHero = UnaryFilter(lambda u: u.getType().isHero())
Filter.IsPowerup = UnaryFilter(lambda u: u.getType().isPowerup())
Filter.IsBeacon = UnaryFilter(lambda u: u.getType().isBeacon())
Filter.IsFlagBeacon = UnaryFilter(lambda u: u.getType().isFlagBeacon())
Filter.IsSpecialBuilding = UnaryFilter(lambda u: u.getType().isSpecialBuilding())
Filter.IsSpell = UnaryFilter(lambda u: u.getType().isSpell())
Filter.ProducesLarva = UnaryFilter(lambda u: u.getType().producesLarva())
Filter.IsMineralField = UnaryFilter(lambda u: u.getType().isMineralField())
Filter.IsCritter = UnaryFilter(lambda u: u.getType().isCritter())
Filter.CanBuildAddon = UnaryFilter(lambda u: u.getType().canBuildAddon())
Filter.HP = CompareFilter(lambda u: u.getHitPoints())
Filter.MaxHP = CompareFilter(lambda u: u.getType().maxHitPoints())
Filter.Shields = CompareFilter(lambda u: u.getShields())
Filter.MaxShields = CompareFilter(lambda u: u.getType().maxShields())
Filter.Energy = CompareFilter(lambda u: u.getEnergy())
Filter.MaxEnergy = CompareFilter(lambda u: _player_stat(u, "maxEnergy"))
Filter.Armor = CompareFilter(lambda u: _player_stat(u, "armor"))
Filter.ArmorUpgrade = CompareFilter(lambda u: u.getType().armorUpgrade())
Filter.MineralPrice = CompareFilter(lambda u: u.getType().mineralPrice())
Filter.GasPrice = CompareFilter(lambda u: u.getType().gasPrice())
Filter.BuildTime = CompareFilter(lambda u: u.getType().buildTime())
Filter.SupplyRequired = CompareFilter(lambda u: u.getType().supplyRequired())
Filter.SupplyProvided = CompareFilter(lambda u: u.getType().supplyProvided())
Filter.SpaceRequired = CompareFilter(lambda u: u.getType().spaceRequired())
Filter.SpaceRemaining = CompareFilter(lambda u: u.getSpaceRemaining())
Filter.SpaceProvided = CompareFilter(lambda u: u.getType().spaceProvided())
Filter.BuildScore = CompareFilter(lambda u: u.getType().buildScore())
Filter.DestroyScore = CompareFilter(lambda u: u.getType().destroyScore())
Filter.TopSpeed = CompareFilter(lambda u: _player_stat(u, "topSpeed"))
Filter.SightRange = CompareFilter(lambda u: _player_stat(u, "sightRange"))
Filter.MaxWeaponCooldown = CompareFilter(lambda u: _player_stat(u, "weaponDamageCooldown"))
Filter.SizeType = CompareFilter(lambda u: u.getType().size())
Filter.GroundWeapon = CompareFilter(lambda u: u.getType().groundWeapon())
Filter.AirWeapon = CompareFilter(lambda u: u.getType().airWeapon())
Filter.GetType = CompareFilter(lambda u: u.getType())
Filter.GetRace = CompareFilter(lambda u: u.getType().getRace())
Filter.GetPlayer = CompareFilter(lambda u: u.getPlayer())
Filter.Resources = CompareFilter(lambda u: u.getResources())
Filter.ResourceGroup = CompareFilter(lambda u: u.getResourceGroup())
Filter.AcidSporeCount = CompareFilter(lambda u: u.getAcidSporeCount())
Filter.InterceptorCount = CompareFilter(lambda u: u.getInterceptorCount())
Filter.ScarabCount = CompareFilter(lambda u: u.getScarabCount())
Filter.SpiderMineCount = CompareFilter(lambda u: u.getSpiderMineCount())
Filter.WeaponCooldown = CompareFilter(lambda u: u.getGroundWeaponCooldown())
Filter.SpellCooldown = CompareFilter(lambda u: u.getSpellCooldown())
Filter.DefenseMatrixPoints = CompareFilter(lambda u: u.getDefenseMatrixPoints())
Filter.DefenseMatrixTime = CompareFilter(lambda u: u.getDefenseMatrixTimer())
Filter.EnsnareTime = CompareFilter(lambda u: u.getEnsnareTimer())
Filter.IrradiateTime = CompareFilter(lambda u: u.getIrradiateTimer())
Filter.LockdownTime = CompareFilter(lambda u: u.getLockdownTimer())
Filter.MaelstromTime = CompareFilter(lambda u: u.getMaelstromTimer())
Filter.OrderTime = CompareFilter(lambda u: u.getOrderTimer())
Filter.PlagueTimer = CompareFilter(lambda u: u.getPlagueTimer())
Filter.RemoveTime = CompareFilter(lambda u: u.getRemoveTimer())
Filter.StasisTime = CompareFilter(lambda u: u.getStasisTimer())
Filter.StimTime = CompareFilter(lambda u: u.getStimTimer())
Filter.BuildType = CompareFilter(lambda u: u.getBuildType())
Filter.RemainingBuildTime = CompareFilter(lambda u: u.getRemainingBuildTime())
Filter.RemainingTrainTime = CompareFilter(lambda u: u.getRemainingTrainTime())
Filter.Target = CompareFilter(lambda u: u.getTarget())
Filter.CurrentOrder = CompareFilter(lambda u: u.getOrder())
Filter.SecondaryOrder = CompareFilter(lambda u: u.getSecondaryOrder())
Filter.OrderTarget = CompareFilter(lambda u: u.getOrderTarget())
Filter.GetLeft = CompareFilter(lambda u: u.getLeft())
Filter.GetTop = CompareFilter(lambda u: u.getTop())
Filter.GetRight = CompareFilter(lambda u: u.getRight())
Filter.GetBottom = CompareFilter(lambda u: u.getBottom())
Filter.Exists = UnaryFilter(lambda u: u.exists())
Filter.IsAttacking = UnaryFilter(lambda u: u.isAttacking())
Filter.IsBeingConstructed = UnaryFilter(lambda u: u.isBeingConstructed())
Filter.IsBeingGathered = UnaryFilter(lambda u: u.isBeingGathered())
Filter.IsBeingHealed = UnaryFilter(lambda u: u.isBeingHealed())
Filter.IsBlind = UnaryFilter(lambda u: u.isBlind())
Filter.IsBraking = UnaryFilter(lambda u: u.isBraking())
Filter.IsBurrowed = UnaryFilter(lambda u: u.isBurrowed())
Filter.IsCarryingGas = UnaryFilter(lambda u: u.isCarryingGas())
Filter.IsCarryingMinerals = UnaryFilter(lambda u: u.isCarryingMinerals())
Filter.IsCarryingSomething = UnaryFilter(lambda u: u.isCarryingMinerals()  or  u.isCarryingGas())
Filter.IsCloaked = UnaryFilter(lambda u: u.isCloaked())
Filter.IsCompleted = UnaryFilter(lambda u: u.isCompleted())
Filter.IsConstructing = UnaryFilter(lambda u: u.isConstructing())
Filter.IsDefenseMatrixed = UnaryFilter(lambda u: u.isDefenseMatrixed())
Filter.IsDetected = UnaryFilter(lambda u: u.isDetected())
Filter.IsEnsnared = UnaryFilter(lambda u: u.isEnsnared())
Filter.IsFollowing = UnaryFilter(lambda u: u.isFollowing())
Filter.IsGatheringGas = UnaryFilter(lambda u: u.isGatheringGas())
Filter.IsGatheringMinerals = UnaryFilter(lambda u: u.isGatheringMinerals())
Filter.IsHallucination = UnaryFilter(lambda u: u.isHallucination())
Filter.IsHoldingPosition = UnaryFilter(lambda u: u.isHoldingPosition())
Filter.IsIdle = UnaryFilter(lambda u: u.isIdle())
Filter.IsInterruptible = UnaryFilter(lambda u: u.isInterruptible())
Filter.IsInvincible = UnaryFilter(lambda u: u.isInvincible())
Filter.IsIrradiated = UnaryFilter(lambda u: u.isIrradiated())
Filter.IsLifted = UnaryFilter(lambda u: u.isLifted())
Filter.IsLoaded = UnaryFilter(lambda u: u.isLoaded())
Filter.IsLockedDown = UnaryFilter(lambda u: u.isLockedDown())
Filter.IsMaelstrommed = UnaryFilter(lambda u: u.isMaelstrommed())
Filter.IsMorphing = UnaryFilter(lambda u: u.isMorphing())
Filter.IsMoving = UnaryFilter(lambda u: u.isMoving())
Filter.IsParasited = UnaryFilter(lambda u: u.isParasited())
Filter.IsPatrolling = UnaryFilter(lambda u: u.isPatrolling())
Filter.IsPlagued = UnaryFilter(lambda u: u.isPlagued())
Filter.IsRepairing = UnaryFilter(lambda u: u.isRepairing())
Filter.IsResearching = UnaryFilter(lambda u: u.isResearching())
Filter.IsSieged = UnaryFilter(lambda u: u.isSieged())
Filter.IsStartingAttack = UnaryFilter(lambda u: u.isStartingAttack())
Filter.IsStasised = UnaryFilter(lambda u: u.isStasised())
Filter.IsStimmed = UnaryFilter(lambda u: u.isStimmed())
Filter.IsStuck = UnaryFilter(lambda u: u.isStuck())
Filter.IsTraining = UnaryFilter(lambda u: u.isTraining())
Filter.IsUnderAttack = UnaryFilter(lambda u: u.isUnderAttack())
Filter.IsUnderDarkSwarm = UnaryFilter(lambda u: u.isUnderDarkSwarm())
Filter.IsUnderDisruptionWeb = UnaryFilter(lambda u: u.isUnderDisruptionWeb())
Filter.IsUnderStorm = UnaryFilter(lambda u: u.isUnderStorm())
Filter.IsPowered = UnaryFilter(lambda u: u.isPowered())
Filter.IsVisible = UnaryFilter(lambda u: u.isVisible())
Filter.HP_Percent = CompareFilter(lambda u: _percent(u, Filter.HP, Filter.MaxHP))
Filter.Shields_Percent = CompareFilter(lambda u: _percent(u, Filter.Shields, Filter.MaxShields))
Filter.Energy_Percent = CompareFilter(lambda u: _percent(u, Filter.Energy, Filter.MaxEnergy))
Filter.IsEnemy = UnaryFilter(lambda u: u.game.self() is not None and u.game.self().isEnemy(u.getPlayer()))
Filter.IsAlly = UnaryFilter(lambda u: u.game.self() is not None and u.game.self().isAlly(u.getPlayer()))
Filter.IsOwned = UnaryFilter(lambda u: u.game.self() is not None and u.game.self() is u.getPlayer())
