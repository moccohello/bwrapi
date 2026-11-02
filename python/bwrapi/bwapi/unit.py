"""BWAPI Unit over the GameData UnitData row the server fills each frame.

Getters read the shared memory directly (UnitShared.cpp); the derived queries
are the shared BWAPILIB ones (Unit.cpp); `can*` follow Templates.h, which the
official client evaluates locally from the same data.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from . import _latcom
from .containers import Unitset
from .filters import matches as _matches_predicate
from .interface import Interface
from .orders import OrderId
from .position import Position, TilePosition
from .types import (Errors, Order, Orders, Races, TechType, TechTypes, UnitCommandType, UnitType, UpgradeType,
                    WeaponType)
from .unit_command import UnitCommand

if TYPE_CHECKING:
    from .game import Game
    from .player import Player
    from .region import Region

_INT_MAX = 2_147_483_647
_NO_TARGET = object()
_COMMAND_TYPE_QUERIES = (
    "canAttackMove", "canAttackUnit", "canBuild", "canBuildAddon", "canTrain", "canMorph",
    "canResearch", "canUpgrade", "canSetRallyPosition", "canSetRallyUnit", "canMove", "canPatrol",
    "canHoldPosition", "canStop", "canFollow", "canGather", "canReturnCargo", "canRepair",
    "canBurrow", "canUnburrow", "canCloak", "canDecloak", "canSiege", "canUnsiege", "canLift",
    "canLand", "canLoad", "canUnload", "canUnloadAll", "canUnloadAllPosition",
    "canRightClickPosition", "canRightClickUnit", "canHaltConstruction", "canCancelConstruction",
    "canCancelAddon", "canCancelTrain", "canCancelTrainSlot", "canCancelMorph", "canCancelResearch",
    "canCancelUpgrade", "canUseTechWithOrWithoutTarget", "canUseTechWithOrWithoutTarget",
    "canUseTechWithOrWithoutTarget", "canPlaceCOP",
)
_UNGROUPED_COMMAND_TYPES = frozenset((2, 3, 6, 7, 8, 9, 24, 25, 27, 28, 33, 34, 35, 36, 38, 39, 43))
_GROUPED_QUERY_TYPES = frozenset((0, 1, 10, 11, 30, 31))
_GAS_ORDERS = frozenset((Orders.Harvest1, Orders.Harvest2, Orders.MoveToGas, Orders.WaitForGas,
                         Orders.HarvestGas, Orders.ReturnGas, Orders.ResetCollision))
_MINERAL_ORDERS = frozenset((Orders.Harvest1, Orders.Harvest2, Orders.MoveToMinerals, Orders.WaitForMinerals,
                             Orders.MiningMinerals, Orders.ReturnMinerals, Orders.ResetCollision))


def _to_position(value) -> Position:
    if isinstance(value, Position):
        return value
    if isinstance(value, tuple):
        return Position(*value)
    return Position(value)


def _to_tile_position(value) -> TilePosition:
    if isinstance(value, TilePosition):
        return value
    if isinstance(value, tuple):
        return TilePosition(*value)
    return TilePosition(value)


class Unit(Interface):
    """One of the 10000 BWAPI unit slots of a Game; the handle lives as long as the Game."""

    __slots__ = ("game", "_id", "_d", "_initial_type", "_initial_position", "_initial_resources",
                 "_initial_hit_points", "_last_command_frame", "_last_command", "_connected", "_loaded")

    def __init__(self, game: Game, unit_id: int) -> None:
        self.game = game
        self._id = unit_id
        self._d = game._data.units[unit_id]
        self._clear()

    def _clear(self) -> None:
        self._initial_type = UnitType()
        self._initial_resources = 0
        self._initial_hit_points = 0
        self._initial_position = Position(32000, 32032)
        self._last_command_frame = 0
        self._last_command = UnitCommand()
        self._connected = Unitset()
        self._loaded = Unitset()
        self._client_info = None
        self._interface_events = None

    def _save_initial_state(self) -> None:
        self._initial_type = self.getType()
        self._initial_position = self.getPosition()
        self._initial_resources = self.getResources()
        self._initial_hit_points = self.getHitPoints()

    def __repr__(self) -> str:
        return f"Unit({self._id}, {self.getType()})"

    # ---- UnitShared.cpp: values of the UnitData row --------------------------------------------
    def getID(self) -> int: return self._id
    def getReplayID(self) -> int: return self._d.replayID
    def getPlayer(self) -> Player | None: return self.game.getPlayer(self._d.player)
    def getType(self) -> UnitType: return UnitType(self._d.type)
    def getPosition(self) -> Position: return Position(self._d.positionX, self._d.positionY)
    def getAngle(self) -> float: return self._d.angle
    def getVelocityX(self) -> float: return self._d.velocityX
    def getVelocityY(self) -> float: return self._d.velocityY
    def getHitPoints(self) -> int: return self._d.hitPoints
    def getShields(self) -> int: return self._d.shields
    def getEnergy(self) -> int: return self._d.energy
    def getResources(self) -> int: return self._d.resources
    def getResourceGroup(self) -> int: return self._d.resourceGroup
    def getLastCommandFrame(self) -> int: return self._last_command_frame
    def getLastCommand(self) -> UnitCommand: return UnitCommand(self._last_command)
    def getLastAttackingPlayer(self) -> Player | None: return self.game.getPlayer(self._d.lastAttackerPlayer)
    def getInitialType(self) -> UnitType: return self._initial_type
    def getInitialPosition(self) -> Position: return Position(self._initial_position)

    def getInitialTilePosition(self) -> TilePosition:
        if self._initial_position == Position(32000, 32032):
            return TilePosition(1000, 1001)
        size = self._initial_type.tileSize()
        return TilePosition(self._initial_position - Position(size.x * 32, size.y * 32) / 2)

    def getInitialHitPoints(self) -> int: return self._initial_hit_points
    def getInitialResources(self) -> int: return self._initial_resources
    def getKillCount(self) -> int: return self._d.killCount
    def getAcidSporeCount(self) -> int: return self._d.acidSporeCount
    def getInterceptorCount(self) -> int: return self._d.interceptorCount
    def getScarabCount(self) -> int: return self._d.scarabCount
    def getSpiderMineCount(self) -> int: return self._d.spiderMineCount
    def getGroundWeaponCooldown(self) -> int: return self._d.groundWeaponCooldown
    def getAirWeaponCooldown(self) -> int: return self._d.airWeaponCooldown
    def getSpellCooldown(self) -> int: return self._d.spellCooldown
    def getDefenseMatrixPoints(self) -> int: return self._d.defenseMatrixPoints
    def getDefenseMatrixTimer(self) -> int: return self._d.defenseMatrixTimer
    def getEnsnareTimer(self) -> int: return self._d.ensnareTimer
    def getIrradiateTimer(self) -> int: return self._d.irradiateTimer
    def getLockdownTimer(self) -> int: return self._d.lockdownTimer
    def getMaelstromTimer(self) -> int: return self._d.maelstromTimer
    def getOrderTimer(self) -> int: return self._d.orderTimer
    def getPlagueTimer(self) -> int: return self._d.plagueTimer
    def getRemoveTimer(self) -> int: return self._d.removeTimer
    def getStasisTimer(self) -> int: return self._d.stasisTimer
    def getStimTimer(self) -> int: return self._d.stimTimer
    def getBuildType(self) -> UnitType: return UnitType(self._d.buildType)

    def getTrainingQueue(self) -> list[UnitType]:
        data = self._d
        return [UnitType(data.trainingQueue[i]) for i in range(data.trainingQueueCount)]

    def getTech(self) -> TechType: return TechType(self._d.tech)
    def getUpgrade(self) -> UpgradeType: return UpgradeType(self._d.upgrade)
    def getRemainingBuildTime(self) -> int: return self._d.remainingBuildTime
    def getRemainingTrainTime(self) -> int: return self._d.remainingTrainTime
    def getRemainingResearchTime(self) -> int: return self._d.remainingResearchTime
    def getRemainingUpgradeTime(self) -> int: return self._d.remainingUpgradeTime
    def getBuildUnit(self) -> Unit | None: return self.game.getUnit(self._d.buildUnit)
    def getTarget(self) -> Unit | None: return self.game.getUnit(self._d.target)
    def getTargetPosition(self) -> Position: return Position(self._d.targetPositionX, self._d.targetPositionY)
    def getOrder(self) -> Order: return Order(self._d.order)
    def getOrderTarget(self) -> Unit | None: return self.game.getUnit(self._d.orderTarget)

    def getOrderTargetPosition(self) -> Position:
        return Position(self._d.orderTargetPositionX, self._d.orderTargetPositionY)

    def getSecondaryOrder(self) -> Order: return Order(self._d.secondaryOrder)
    def getRallyPosition(self) -> Position: return Position(self._d.rallyPositionX, self._d.rallyPositionY)
    def getRallyUnit(self) -> Unit | None: return self.game.getUnit(self._d.rallyUnit)
    def getAddon(self) -> Unit | None: return self.game.getUnit(self._d.addon)
    def getNydusExit(self) -> Unit | None: return self.game.getUnit(self._d.nydusExit)
    def getPowerUp(self) -> Unit | None: return self.game.getUnit(self._d.powerUp)
    def getTransport(self) -> Unit | None: return self.game.getUnit(self._d.transport)
    def getLoadedUnits(self) -> Unitset: return Unitset(self._loaded)
    def getCarrier(self) -> Unit | None: return self.game.getUnit(self._d.carrier)

    def getInterceptors(self) -> Unitset:
        if self.getType() not in (72, 82):  # Protoss_Carrier, Hero_Gantrithor
            return Unitset()
        return Unitset(self._connected)

    def getHatchery(self) -> Unit | None: return self.game.getUnit(self._d.hatchery)

    def getLarva(self) -> Unitset:
        if not self.getType().producesLarva():
            return Unitset()
        return Unitset(self._connected)

    def exists(self) -> bool: return self._d.exists
    def hasNuke(self) -> bool: return self._d.hasNuke
    def isAccelerating(self) -> bool: return self._d.isAccelerating
    def isAttacking(self) -> bool: return self._d.isAttacking
    def isAttackFrame(self) -> bool: return self._d.isAttackFrame
    def isBeingGathered(self) -> bool: return self._d.isBeingGathered

    def isBeingHealed(self) -> bool:
        data = self._d
        return self.getType().getRace() == Races.Terran and data.isCompleted and data.hitPoints > data.lastHitPoints

    def isBlind(self) -> bool: return self._d.isBlind
    def isBraking(self) -> bool: return self._d.isBraking
    def isBurrowed(self) -> bool: return self._d.isBurrowed
    def isCarryingGas(self) -> bool: return self._d.carryResourceType == 1
    def isCarryingMinerals(self) -> bool: return self._d.carryResourceType == 2
    def isCloaked(self) -> bool: return self._d.isCloaked
    def isCompleted(self) -> bool: return self._d.isCompleted
    def isConstructing(self) -> bool: return self._d.isConstructing
    def isDetected(self) -> bool: return self._d.isDetected

    def isGatheringGas(self) -> bool:
        data = self._d
        if not data.isGathering or data.order not in _GAS_ORDERS:
            return False
        if data.order == Orders.ResetCollision:
            return data.carryResourceType == 1
        if data.order in (Orders.WaitForGas, Orders.HarvestGas, Orders.ReturnGas):
            return True
        player = self.getPlayer()
        for target in (self.getTarget(), self.getOrderTarget()):
            if (target is not None and target.exists() and target.isCompleted() and target.getPlayer() is player
                    and target.getType() != 188  # Resource_Vespene_Geyser
                    and (target.getType().isRefinery() or target.getType().isResourceDepot())):
                return True
        return False

    def isGatheringMinerals(self) -> bool:
        data = self._d
        if not data.isGathering or data.order not in _MINERAL_ORDERS:
            return False
        if data.order == Orders.ResetCollision:
            return data.carryResourceType == 2
        if data.order in (Orders.WaitForMinerals, Orders.MiningMinerals, Orders.ReturnMinerals):
            return True
        player = self.getPlayer()
        for target in (self.getTarget(), self.getOrderTarget()):
            if target is not None and target.exists() and (
                    target.getType().isMineralField()
                    or (target.isCompleted() and target.getPlayer() is player and target.getType().isResourceDepot())):
                return True
        return False

    def isHallucination(self) -> bool: return self._d.isHallucination
    def isIdle(self) -> bool: return self._d.isIdle
    def isInterruptible(self) -> bool: return self._d.isInterruptible
    def isInvincible(self) -> bool: return self._d.isInvincible
    def isLifted(self) -> bool: return self._d.isLifted
    def isMorphing(self) -> bool: return self._d.isMorphing
    def isMoving(self) -> bool: return self._d.isMoving
    def isParasited(self) -> bool: return self._d.isParasited
    def isSelected(self) -> bool: return self._d.isSelected
    def isStartingAttack(self) -> bool: return self._d.isStartingAttack
    def isStuck(self) -> bool: return self._d.isStuck
    def isTraining(self) -> bool: return self._d.isTraining
    def isUnderAttack(self) -> bool: return self._d.recentlyAttacked
    def isUnderDarkSwarm(self) -> bool: return self._d.isUnderDarkSwarm
    def isUnderDisruptionWeb(self) -> bool: return self._d.isUnderDWeb
    def isUnderStorm(self) -> bool: return self._d.isUnderStorm
    def isPowered(self) -> bool: return self._d.isPowered

    def isVisible(self, player: Player | None = None) -> bool:
        visible = self._d.isVisible
        if player is None:
            player = self.game.self()
            if player is None:
                return any(visible)
        player_id = player.getID()
        return 0 <= player_id < 9 and visible[player_id]

    # ---- Unit.cpp: queries shared by every BWAPI implementation --------------------------------
    def getTilePosition(self) -> TilePosition:
        kind = self.getType()
        data = self._d
        return TilePosition(abs(data.positionX - kind.tileWidth() * 32 // 2) // 32,
                            abs(data.positionY - kind.tileHeight() * 32 // 2) // 32)

    def getLeft(self) -> int: return self._d.positionX - self.getType().dimensionLeft()
    def getTop(self) -> int: return self._d.positionY - self.getType().dimensionUp()
    def getRight(self) -> int: return self._d.positionX + self.getType().dimensionRight()
    def getBottom(self) -> int: return self._d.positionY + self.getType().dimensionDown()

    def getDistance(self, target) -> int:
        if isinstance(target, Unit):
            return self._distance_to_unit(target)
        position = _to_position(target)
        if not self.exists() or not self.game._valid_position(position.x, position.y):
            return _INT_MAX
        x = self.getLeft() - position.x
        if x < 0:
            x = max(position.x - (self.getRight() + 1), 0)
        y = self.getTop() - position.y
        if y < 0:
            y = max(position.y - (self.getBottom() + 1), 0)
        return _approx_distance(x, y)

    def _distance_to_unit(self, target: Unit) -> int:
        if not self.exists() or target is None or not target.exists():
            return _INT_MAX
        if self is target:
            return 0
        x = self.getLeft() - (target.getRight() + 1)
        if x < 0:
            x = max(target.getLeft() - 1 - self.getRight(), 0)
        y = self.getTop() - (target.getBottom() + 1)
        if y < 0:
            y = max(target.getTop() - 1 - self.getBottom(), 0)
        return _approx_distance(x, y)

    def hasPath(self, target) -> bool:
        game = self.game
        game.setLastError()
        if target is None:
            return game.setLastError(Errors.Invalid_Parameter)
        if isinstance(target, Unit):
            if not target.exists():
                return game.setLastError(Errors.Unit_Not_Visible)
            target = target.getPosition()
        target = _to_position(target)
        if not game._valid_position(target.x, target.y):
            return game.setLastError(Errors.Invalid_Parameter)
        if self.isFlying():
            return True
        if not self.exists():
            return game.setLastError(Errors.Unit_Not_Visible)
        left, top, right, bottom = self.getLeft(), self.getTop(), self.getRight(), self.getBottom()
        return any(game.hasPath(source, target) for source in (
            self.getPosition(), Position(left, top), Position(right, top), Position(left, bottom),
            Position(right, bottom)))

    def getRegion(self) -> Region | None: return self.game.getRegionAt(self.getPosition())

    def getUnitsInRadius(self, radius: int, pred=None) -> Unitset:
        if not self.exists():
            return Unitset()
        return self.game.getUnitsInRectangle(
            self.getLeft() - radius, self.getTop() - radius, self.getRight() + radius, self.getBottom() + radius,
            lambda unit: unit is not self and self.getDistance(unit) <= radius and _matches_predicate(unit, pred))

    def getClosestUnit(self, pred=None, radius: int = 999999) -> Unit | None:
        if not self.exists():
            return None
        return self.game.getClosestUnitInRectangle(
            self.getPosition(),
            lambda unit: unit is not self and self.getDistance(unit) <= radius and _matches_predicate(unit, pred),
            self.getLeft() - radius, self.getTop() - radius, self.getRight() + radius, self.getBottom() + radius)

    def isBeingConstructed(self) -> bool:
        if self.isMorphing():
            return True
        if self.isCompleted():
            return False
        if self.getType().getRace() != Races.Terran:
            return True
        return self.getBuildUnit() is not None

    def isDefenseMatrixed(self) -> bool: return self._d.defenseMatrixTimer != 0
    def isEnsnared(self) -> bool: return self._d.ensnareTimer != 0
    def isFollowing(self) -> bool: return self._d.order == Orders.Follow
    def isFlying(self) -> bool: return self.getType().isFlyer() or self._d.isLifted
    def isHoldingPosition(self) -> bool: return self._d.order == Orders.HoldPosition
    def isIrradiated(self) -> bool: return self._d.irradiateTimer != 0
    def isLoaded(self) -> bool: return self.getTransport() is not None
    def isLockedDown(self) -> bool: return self._d.lockdownTimer != 0
    def isMaelstrommed(self) -> bool: return self._d.maelstromTimer != 0
    def isPatrolling(self) -> bool: return self._d.order == Orders.Patrol
    def isPlagued(self) -> bool: return self._d.plagueTimer != 0
    def isRepairing(self) -> bool: return self._d.order == Orders.Repair
    def isResearching(self) -> bool: return self._d.order == Orders.ResearchTech
    def isSieged(self) -> bool: return self._d.type in (30, 25)  # Siege Mode, Edmund Duke Siege Mode
    def isStasised(self) -> bool: return self._d.stasisTimer != 0
    def isStimmed(self) -> bool: return self._d.stimTimer != 0
    def isUpgrading(self) -> bool: return self._d.order == Orders.Upgrade

    def getSpaceRemaining(self) -> int:
        space = self.getType().spaceProvided()
        for unit in self._loaded:
            space -= unit.getType().spaceRequired()
        return max(space, 0)

    # ---- commands --------------------------------------------------------------------------------
    def issueCommand(self, command: UnitCommand) -> bool:
        """UnitImpl::issueCommand: check, apply latency compensation and queue the GameData command."""
        if not self.canIssueCommand(command):
            return False
        command = UnitCommand(command)
        command.unit = self
        # Train or morph on a hatchery goes to one of its larvae.
        if (int(command.type) in (4, 5) and self.getType().producesLarva()
                and command.getUnitType().whatBuilds()[0] == 35):  # Zerg_Larva
            for larva in self.getLarva():
                if not larva.isConstructing() and larva.isCompleted() and larva.canCommand():
                    command.unit = larva
                    break
            else:
                return False
        game = self.game
        target = command.target
        game._add_unit_command(int(command.type), command.unit.getID(),
                               -1 if target is None else target.getID(), command.x, command.y, command.extra)
        _latcom.execute(game, command)
        self._last_command_frame = game.getFrameCount()
        self._last_command = command
        return True

    def _issue(self, factory: str, *args) -> bool:
        return self.issueCommand(getattr(UnitCommand, factory)(self, *args))

    def attack(self, target, shiftQueueCommand: bool = False) -> bool:
        return self._issue("attack", target, shiftQueueCommand)

    def build(self, type, target=None) -> bool:
        if target is None or _to_tile_position(target) == TilePosition(1000, 1001):
            return self.train(type)
        return self._issue("build", target, type)

    def buildAddon(self, type) -> bool: return self._issue("buildAddon", type)

    def train(self, type=None) -> bool:
        type = UnitType(type)
        if type == 228:  # None: carriers and reavers train their ammunition
            if self.getType() in (72, 82):
                type = UnitType(73)
            elif self.getType() in (83, 81):
                type = UnitType(85)
        return self._issue("train", type)

    def morph(self, type) -> bool: return self._issue("morph", type)
    def research(self, tech) -> bool: return self._issue("research", tech)
    def upgrade(self, upgrade) -> bool: return self._issue("upgrade", upgrade)
    def setRallyPoint(self, target) -> bool: return self._issue("setRallyPoint", target)
    def move(self, target, shiftQueueCommand: bool = False) -> bool: return self._issue("move", target, shiftQueueCommand)
    def patrol(self, target, shiftQueueCommand: bool = False) -> bool: return self._issue("patrol", target, shiftQueueCommand)
    def holdPosition(self, shiftQueueCommand: bool = False) -> bool: return self._issue("holdPosition", shiftQueueCommand)
    def stop(self, shiftQueueCommand: bool = False) -> bool: return self._issue("stop", shiftQueueCommand)
    def follow(self, target, shiftQueueCommand: bool = False) -> bool: return self._issue("follow", target, shiftQueueCommand)
    def gather(self, target, shiftQueueCommand: bool = False) -> bool: return self._issue("gather", target, shiftQueueCommand)
    def returnCargo(self, shiftQueueCommand: bool = False) -> bool: return self._issue("returnCargo", shiftQueueCommand)
    def repair(self, target, shiftQueueCommand: bool = False) -> bool: return self._issue("repair", target, shiftQueueCommand)
    def burrow(self) -> bool: return self._issue("burrow")
    def unburrow(self) -> bool: return self._issue("unburrow")
    def cloak(self) -> bool: return self._issue("cloak")
    def decloak(self) -> bool: return self._issue("decloak")
    def siege(self) -> bool: return self._issue("siege")
    def unsiege(self) -> bool: return self._issue("unsiege")
    def lift(self) -> bool: return self._issue("lift")
    def land(self, target) -> bool: return self._issue("land", target)
    def load(self, target, shiftQueueCommand: bool = False) -> bool: return self._issue("load", target, shiftQueueCommand)
    def unload(self, target) -> bool: return self._issue("unload", target)

    def unloadAll(self, target=None, shiftQueueCommand: bool = False) -> bool:
        if type(target) is bool:
            target, shiftQueueCommand = None, target
        return self._issue("unloadAll", target, shiftQueueCommand)

    def rightClick(self, target, shiftQueueCommand: bool = False) -> bool:
        return self._issue("rightClick", target, shiftQueueCommand)

    def haltConstruction(self) -> bool: return self._issue("haltConstruction")
    def cancelConstruction(self) -> bool: return self._issue("cancelConstruction")
    def cancelAddon(self) -> bool: return self._issue("cancelAddon")
    def cancelTrain(self, slot: int = -2) -> bool: return self._issue("cancelTrain", slot)
    def cancelMorph(self) -> bool: return self._issue("cancelMorph")
    def cancelResearch(self) -> bool: return self._issue("cancelResearch")
    def cancelUpgrade(self) -> bool: return self._issue("cancelUpgrade")

    def useTech(self, tech, target=None) -> bool:
        if target is None:
            return self._issue("useTech", tech)
        return self._issue("useTech", tech, target)

    def placeCOP(self, target) -> bool: return self._issue("placeCOP", target)

    # ---- Templates.h --------------------------------------------------------------------------
    def _can_order_state(self, expected_order: int, check_commandibility: bool) -> bool:
        if not self._can_query_start(check_commandibility):
            return False
        if self._d.order != expected_order:
            return self.game.setLastError(Errors.Incompatible_State)
        return True

    def canIssueCommand(self, command: UnitCommand, checkCanUseTechPositionOnPositions=True,
                        checkCanUseTechUnitOnUnits=True, checkCanBuildUnitType=True, checkCanTargetUnit=True,
                        checkCanIssueCommandType=True, checkCommandibility=True) -> bool:
        if int(command.type) >= 44:
            return False
        if not self._can_query_start(checkCommandibility):
            return False
        if checkCanIssueCommandType and not self.canIssueCommandType(command.type, False):
            return False
        return self._can_command_target(command, checkCanUseTechPositionOnPositions, checkCanUseTechUnitOnUnits,
                                        checkCanBuildUnitType, checkCanTargetUnit, False)

    def canIssueCommandGrouped(self, command: UnitCommand, checkCanUseTechPositionOnPositions=True,
                               checkCanUseTechUnitOnUnits=True, checkCanTargetUnit=True, checkCanIssueCommandType=True,
                               checkCommandibilityGrouped=True, checkCommandibility=True) -> bool:
        if int(command.type) >= 44:
            return False
        if not self._can_query_start(checkCommandibility):
            return False
        if checkCommandibilityGrouped and not self.canCommandGrouped(False):
            return False
        if checkCanIssueCommandType and not self.canIssueCommandTypeGrouped(command.type, False, False):
            return False
        return self._can_command_target(command, checkCanUseTechPositionOnPositions, checkCanUseTechUnitOnUnits,
                                        False, checkCanTargetUnit, True)

    def _can_command_target(self, command, tech_position, tech_unit, build_type, target_check, grouped):
        ct = int(command.type)
        if grouped and ct in _UNGROUPED_COMMAND_TYPES:
            return False
        if ct in (1, 9, 14, 15, 17, 26, 27, 31, 42):
            target = command.target
            if ct == 27:
                return self.canUnload(target, target_check, False, False, False)
            if ct == 42:
                return self.canUseTechUnit(command.getTechType(), target, target_check, tech_unit, False, False)
            method = _COMMAND_TYPE_QUERIES[ct]
            if grouped and ct in (1, 31):
                return getattr(self, method + "Grouped")(target, target_check, False, False, False)
            return getattr(self, method)(target, target_check, False, False)
        if ct == 2:
            return self.canBuild(command.getUnitType(), command.getTargetTilePosition(), build_type, False, False)
        if ct in (3, 4, 5):
            return getattr(self, _COMMAND_TYPE_QUERIES[ct])(command.getUnitType(), False, False)
        if ct == 6:
            return self.game.canResearch(command.getTechType(), self, False)
        if ct == 7:
            return self.game.canUpgrade(command.getUpgradeType(), self, False)
        if ct in (25, 43):
            return getattr(self, _COMMAND_TYPE_QUERIES[ct])(command.getTargetTilePosition(), False, False)
        if ct == 29:
            return self.canUnloadAllPosition(command.getTargetPosition(), False, False)
        if ct == 36:
            return self.canCancelTrainSlot(command.extra, False, False)
        if ct == 40:
            return self.canUseTechWithoutTarget(command.getTechType(), False, False)
        if ct == 41:
            return self.canUseTechPosition(command.getTechType(), command.getTargetPosition(), tech_position,
                                           False, False)
        return True

    def canCommand(self) -> bool:
        self.game.setLastError()
        if self.getPlayer() is not self.game.self():
            return self.game.setLastError(Errors.Unit_Not_Owned)
        if not self.exists():
            return self.game.setLastError(Errors.Unit_Does_Not_Exist)
        if (self.isLockedDown() or self.isMaelstrommed() or self.isStasised()
                or not self.isPowered() or self.getOrder() == Orders.ZergBirth or self.isLoaded()):
            if not self.getType().producesLarva():
                return self.game.setLastError(Errors.Unit_Busy)
            for larva in self.getLarva():
                if larva.canCommand():
                    return self.game.setLastError()
            return self.game.setLastError(Errors.Unit_Busy)
        unit_type = self.getType()
        if unit_type in (73, 13, 33, 101):
            return self.game.setLastError(Errors.Incompatible_UnitType)
        if self.isCompleted() and (unit_type in (156, 109, 172, 14) or unit_type.isResourceContainer()
                or unit_type.isPowerup() or (unit_type.isSpecialBuilding() and not unit_type.isFlagBeacon())):
            return self.game.setLastError(Errors.Incompatible_State)
        if not self.isCompleted() and not unit_type.isBuilding() and not self.isMorphing():
            return self.game.setLastError(Errors.Incompatible_State)
        return self.game.setLastError()

    def canTargetUnit(self, targetUnit, checkCommandibility=True) -> bool:
        if not self._can_query_start(checkCommandibility):
            return False
        if targetUnit is None or targetUnit.game is not self.game or not targetUnit.exists():
            return self.game.setLastError(Errors.Unit_Does_Not_Exist)
        return targetUnit.isTargetable()

    def isTargetable(self) -> bool:
        if not self.exists():
            return self.game.setLastError(Errors.Unit_Does_Not_Exist)
        if not self.isVisible() and not self.game.isFlagEnabled(0):
            return False
        if (not self.isCompleted() and not self.getType().isBuilding()
                and not self.isMorphing() and self.getType() not in (68, 63)):
            return self.game.setLastError(Errors.Incompatible_State)
        if self.getType() in (33, 202, 105, 101):
            return self.game.setLastError(Errors.Incompatible_UnitType)
        return True

    def canAttackMove(self, checkCommandibility=True) -> bool:
        if not self._can_query_start(checkCommandibility):
            return False
        return (self.getType() == 34 or self.canAttackUnit(False)) and self.canMove(False)

    def canAttackMoveGrouped(self, checkCommandibilityGrouped=True, checkCommandibility=True) -> bool:
        if not self._can_query_start(checkCommandibility):
            return False
        if checkCommandibilityGrouped and not self.canCommandGrouped(False):
            return False
        if not self.getType().canMove() and self.getType() not in (30, 59, 97):
            return self.game.setLastError(Errors.Incompatible_UnitType)
        return True

    def canAttackUnit(self, target=_NO_TARGET, *flags, checkCanTargetUnit=True,
                      checkCanIssueCommandType=True, checkCommandibility=True) -> bool:
        targeted, (ct, ci, cc) = _query_flags(target, flags, (checkCanTargetUnit, checkCanIssueCommandType, checkCommandibility))
        if not self._can_query_start(cc):
            return False
        if not targeted:
            kind = self.getType()
            if not kind.isBuilding() and not self.isInterruptible():
                return self.game.setLastError(Errors.Unit_Busy)
            if kind.groundWeapon() == 130 and kind.airWeapon() == 130:
                if kind in (72, 82):
                    if self.getInterceptorCount() <= 0:
                        return self.game.setLastError(Errors.Unable_To_Hit)
                elif kind in (83, 81):
                    if self.getScarabCount() <= 0:
                        return self.game.setLastError(Errors.Unable_To_Hit)
                else:
                    return self.game.setLastError(Errors.Unable_To_Hit)
            if (kind == 103 and not self.isBurrowed()) or (kind != 103 and self.isBurrowed()):
                return self.game.setLastError(Errors.Unable_To_Hit)
            if not self.isCompleted():
                return self.game.setLastError(Errors.Incompatible_State)
            if self.getOrder() == Orders.ConstructingBuilding:
                return self.game.setLastError(Errors.Unit_Busy)
            return True
        if ci and not self.canAttackUnit(False):
            return False
        if ct and not self.canTargetUnit(target, False):
            return False
        target = self._query_target(target)
        if target.isInvincible():
            return self.game.setLastError(Errors.Unable_To_Hit)
        kind = self.getType()
        airborne = target.isFlying()
        weapon = kind.airWeapon() if airborne else kind.groundWeapon()
        if weapon == 130:
            if kind in (83, 81):
                if airborne:
                    # BWAPI 4.4 only sets the Error here and continues.
                    self.game.setLastError(Errors.Unable_To_Hit)
            elif kind not in (72, 82):
                return self.game.setLastError(Errors.Unable_To_Hit)
        if not kind.canMove() and not self.isInWeaponRange(target):
            return self.game.setLastError(Errors.Out_Of_Range)
        if kind == 103 and not self.isInWeaponRange(target):
            return self.game.setLastError(Errors.Out_Of_Range)
        return self.game.setLastError(Errors.Invalid_Parameter) if target is self else True

    def canAttackUnitGrouped(self, target=_NO_TARGET, *flags, checkCanTargetUnit=True,
                             checkCanIssueCommandTypeGrouped=True, checkCommandibilityGrouped=True, checkCommandibility=True) -> bool:
        targeted, (ct, ci, cg, cc) = _query_flags(target, flags, (checkCanTargetUnit, checkCanIssueCommandTypeGrouped,
                                                               checkCommandibilityGrouped, checkCommandibility), 2)
        if not self._can_query_start(cc):
            return False
        if cg and not self.canCommandGrouped(False):
            return False
        if not targeted:
            if not self.isInterruptible():
                return self.game.setLastError(Errors.Unit_Busy)
            kind = self.getType()
            if not kind.canMove() and kind != 30:
                return self.game.setLastError(Errors.Incompatible_UnitType)
            if not self.isCompleted():
                return self.game.setLastError(Errors.Incompatible_State)
            if (kind == 103 and not self.isBurrowed()) or (kind != 103 and self.isBurrowed()):
                return self.game.setLastError(Errors.Unable_To_Hit)
            if self.getOrder() == Orders.ConstructingBuilding:
                return self.game.setLastError(Errors.Unit_Busy)
            return True
        if ci and not self.canAttackUnitGrouped(False, False):
            return False
        if ct and not self.canTargetUnit(target, False):
            return False
        target = self._query_target(target)
        if target.isInvincible():
            return self.game.setLastError(Errors.Unable_To_Hit)
        kind = self.getType()
        if kind == 103 and not self.isInWeaponRange(target):
            return self.game.setLastError(Errors.Out_Of_Range)
        if kind == 45 and (target.getType() != 106 or target.getHitPoints() >= 750 or target.getHitPoints() <= 0):
            return self.game.setLastError(Errors.Invalid_Parameter)
        return self.game.setLastError(Errors.Invalid_Parameter) if target is self else True

    def canAttack(self, target=_NO_TARGET, *flags, checkCanTargetUnit=True,
                  checkCanIssueCommandType=True, checkCommandibility=True) -> bool:
        targeted, (ct, ci, cc) = _query_flags(target, flags, (checkCanTargetUnit, checkCanIssueCommandType, checkCommandibility))
        if not self._can_query_start(cc):
            return False
        if not targeted:
            return self.canAttackMove(False) or self.canAttackUnit(False)
        if target is None:
            return self.game.setLastError(Errors.Invalid_Parameter)
        if isinstance(target, Unit):
            return self.canAttackUnit(target, ct, ci, False)
        _to_position(target)
        return self.canAttackMove(False)

    def canAttackGrouped(self, target=_NO_TARGET, *flags, checkCanTargetUnit=True,
                         checkCanIssueCommandTypeGrouped=True, checkCommandibilityGrouped=True, checkCommandibility=True) -> bool:
        targeted, (ct, ci, cg, cc) = _query_flags(target, flags, (checkCanTargetUnit, checkCanIssueCommandTypeGrouped,
                                                               checkCommandibilityGrouped, checkCommandibility), 2)
        if not self._can_query_start(cc):
            return False
        if cg and not self.canCommandGrouped(False):
            return False
        if not targeted:
            return self.canAttackMoveGrouped(False, False) or self.canAttackUnitGrouped(False, False)
        if target is None:
            return self.game.setLastError(Errors.Invalid_Parameter)
        if isinstance(target, Unit):
            return self.canAttackUnitGrouped(target, ct, ci, False, False)
        _to_position(target)
        return self.canAttackMoveGrouped(False, False)

    def canBuild(self, unit_type=_NO_TARGET, *args, checkTargetUnitType=True,
                 checkCanIssueCommandType=True, checkCommandibility=True) -> bool:
        positioned = bool(args) and type(args[0]) is not bool
        position = args[0] if positioned else _NO_TARGET
        defaults = ((checkTargetUnitType, checkCanIssueCommandType, checkCommandibility) if positioned
                    else (checkCanIssueCommandType, checkCommandibility))
        typed, values = _query_flags(unit_type, args[1:] if positioned else args, defaults)
        ci, cc = values[-2:]
        if not self._can_query_start(cc):
            return False
        if not typed:
            if not self.getType().isBuilding() and not self.isInterruptible():
                return self.game.setLastError(Errors.Unit_Busy)
            if self.isConstructing() or not self.isCompleted() or (self.getType().isBuilding() and not self.isIdle()):
                return self.game.setLastError(Errors.Unit_Busy)
            return self.game.setLastError(Errors.Incompatible_UnitType) if self.isHallucination() else True
        if ci and not self.canBuild(False):
            return False
        kind = UnitType(unit_type)
        if positioned:
            if values[0] and not self.canBuild(kind, False, False):
                return False
            tile = _to_tile_position(position)
            if not (0 <= tile.x < self.game.mapWidth() and 0 <= tile.y < self.game.mapHeight()):
                return self.game.setLastError(Errors.Invalid_Tile_Position)
            return self.game.canBuildHere(tile, kind, self, True)
        if not self.game.canMake(kind, self):
            return False
        if not kind.isBuilding():
            return self.game.setLastError(Errors.Incompatible_UnitType)
        return self.game.setLastError(Errors.Incompatible_State) if self.getAddon() is not None else True

    def canBuildAddon(self, unit_type=_NO_TARGET, *flags, checkCanIssueCommandType=True, checkCommandibility=True) -> bool:
        typed, (ci, cc) = _query_flags(unit_type, flags, (checkCanIssueCommandType, checkCommandibility))
        if not self._can_query_start(cc):
            return False
        if not typed:
            if (self.isConstructing() or not self.isCompleted() or self.isLifted()
                    or (self.getType().isBuilding() and not self.isIdle())):
                return self.game.setLastError(Errors.Unit_Busy)
            if self.getAddon() is not None:
                return self.game.setLastError(Errors.Incompatible_State)
            return True if self.getType().canBuildAddon() else self.game.setLastError(Errors.Incompatible_UnitType)
        if ci and not self.canBuildAddon(False):
            return False
        kind = UnitType(unit_type)
        if not self.game.canMake(kind, self):
            return False
        if not kind.isAddon():
            return self.game.setLastError(Errors.Incompatible_UnitType)
        return self.game.canBuildHere(self.getTilePosition(), kind, self)

    def canTrain(self, unit_type=_NO_TARGET, *flags, checkCanIssueCommandType=True, checkCommandibility=True) -> bool:
        typed, (ci, cc) = _query_flags(unit_type, flags, (checkCanIssueCommandType, checkCommandibility))
        return self._can_produce(unit_type, typed, ci, cc, False)

    def canMorph(self, unit_type=_NO_TARGET, *flags, checkCanIssueCommandType=True, checkCommandibility=True) -> bool:
        typed, (ci, cc) = _query_flags(unit_type, flags, (checkCanIssueCommandType, checkCommandibility))
        return self._can_produce(unit_type, typed, ci, cc, True)

    def _can_produce(self, unit_type, typed, ci, cc, morph):
        if not self._can_query_start(cc):
            return False
        kind = self.getType()
        if not typed:
            if kind.producesLarva():
                if not self.isConstructing() and self.isCompleted() and (not morph or not kind.isBuilding() or self.isIdle()):
                    return self.game.setLastError()
                for larva in self.getLarva():
                    if not larva.isConstructing() and larva.isCompleted() and larva.canCommand():
                        return self.game.setLastError()
                return self.game.setLastError(Errors.Unit_Busy)
            if (self.isConstructing() or not self.isCompleted()
                    or (morph and kind.isBuilding() and not self.isIdle()) or (not morph and self.isLifted())):
                return self.game.setLastError(Errors.Unit_Busy)
            if morph:
                valid = kind in (38, 43, 143, 141, 131, 132, 133, 35)
            else:
                valid = kind.canProduce() or kind in (108, 38, 43, 143, 141, 35)
            if not valid or self.isHallucination():
                return self.game.setLastError(Errors.Incompatible_UnitType)
            return True
        query = self.canMorph if morph else self.canTrain
        if ci and not query(False):
            return False
        build_type = UnitType(unit_type)
        builder = self
        if kind.producesLarva():
            if build_type.whatBuilds()[0] == 35:
                builder = None
                for larva in self.getLarva():
                    if (larva.canMorph if morph else larva.canTrain)(True):
                        builder = larva
                        break
                if builder is None:
                    return self.game.setLastError(Errors.Unit_Does_Not_Exist)
            elif self.isConstructing() or not self.isCompleted() or (morph and kind.isBuilding() and not self.isIdle()):
                return self.game.setLastError(Errors.Unit_Busy)
        if not self.game.canMake(build_type, builder):
            return False
        if not morph and (build_type.isAddon() or (build_type.isBuilding() and not builder.getType().isBuilding())):
            return self.game.setLastError(Errors.Incompatible_UnitType)
        return self.game.setLastError(Errors.Incompatible_UnitType) if build_type in (35, 36, 59) else True

    def _query_target(self, target):
        if not isinstance(target, Unit) or target.game is not self.game:
            raise TypeError("a BWAPI target is required")
        return target

    def canSetRallyPoint(self, target=_NO_TARGET, *flags, checkCanTargetUnit=True,
                         checkCanIssueCommandType=True, checkCommandibility=True) -> bool:
        targeted, (ct, ci, cc) = _query_flags(target, flags, (checkCanTargetUnit, checkCanIssueCommandType, checkCommandibility))
        if not self._can_query_start(cc):
            return False
        if not targeted:
            return self.canSetRallyPosition(False) or self.canSetRallyUnit(False)
        if target is None:
            return self.game.setLastError(Errors.Invalid_Parameter)
        if isinstance(target, Unit):
            return self.canSetRallyUnit(target, ct, ci, False)
        _to_position(target)
        return self.canSetRallyPosition(False)

    def canSetRallyUnit(self, target=_NO_TARGET, *flags, checkCanTargetUnit=True,
                        checkCanIssueCommandType=True, checkCommandibility=True) -> bool:
        targeted, (ct, ci, cc) = _query_flags(target, flags, (checkCanTargetUnit, checkCanIssueCommandType, checkCommandibility))
        if not self._can_query_start(cc):
            return False
        if not targeted:
            return self.canSetRallyPosition(False)
        if ci and not self.canSetRallyUnit(False):
            return False
        return self.canTargetUnit(target, False) if ct else True

    def canFollow(self, target=_NO_TARGET, *flags, checkCanTargetUnit=True,
                  checkCanIssueCommandType=True, checkCommandibility=True) -> bool:
        targeted, (ct, ci, cc) = _query_flags(target, flags, (checkCanTargetUnit, checkCanIssueCommandType, checkCommandibility))
        if not self._can_query_start(cc):
            return False
        if not targeted:
            return self.canMove(False)
        if ci and not self.canFollow(False):
            return False
        if ct and not self.canTargetUnit(target, False):
            return False
        return self.game.setLastError(Errors.Invalid_Parameter) if target is self else True

    def canGather(self, target=_NO_TARGET, *flags, checkCanTargetUnit=True,
                  checkCanIssueCommandType=True, checkCommandibility=True) -> bool:
        targeted, (ct, ci, cc) = _query_flags(target, flags, (checkCanTargetUnit, checkCanIssueCommandType, checkCommandibility))
        if not self._can_query_start(cc):
            return False
        if not targeted:
            if not self.getType().isBuilding() and not self.isInterruptible():
                return self.game.setLastError(Errors.Unit_Busy)
            if not self.getType().isWorker():
                return self.game.setLastError(Errors.Incompatible_UnitType)
            if self.getPowerUp() is not None:
                return self.game.setLastError(Errors.Unit_Busy)
            if self.isHallucination():
                return self.game.setLastError(Errors.Incompatible_UnitType)
            if self.isBurrowed() or not self.isCompleted():
                return self.game.setLastError(Errors.Incompatible_State)
            if self.getOrder() == Orders.ConstructingBuilding:
                return self.game.setLastError(Errors.Unit_Busy)
            return True
        if ci and not self.canGather(False):
            return False
        if ct and not self.canTargetUnit(target, False):
            return False
        target = self._query_target(target)
        kind = target.getType()
        if not kind.isResourceContainer() or kind == 188:
            return self.game.setLastError(Errors.Incompatible_UnitType)
        if not target.isCompleted():
            return self.game.setLastError(Errors.Unit_Busy)
        if not self.hasPath(target.getPosition()):
            return self.game.setLastError(Errors.Unreachable_Location)
        if kind.isRefinery() and target.getPlayer() is not self.game.self():
            return self.game.setLastError(Errors.Unit_Not_Owned)
        return True

    def canReturnCargo(self, checkCommandibility=True) -> bool:
        if not self._can_query_start(checkCommandibility):
            return False
        if not self.getType().isBuilding() and not self.isInterruptible():
            return self.game.setLastError(Errors.Unit_Busy)
        if not self.getType().isWorker():
            return self.game.setLastError(Errors.Incompatible_UnitType)
        if not self.isCarryingGas() and not self.isCarryingMinerals():
            return self.game.setLastError(Errors.Insufficient_Ammo)
        if self.isBurrowed():
            return self.game.setLastError(Errors.Incompatible_State)
        if self.getOrder() == Orders.ConstructingBuilding:
            return self.game.setLastError(Errors.Unit_Busy)
        return True

    def canRepair(self, target=_NO_TARGET, *flags, checkCanTargetUnit=True,
                  checkCanIssueCommandType=True, checkCommandibility=True) -> bool:
        targeted, (ct, ci, cc) = _query_flags(target, flags, (checkCanTargetUnit, checkCanIssueCommandType, checkCommandibility))
        if not self._can_query_start(cc):
            return False
        if not targeted:
            if not self.isInterruptible():
                return self.game.setLastError(Errors.Unit_Busy)
            if self.getType() != 7:
                return self.game.setLastError(Errors.Incompatible_UnitType)
            if not self.isCompleted():
                return self.game.setLastError(Errors.Incompatible_State)
            if self.isHallucination():
                return self.game.setLastError(Errors.Incompatible_UnitType)
            if self.getOrder() == Orders.ConstructingBuilding:
                return self.game.setLastError(Errors.Unit_Busy)
            return True
        if ci and not self.canRepair(False):
            return False
        if ct and not self.canTargetUnit(target, False):
            return False
        target = self._query_target(target)
        kind = target.getType()
        if kind.getRace() != Races.Terran or not kind.isMechanical():
            return self.game.setLastError(Errors.Incompatible_UnitType)
        if target.getHitPoints() == kind.maxHitPoints() or not target.isCompleted():
            return self.game.setLastError(Errors.Incompatible_State)
        return self.game.setLastError(Errors.Invalid_Parameter) if target is self else True

    def canBurrow(self, checkCommandibility=True) -> bool:
        if not self._can_query_start(checkCommandibility):
            return False
        return self.canUseTechWithoutTarget(TechTypes.Burrowing, True, False)

    def canCloak(self, checkCommandibility=True) -> bool:
        if not self._can_query_start(checkCommandibility):
            return False
        return self.canUseTechWithoutTarget(self.getType().cloakingTech(), True, False)

    def canSiege(self, checkCommandibility=True) -> bool:
        if not self._can_query_start(checkCommandibility):
            return False
        return self.canUseTechWithoutTarget(TechTypes.Tank_Siege_Mode, True, False)

    def canLoad(self, target=_NO_TARGET, *flags, checkCanTargetUnit=True,
                checkCanIssueCommandType=True, checkCommandibility=True) -> bool:
        targeted, (ct, ci, cc) = _query_flags(target, flags, (checkCanTargetUnit, checkCanIssueCommandType, checkCommandibility))
        if not self._can_query_start(cc):
            return False
        if not targeted:
            if not self.getType().isBuilding() and not self.isInterruptible():
                return self.game.setLastError(Errors.Unit_Busy)
            if not self.isCompleted():
                return self.game.setLastError(Errors.Unit_Busy)
            if self.getType() == 42 and self._query_self().getUpgradeLevel(24) == 0:
                return self.game.setLastError(Errors.Insufficient_Tech)
            if self.isBurrowed():
                return self.game.setLastError(Errors.Incompatible_State)
            if self.getOrder() == Orders.ConstructingBuilding:
                return self.game.setLastError(Errors.Unit_Busy)
            return self.game.setLastError(Errors.Incompatible_UnitType) if self.getType() == 35 else True
        if ci and not self.canLoad(False):
            return False
        if ct and not self.canTargetUnit(target, False):
            return False
        target = self._query_target(target)
        if target.getPlayer() is not self.game.self():
            return self.game.setLastError(Errors.Unit_Not_Owned)
        if target.isLoaded() or not target.isCompleted():
            return self.game.setLastError(Errors.Unit_Busy)
        if target.getType() == 42 and self._query_self().getUpgradeLevel(24) == 0:
            return self.game.setLastError(Errors.Insufficient_Tech)
        own_space, target_space = self.getType().spaceProvided(), target.getType().spaceProvided()
        if own_space <= 0 and target_space <= 0:
            return self.game.setLastError(Errors.Incompatible_UnitType)
        passenger, transport = (target, self) if own_space > 0 else (self, target)
        kind = passenger.getType()
        if not kind.canMove() or kind.isFlyer() or kind.spaceRequired() > 8:
            return self.game.setLastError(Errors.Incompatible_UnitType)
        if not passenger.isCompleted() or passenger.isBurrowed():
            return self.game.setLastError(Errors.Incompatible_State)
        if transport.isHallucination():
            return self.game.setLastError(Errors.Incompatible_UnitType)
        if transport.getType() == 125:
            if not kind.isOrganic() or kind.getRace() != Races.Terran:
                return self.game.setLastError(Errors.Incompatible_UnitType)
            if not passenger.hasPath(transport.getPosition()):
                return self.game.setLastError(Errors.Unreachable_Location)
        free = own_space if own_space > 0 else target_space
        for unit in transport.getLoadedUnits():
            required = unit.getType().spaceRequired()
            if 0 < required < 8:
                free -= required
        return self.game.setLastError(Errors.Insufficient_Space) if kind.spaceRequired() > free else True

    def canUnloadWithOrWithoutTarget(self, checkCommandibility=True) -> bool:
        if not self._can_query_start(checkCommandibility):
            return False
        if not self.getType().isBuilding() and not self.isInterruptible():
            return self.game.setLastError(Errors.Unit_Busy)
        if not self.getLoadedUnits():
            return self.game.setLastError(Errors.Unit_Does_Not_Exist)
        if self.getType() == 42 and self._query_self().getUpgradeLevel(24) == 0:
            return self.game.setLastError(Errors.Insufficient_Tech)
        return self.game.setLastError(Errors.Incompatible_UnitType) if self.getType().spaceProvided() <= 0 else True

    def canUnloadAtPosition(self, target, checkCanIssueCommandType=True, checkCommandibility=True) -> bool:
        if not self._can_query_start(checkCommandibility):
            return False
        if checkCanIssueCommandType and not self.canUnloadWithOrWithoutTarget(False):
            return False
        if self.getType() != 125:
            if target is None:
                raise TypeError("a BWAPI target is required")
            position = _to_position(target)
            # C++ integer division truncates toward zero, negatives included.
            x, y = int(position.x / 8), int(position.y / 8)
            if not (0 <= x < self.game.mapWidth() * 4 and 0 <= y < self.game.mapHeight() * 4):
                return self.game.setLastError(Errors.Invalid_Tile_Position)
            if not self.game.isWalkable(x, y):
                return self.game.setLastError(Errors.Unreachable_Location)
        return True

    def canUnload(self, target=_NO_TARGET, *flags, checkCanTargetUnit=True, checkPosition=True,
                  checkCanIssueCommandType=True, checkCommandibility=True) -> bool:
        targeted, (ct, cp, ci, cc) = _query_flags(target, flags, (checkCanTargetUnit, checkPosition,
                                                               checkCanIssueCommandType, checkCommandibility))
        if not targeted:
            return self.canUnloadAtPosition(self.getPosition(), True, cc)
        if not self._can_query_start(cc):
            return False
        if ci and not self.canUnloadWithOrWithoutTarget(False):
            return False
        if cp and not self.canUnloadAtPosition(self.getPosition(), False, False):
            return False
        if ct and not self.canTargetUnit(target, False):
            return False
        target = self._query_target(target)
        if not target.isLoaded():
            return self.game.setLastError(Errors.Incompatible_State)
        return self.game.setLastError(Errors.Invalid_Parameter) if target.getTransport() is not self else True

    def canUnloadAll(self, checkCommandibility=True) -> bool:
        return self.canUnloadAtPosition(self.getPosition(), True, checkCommandibility)

    def canUnloadAllPosition(self, target=_NO_TARGET, *flags, checkCanIssueCommandType=True, checkCommandibility=True) -> bool:
        targeted, (ci, cc) = _query_flags(target, flags, (checkCanIssueCommandType, checkCommandibility))
        if not self._can_query_start(cc):
            return False
        if not targeted:
            if not self.canUnloadWithOrWithoutTarget(False):
                return False
            return self.game.setLastError(Errors.Incompatible_UnitType) if self.getType() == 125 else True
        if ci and not self.canUnloadAllPosition(False):
            return False
        return self.canUnloadAtPosition(target, False, False)

    def _query_self(self):
        player = self.game.self()
        if player is None:
            raise TypeError("a BWAPI target is required")
        return player

    def canRightClickPosition(self, checkCommandibility=True) -> bool:
        if not self._can_query_start(checkCommandibility):
            return False
        if not self.getType().isBuilding() and not self.isInterruptible():
            return self.game.setLastError(Errors.Unit_Busy)
        if not self.canMove(False) and not self.canSetRallyPosition(False):
            return self.game.setLastError(Errors.Incompatible_State)
        return True

    def canRightClickPositionGrouped(self, checkCommandibilityGrouped=True, checkCommandibility=True) -> bool:
        if not self._can_query_start(checkCommandibility):
            return False
        if checkCommandibilityGrouped and not self.canCommandGrouped(False):
            return False
        if not self.isInterruptible():
            return self.game.setLastError(Errors.Unit_Busy)
        return True if self.canMoveGrouped(False, False) else self.game.setLastError(Errors.Incompatible_State)

    def canRightClickUnit(self, target=_NO_TARGET, *flags, checkCanTargetUnit=True,
                          checkCanIssueCommandType=True, checkCommandibility=True) -> bool:
        targeted, (ct, ci, cc) = _query_flags(target, flags, (checkCanTargetUnit, checkCanIssueCommandType, checkCommandibility))
        if not self._can_query_start(cc):
            return False
        if not targeted:
            if not self.getType().isBuilding() and not self.isInterruptible():
                return self.game.setLastError(Errors.Unit_Busy)
            if (not self.canFollow(False) and not self.canAttackUnit(False)
                    and not self.canLoad(False) and not self.canSetRallyUnit(False)):
                return self.game.setLastError(Errors.Incompatible_State)
            return True
        if ci and not self.canRightClickUnit(False):
            return False
        if ct and not self.canTargetUnit(target, False):
            return False
        target = self._query_target(target)
        if self._query_enemy(target) and not self.canAttackUnit(target, False, True, False):
            return False
        if (not self.canFollow(target, False, True, False) and not self.canLoad(target, False, True, False)
                and not self.canSetRallyUnit(target, False, True, False)):
            return self.game.setLastError(Errors.Incompatible_State)
        return True

    def canRightClickUnitGrouped(self, target=_NO_TARGET, *flags, checkCanTargetUnit=True,
                                 checkCanIssueCommandType=True, checkCommandibilityGrouped=True, checkCommandibility=True) -> bool:
        targeted, (ct, ci, cg, cc) = _query_flags(target, flags, (checkCanTargetUnit, checkCanIssueCommandType,
                                                               checkCommandibilityGrouped, checkCommandibility), 2)
        if not self._can_query_start(cc):
            return False
        if cg and not self.canCommandGrouped(False):
            return False
        if not targeted:
            if not self.isInterruptible():
                return self.game.setLastError(Errors.Unit_Busy)
            if not self.canFollow(False) and not self.canAttackUnitGrouped(False, False) and not self.canLoad(False):
                return self.game.setLastError(Errors.Incompatible_State)
            return True
        if ci and not self.canRightClickUnitGrouped(False, False):
            return False
        if ct and not self.canTargetUnit(target, False):
            return False
        target = self._query_target(target)
        if self._query_enemy(target) and not self.canAttackUnitGrouped(target, False, True, False, False):
            return False
        if not self.canFollow(target, False, True, False) and not self.canLoad(target, False, True, False):
            return self.game.setLastError(Errors.Incompatible_State)
        return True

    def canRightClick(self, target=_NO_TARGET, *flags, checkCanTargetUnit=True,
                      checkCanIssueCommandType=True, checkCommandibility=True) -> bool:
        targeted, (ct, ci, cc) = _query_flags(target, flags, (checkCanTargetUnit, checkCanIssueCommandType, checkCommandibility))
        if not self._can_query_start(cc):
            return False
        if not targeted:
            return self.canRightClickPosition(False) or self.canRightClickUnit(False)
        if target is None:
            return self.game.setLastError(Errors.Invalid_Parameter)
        if isinstance(target, Unit):
            return self.canRightClickUnit(target, ct, ci, False)
        _to_position(target)
        return self.canRightClickPosition(False)

    def canRightClickGrouped(self, target=_NO_TARGET, *flags, checkCanTargetUnit=True,
                             checkCanIssueCommandTypeGrouped=True, checkCommandibilityGrouped=True, checkCommandibility=True) -> bool:
        targeted, (ct, ci, cg, cc) = _query_flags(target, flags, (checkCanTargetUnit, checkCanIssueCommandTypeGrouped,
                                                               checkCommandibilityGrouped, checkCommandibility), 2)
        if not self._can_query_start(cc):
            return False
        if cg and not self.canCommandGrouped(False):
            return False
        if not targeted:
            return self.canRightClickPositionGrouped(False, False) or self.canRightClickUnitGrouped(False, False)
        if target is None:
            return self.game.setLastError(Errors.Invalid_Parameter)
        if isinstance(target, Unit):
            return self.canRightClickUnitGrouped(target, ct, ci, False, False)
        _to_position(target)
        return self.canRightClickPositionGrouped(False, False)

    def _query_enemy(self, target):
        owner = target.getPlayer()
        if owner is None:
            raise TypeError("a BWAPI target is required")
        if owner.isNeutral():
            return False
        player = self.getPlayer()
        if player is None:
            raise TypeError("a BWAPI target is required")
        return player.isEnemy(owner)

    def canUseTechWithOrWithoutTarget(self, tech=_NO_TARGET, *flags,
                                      checkCanIssueCommandType=True, checkCommandibility=True) -> bool:
        typed, (ci, cc) = _query_flags(tech, flags, (checkCanIssueCommandType, checkCommandibility))
        if not self._can_query_start(cc):
            return False
        kind = self.getType()
        if not typed:
            if not kind.isBuilding() and not self.isInterruptible():
                return self.game.setLastError(Errors.Unit_Busy)
            if not self.isCompleted():
                return self.game.setLastError(Errors.Incompatible_State)
            return self.game.setLastError(Errors.Incompatible_UnitType) if self.isHallucination() else True
        if ci and not self.canUseTechWithOrWithoutTarget(False):
            return False
        tech = TechType(tech)
        if not kind.isHero() and not self._query_self().hasResearched(tech) and kind != 103:
            return self.game.setLastError(Errors.Insufficient_Tech)
        if self.getEnergy() < tech.energyCost():
            return self.game.setLastError(Errors.Insufficient_Energy)
        if tech != TechTypes.Burrowing and kind not in tech.whatUses():
            return self.game.setLastError(Errors.Incompatible_UnitType)
        if tech == TechTypes.Spider_Mines:
            return self.game.setLastError(Errors.Insufficient_Ammo) if self.getSpiderMineCount() <= 0 else True
        if tech == TechTypes.Tank_Siege_Mode:
            if self.isSieged():
                return self.game.setLastError(Errors.Incompatible_State)
            if self.getOrder() in (Orders.Sieging, Orders.Unsieging):
                return self.game.setLastError(Errors.Unit_Busy)
        elif tech in (TechTypes.Cloaking_Field, TechTypes.Personnel_Cloaking):
            if self.getSecondaryOrder() == Orders.Cloak:
                return self.game.setLastError(Errors.Incompatible_State)
        elif tech == TechTypes.Burrowing:
            if not kind.isBurrowable():
                return self.game.setLastError(Errors.Incompatible_UnitType)
            if self.isBurrowed() or self.getOrder() in (Orders.Burrowing, Orders.Unburrowing):
                return self.game.setLastError(Errors.Incompatible_State)
        elif tech in (44, 46):
            return self.game.setLastError(Errors.Incompatible_TechType)
        elif tech == TechTypes.Nuclear_Strike:
            player = self.getPlayer()
            if player is None:
                raise TypeError("a BWAPI target is required")
            if player.completedUnitCount(14) == 0:
                return self.game.setLastError(Errors.Insufficient_Ammo)
        return True

    def canUseTech(self, tech, target=None, checkCanTargetUnit=True, checkTargetsType=True,
                   checkCanIssueCommandType=True, checkCommandibility=True) -> bool:
        if not self._can_query_start(checkCommandibility):
            return False
        if target is None:
            return self.canUseTechWithoutTarget(tech, checkCanIssueCommandType, False)
        if isinstance(target, Unit):
            return self.canUseTechUnit(tech, target, checkCanTargetUnit, checkTargetsType, checkCanIssueCommandType, False)
        return self.canUseTechPosition(tech, target, checkTargetsType, checkCanIssueCommandType, False)

    def canUseTechWithoutTarget(self, tech, checkCanIssueCommandType=True, checkCommandibility=True) -> bool:
        if not self._can_query_start(checkCommandibility):
            return False
        if checkCanIssueCommandType and not self.canUseTechWithOrWithoutTarget(False):
            return False
        tech = TechType(tech)
        if not self.canUseTechWithOrWithoutTarget(tech, False, False):
            return False
        if tech.targetsUnit() or tech.targetsPosition() or tech in (44, 46, TechTypes.Lurker_Aspect):
            return self.game.setLastError(Errors.Incompatible_TechType)
        return True

    def canUseTechUnit(self, tech, target=_NO_TARGET, *flags, checkCanTargetUnit=True,
                       checkTargetsUnits=True, checkCanIssueCommandType=True, checkCommandibility=True) -> bool:
        targeted, (ct, tt, ci, cc) = _query_flags(target, flags, (checkCanTargetUnit, checkTargetsUnits,
                                                               checkCanIssueCommandType, checkCommandibility), 2)
        if not self._can_query_start(cc):
            return False
        if ci and not self.canUseTechWithOrWithoutTarget(False):
            return False
        tech = TechType(tech)
        if not targeted:
            if not self.canUseTechWithOrWithoutTarget(tech, False, False):
                return False
            return True if tech.targetsUnit() else self.game.setLastError(Errors.Incompatible_TechType)
        if tt and not self.canUseTechUnit(tech, False, False):
            return False
        if ct and not self.canTargetUnit(target, False):
            return False
        target = self._query_target(target)
        kind = target.getType()
        if tech in (TechTypes.Archon_Warp, TechTypes.Dark_Archon_Meld):
            if kind != (67 if tech == TechTypes.Archon_Warp else 61):
                return self.game.setLastError(Errors.Incompatible_UnitType)
            if target.getPlayer() is not self.getPlayer():
                return self.game.setLastError(Errors.Unit_Not_Owned)
        elif tech == TechTypes.Consume:
            if target.getPlayer() is not self.getPlayer():
                return self.game.setLastError(Errors.Unit_Not_Owned)
            if kind.getRace() != Races.Zerg or kind == 35:
                return self.game.setLastError(Errors.Incompatible_UnitType)
        elif tech == TechTypes.Spawn_Broodlings:
            if (not kind.isOrganic() and not kind.isMechanical()) or kind.isRobotic() or kind.isFlyer():
                return self.game.setLastError(Errors.Incompatible_UnitType)
        elif tech == TechTypes.Lockdown:
            if not kind.isMechanical():
                return self.game.setLastError(Errors.Incompatible_UnitType)
        elif tech == TechTypes.Healing:
            if target.getHitPoints() == kind.maxHitPoints():
                return self.game.setLastError(Errors.Incompatible_State)
            if not kind.isOrganic() or kind.isFlyer():
                return self.game.setLastError(Errors.Incompatible_UnitType)
            if self._query_enemy(target):
                return self.game.setLastError(Errors.Invalid_Parameter)
        elif tech == TechTypes.Mind_Control:
            if target.getPlayer() is self.getPlayer():
                return self.game.setLastError(Errors.Invalid_Parameter)
            if kind in (73, 13, 97, 59, 35, 36):
                return self.game.setLastError(Errors.Incompatible_UnitType)
        elif tech == TechTypes.Feedback:
            if not kind.isSpellcaster():
                return self.game.setLastError(Errors.Incompatible_UnitType)
        elif tech == TechTypes.Infestation:
            if kind != 106 or target.getHitPoints() >= 750 or target.getHitPoints() <= 0:
                return self.game.setLastError(Errors.Invalid_Parameter)
        merging = tech in (TechTypes.Archon_Warp, TechTypes.Dark_Archon_Meld)
        if merging:
            if not self.hasPath(target.getPosition()):
                return self.game.setLastError(Errors.Unreachable_Location)
            if target.isHallucination():
                return self.game.setLastError(Errors.Invalid_Parameter)
            if target.isMaelstrommed():
                return self.game.setLastError(Errors.Incompatible_State)
        affected = (TechTypes.Parasite, TechTypes.Irradiate, TechTypes.Optical_Flare,
                    TechTypes.Spawn_Broodlings, TechTypes.Lockdown, TechTypes.Defensive_Matrix,
                    TechTypes.Hallucination, TechTypes.Healing, TechTypes.Restoration, TechTypes.Mind_Control)
        if (merging or tech in (*affected, TechTypes.Consume, TechTypes.Feedback, TechTypes.Yamato_Gun)) and target.isStasised():
            return self.game.setLastError(Errors.Incompatible_State)
        if tech in (*affected, TechTypes.Yamato_Gun) and target.isInvincible():
            return self.game.setLastError(Errors.Invalid_Parameter)
        if tech in (*affected, TechTypes.Consume, TechTypes.Feedback) and kind.isBuilding():
            return self.game.setLastError(Errors.Incompatible_UnitType)
        return self.game.setLastError(Errors.Invalid_Parameter) if target is self else True

    def canUseTechPosition(self, tech, target=_NO_TARGET, *flags, checkTargetsPositions=True,
                           checkCanIssueCommandType=True, checkCommandibility=True) -> bool:
        targeted, (tp, ci, cc) = _query_flags(target, flags, (checkTargetsPositions, checkCanIssueCommandType, checkCommandibility), 2)
        if not self._can_query_start(cc):
            return False
        if ci and not self.canUseTechWithOrWithoutTarget(False):
            return False
        tech = TechType(tech)
        if not targeted:
            if not self.canUseTechWithOrWithoutTarget(tech, False, False):
                return False
            return True if tech.targetsPosition() else self.game.setLastError(Errors.Incompatible_TechType)
        if tp and not self.canUseTechPosition(tech, False, False):
            return False
        if tech == TechTypes.Spider_Mines and not self.hasPath(target):
            return self.game.setLastError(Errors.Unreachable_Location)
        return True

    def canPlaceCOP(self, target=_NO_TARGET, *flags, checkCanIssueCommandType=True, checkCommandibility=True) -> bool:
        targeted, (ci, cc) = _query_flags(target, flags, (checkCanIssueCommandType, checkCommandibility))
        if not self._can_query_start(cc):
            return False
        if not targeted:
            if not self.getType().isFlagBeacon():
                return self.game.setLastError(Errors.Incompatible_UnitType)
            # Templates.h: buttonset 228 is a flag that was already placed.
            button_set = self._d.buttonset
            if button_set == 228 or self.getOrder() != Orders.CTFCOPInit:
                return self.game.setLastError(Errors.Incompatible_State)
            return True
        if ci and not self.canPlaceCOP(False):
            return False
        return self.game.canBuildHere(target, self.getType(), self, True)

    def canIssueCommandType(self, command_type, checkCommandibility=True) -> bool:
        if not self._can_query_start(checkCommandibility):
            return False
        ct = _command_type_id(command_type)
        return getattr(self, _COMMAND_TYPE_QUERIES[ct])(False) if 0 <= ct < 44 else True

    def canIssueCommandTypeGrouped(self, command_type, checkCommandibilityGrouped=True, checkCommandibility=True) -> bool:
        if not self._can_query_start(checkCommandibility):
            return False
        if checkCommandibilityGrouped and not self.canCommandGrouped(False):
            return False
        ct = _command_type_id(command_type)
        if ct in _UNGROUPED_COMMAND_TYPES:
            return False
        if ct in _GROUPED_QUERY_TYPES:
            return getattr(self, _COMMAND_TYPE_QUERIES[ct] + "Grouped")(False, False)
        return getattr(self, _COMMAND_TYPE_QUERIES[ct])(False) if 0 <= ct < 44 else True

    def _can_query_start(self, check_commandibility):
        if type(check_commandibility) is not bool:
            raise TypeError("checkCommandibility must be bool")
        return self.canCommand() if check_commandibility else self.game.setLastError()

    def canCommandGrouped(self, checkCommandibility=True) -> bool:
        if not self._can_query_start(checkCommandibility):
            return False
        if self.getType().isBuilding() or self.getType().isCritter():
            return self.game.setLastError(Errors.Incompatible_UnitType)
        return True

    def canMove(self, checkCommandibility=True) -> bool:
        if not self._can_query_start(checkCommandibility):
            return False
        unit_type = self.getType()
        if not unit_type.isBuilding():
            if not self.isInterruptible():
                return self.game.setLastError(Errors.Unit_Busy)
            if not unit_type.canMove():
                return self.game.setLastError(Errors.Incompatible_UnitType)
            if self.isBurrowed():
                return self.game.setLastError(Errors.Incompatible_State)
            if self.getOrder() == Orders.ConstructingBuilding:
                return self.game.setLastError(Errors.Unit_Busy)
            if unit_type == 35:
                return self.game.setLastError(Errors.Incompatible_UnitType)
        else:
            if not unit_type.isFlyingBuilding():
                return self.game.setLastError(Errors.Incompatible_UnitType)
            if not self.isLifted():
                return self.game.setLastError(Errors.Incompatible_State)
        return True if self.isCompleted() else self.game.setLastError(Errors.Incompatible_State)

    def canMoveGrouped(self, checkCommandibilityGrouped=True, checkCommandibility=True) -> bool:
        if not self._can_query_start(checkCommandibility):
            return False
        if checkCommandibilityGrouped and not self.canCommandGrouped(False):
            return False
        if not self.getType().canMove():
            return self.game.setLastError(Errors.Incompatible_UnitType)
        if not self.isCompleted() and not self.isMorphing():
            return self.game.setLastError(Errors.Incompatible_State)
        return True

    def canPatrol(self, checkCommandibility=True) -> bool:
        return self.canMove(checkCommandibility)

    def canPatrolGrouped(self, checkCommandibilityGrouped=True, checkCommandibility=True) -> bool:
        return self.canMoveGrouped(checkCommandibilityGrouped, checkCommandibility)

    def canHoldPosition(self, checkCommandibility=True) -> bool:
        if not self._can_query_start(checkCommandibility):
            return False
        unit_type = self.getType()
        if not unit_type.isBuilding():
            if not unit_type.canMove():
                return self.game.setLastError(Errors.Incompatible_UnitType)
            if self.isBurrowed() and unit_type != 103:
                return self.game.setLastError(Errors.Incompatible_State)
            if self.getOrder() == Orders.ConstructingBuilding:
                return self.game.setLastError(Errors.Unit_Busy)
            if unit_type == 35:
                return self.game.setLastError(Errors.Incompatible_UnitType)
        else:
            if not unit_type.isFlyingBuilding():
                return self.game.setLastError(Errors.Incompatible_UnitType)
            if not self.isLifted():
                return self.game.setLastError(Errors.Incompatible_State)
        return True if self.isCompleted() else self.game.setLastError(Errors.Incompatible_State)

    def canStop(self, checkCommandibility=True) -> bool:
        if not self._can_query_start(checkCommandibility):
            return False
        if not self.isCompleted() or (self.isBurrowed() and self.getType() != 103):
            return self.game.setLastError(Errors.Incompatible_State)
        if self.getType().isBuilding() and not self.isLifted() and self.getType() not in (162, 146, 144, 124):
            return self.game.setLastError(Errors.Incompatible_State)
        return True

    def canUnburrow(self, checkCommandibility=True) -> bool:
        if not self._can_query_start(checkCommandibility):
            return False
        if not self.getType().isBurrowable():
            return self.game.setLastError(Errors.Incompatible_UnitType)
        if not self.isBurrowed() or self.getOrder() == Orders.Unburrowing:
            return self.game.setLastError(Errors.Incompatible_State)
        return True

    def canSetRallyPosition(self, checkCommandibility=True) -> bool:
        if not self._can_query_start(checkCommandibility):
            return False
        if not self.getType().canProduce() or not self.getType().isBuilding():
            return self.game.setLastError(Errors.Incompatible_UnitType)
        return self.game.setLastError(Errors.Incompatible_State) if self.isLifted() else True

    def canDecloak(self, checkCommandibility=True) -> bool:
        if not self._can_query_start(checkCommandibility):
            return False
        if self.getType().cloakingTech() == 44:
            return self.game.setLastError(Errors.Incompatible_UnitType)
        if self.getSecondaryOrder() != Orders.Cloak:
            return self.game.setLastError(Errors.Incompatible_State)
        return True

    def canUnsiege(self, checkCommandibility=True) -> bool:
        if not self._can_query_start(checkCommandibility):
            return False
        if not self.isSieged():
            return self.game.setLastError(Errors.Incompatible_State)
        if self.getOrder() in (Orders.Sieging, Orders.Unsieging):
            return self.game.setLastError(Errors.Unit_Busy)
        if self.isHallucination():
            return self.game.setLastError(Errors.Incompatible_UnitType)
        return True

    def canLift(self, checkCommandibility=True) -> bool:
        if not self._can_query_start(checkCommandibility):
            return False
        if not self.getType().isFlyingBuilding():
            return self.game.setLastError(Errors.Incompatible_UnitType)
        if self.isLifted() or not self.isCompleted():
            return self.game.setLastError(Errors.Incompatible_State)
        if not self.isIdle():
            return self.game.setLastError(Errors.Unit_Busy)
        return True

    def canLand(self, target=None, checkCanIssueCommandType=True, checkCommandibility=True) -> bool:
        check = target if type(target) is bool else checkCommandibility
        if not self._can_query_start(check):
            return False
        if target is None or type(target) is bool:
            if not self.getType().isFlyingBuilding():
                return self.game.setLastError(Errors.Incompatible_UnitType)
            return True if self.isLifted() else self.game.setLastError(Errors.Incompatible_State)
        if checkCanIssueCommandType and not self.canLand(checkCommandibility):
            return False
        return self.game.canBuildHere(target, self.getType(), None, True)

    def isInWeaponRange(self, target) -> bool:
        if not self.exists() or target is None or not target.exists() or target is self:
            return False
        if target.game is not self.game:
            return False
        weapon = self.getType().airWeapon() if target.isFlying() else self.getType().groundWeapon()
        if weapon in (130, 131):
            return False
        player = self.getPlayer()
        if player is None:
            raise TypeError("a BWAPI target is required")
        distance = self.getDistance(target)
        return (weapon.minRange() == 0 or weapon.minRange() < distance) and distance <= player.weaponMaxRange(weapon)

    def getUnitsInWeaponRange(self, weapon, predicate=None) -> Unitset:
        if not self.exists():
            return Unitset()
        weapon = WeaponType(weapon)
        player = self.getPlayer()
        if player is None:
            raise TypeError("a BWAPI target is required")
        maximum = player.weaponMaxRange(weapon)

        def matches(unit):
            if unit is self or unit.isInvincible():
                return False
            distance = self.getDistance(unit)
            if (weapon.minRange() and distance < weapon.minRange()) or distance > maximum:
                return False
            unit_type = unit.getType()
            # Follows the exclusion conditions of BWAPI 4.4 Unit.cpp exactly.
            if ((weapon.targetsOwn() and unit.getPlayer() is not player)
                    or (not weapon.targetsAir() and not unit.isFlying())
                    or (not weapon.targetsGround() and unit.isFlying())
                    or (weapon.targetsMechanical() and unit_type.isMechanical())
                    or (weapon.targetsOrganic() and unit_type.isOrganic())
                    or (weapon.targetsNonBuilding() and not unit_type.isBuilding())
                    or (weapon.targetsNonRobotic() and not unit_type.isRobotic())
                    or (weapon.targetsOrgOrMech() and (unit_type.isOrganic() or unit_type.isMechanical()))):
                return False
            return _matches_predicate(unit, predicate)

        return self.game.getUnitsInRectangle(self.getLeft() - maximum, self.getTop() - maximum,
                                             self.getRight() + maximum, self.getBottom() + maximum, matches)

    def canResearch(self, tech=None, checkCanIssueCommandType=True, *, checkCommandibility=True) -> bool:
        if tech is not None and type(tech) is not bool:
            return self.game.canResearch(tech, self, checkCanIssueCommandType)
        check = tech if type(tech) is bool else checkCommandibility
        if not self._can_query_start(check):
            return False
        if self.isLifted() or not self.isIdle() or not self.isCompleted():
            return self.game.setLastError(Errors.Unit_Busy)
        return True

    def canUpgrade(self, upgrade=None, checkCanIssueCommandType=True, *, checkCommandibility=True) -> bool:
        if upgrade is not None and type(upgrade) is not bool:
            return self.game.canUpgrade(upgrade, self, checkCanIssueCommandType)
        check = upgrade if type(upgrade) is bool else checkCommandibility
        if not self._can_query_start(check):
            return False
        if self.isLifted() or not self.isIdle() or not self.isCompleted():
            return self.game.setLastError(Errors.Unit_Busy)
        return True

    def canCancelConstruction(self, checkCommandibility=True) -> bool:
        if not self._can_query_start(checkCommandibility):
            return False
        if not self.getType().isBuilding():
            return self.game.setLastError(Errors.Incompatible_UnitType)
        if self.isCompleted() or (self.getType() == 134 and self.getNydusExit() is not None):
            return self.game.setLastError(Errors.Incompatible_State)
        return True

    def canCancelAddon(self, checkCommandibility=True) -> bool:
        if not self._can_query_start(checkCommandibility):
            return False
        addon = self.getAddon()
        if addon is None or addon.isCompleted():
            return self.game.setLastError(Errors.Incompatible_UnitType)
        return True

    def canCancelTrain(self, checkCommandibility=True) -> bool:
        if not self._can_query_start(checkCommandibility):
            return False
        return True if self.isTraining() else self.game.setLastError(Errors.Incompatible_State)

    def canCancelTrainSlot(self, slot=None, checkCanIssueCommandType=True, checkCommandibility=True) -> bool:
        if slot is None or type(slot) is bool:
            return self.canCancelTrain(checkCommandibility if slot is None else slot)
        if not self._can_query_start(checkCommandibility):
            return False
        if checkCanIssueCommandType and not self.canCancelTrain(False):
            return False
        if not self.isTraining() or (slot >= 0 and len(self.getTrainingQueue()) <= slot):
            return self.game.setLastError(Errors.Incompatible_State)
        return True

    def canCancelMorph(self, checkCommandibility=True) -> bool:
        if not self._can_query_start(checkCommandibility):
            return False
        if not self.isMorphing() or (not self.isCompleted() and self.getType() == 134 and self.getNydusExit() is not None):
            return self.game.setLastError(Errors.Incompatible_State)
        if self.isHallucination():
            return self.game.setLastError(Errors.Incompatible_UnitType)
        return True

    def canHaltConstruction(self, checkCommandibility: bool = True) -> bool:
        return self._can_order_state(int(OrderId.ConstructingBuilding), checkCommandibility)

    def canCancelResearch(self, checkCommandibility: bool = True) -> bool:
        return self._can_order_state(int(OrderId.ResearchTech), checkCommandibility)

    def canCancelUpgrade(self, checkCommandibility: bool = True) -> bool:
        return self._can_order_state(int(OrderId.Upgrade), checkCommandibility)


def _query_flags(target, flags, defaults, base_count=1):
    """A leading bool is the first check flag of the overload without a target."""
    targeted = target is not _NO_TARGET and type(target) is not bool
    values = flags if targeted or target is _NO_TARGET else (target, *flags)
    count = len(defaults) if targeted else base_count
    if len(values) > count or any(type(value) is not bool for value in values):
        raise TypeError("the check flag of this overload must be a bool")
    start = len(defaults) - count
    return targeted, (*defaults[:start], *values, *defaults[start + len(values):])


def _command_type_id(value) -> int:
    return int(UnitCommandType(value))


def _approx_distance(x: int, y: int) -> int:
    """Position::getApproxDistance from the origin."""
    maximum, minimum = max(abs(x), abs(y)), min(abs(x), abs(y))
    if minimum <= maximum >> 2:
        return maximum
    minimum_calc = (3 * minimum) >> 3
    return (minimum_calc >> 5) + minimum_calc + maximum - (maximum >> 4) - (maximum >> 6)
