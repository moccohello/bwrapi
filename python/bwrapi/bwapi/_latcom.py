"""Latency compensation of the BWAPI 4.4 client (`CommandTemp::execute`).

The official client applies a command's expected effect to its own GameData
the moment the bot issues it, so the bot sees the order, the spent resources
and the queued unit before the game runs the command after its latency. The
server rewrites the whole GameData on the next frame, so nothing here outlives
that frame. Like the client, a command runs this once with its first event
(resources, or the order for Halt_Construction); the later events only belong
to the server-side buffer.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from .types import Orders, Races, TechType, TechTypes, UnitType, UnitTypes, UpgradeType, UpgradeTypes

if TYPE_CHECKING:
    from .unit import Unit
    from .unit_command import UnitCommand

_MOVE_TYPES = frozenset((14, 12, 10, 11, 30, 0))  # Follow, Hold, Move, Patrol, Right_Click_Position, Attack_Move
_CURRENT_FRAME_TYPES = frozenset((5, 3, 4))  # Morph, Build_Addon, Train


def _id(unit) -> int:
    return -1 if unit is None else unit.getID()


def _add_supply(player, race, amount) -> None:
    """Only the three playable races have a supply pool; the checks do not rule out the others."""
    if 0 <= race < 3:
        player.supplyUsed[race] += amount


def _move_to(data, order, x, y) -> None:
    data.order = order
    data.targetPositionX = data.orderTargetPositionX = x
    data.targetPositionY = data.orderTargetPositionY = y


def execute(game, command: UnitCommand) -> None:
    """Apply the first event of `command` to the client's GameData."""
    if not game.isLatComEnabled() or command.isQueued():
        return
    ctype = int(command.type)
    current_frame = game.getRemainingLatencyFrames() == 0
    if current_frame and ctype not in _CURRENT_FRAME_TYPES:
        return
    unit: Unit = command.unit
    target: Unit | None = command.target
    owner = unit.getPlayer() if unit is not None else game.self()
    if unit is None or not unit._d.exists or owner is None:
        return
    u, p = unit._d, owner._d
    kind = unit.getType()
    if ctype in _MOVE_TYPES and not kind.canMove():
        return

    if ctype == 0:  # Attack_Move
        _move_to(u, Orders.AttackMove, command.x, command.y)
    elif ctype == 1:  # Attack_Unit
        if target is None or not target._d.exists or not kind.canAttack():
            return
        u.order = Orders.AttackUnit
        u.target = _id(target)
    elif ctype == 2:  # Build
        u.order = Orders.PlaceBuilding
        u.isConstructing = True
        u.isIdle = False
        u.buildType = command.extra
    elif ctype == 3:  # Build_Addon
        addon = UnitType(command.extra)
        p.minerals -= addon.mineralPrice()
        p.gas -= addon.gasPrice()
        if not current_frame:
            u.isIdle = False
            u.order = Orders.PlaceAddon
    elif ctype == 18:  # Burrow
        u.order = Orders.Burrowing
    elif ctype == 34:  # Cancel_Addon
        addon = UnitType(u.buildType)
        p.minerals += int(addon.mineralPrice() * 0.75)
        p.gas += int(addon.gasPrice() * 0.75)
        u.buildType = UnitTypes.None_
    elif ctype == 33:  # Cancel_Construction
        if kind.getRace() == Races.Terran:
            builder = game.getUnit(u.buildUnit)
            if builder is not None and builder.exists():
                builder._d.buildType = UnitTypes.None_
        u.buildUnit = -1
        p.minerals += int(kind.mineralPrice() * 0.75)
        p.gas += int(kind.gasPrice() * 0.75)
        u.remainingBuildTime = 0
        if kind.getRace() == Races.Zerg:
            u.type = kind.whatBuilds()[0]
            u.buildType = UnitTypes.None_
            u.isMorphing = False
            u.order = Orders.ResetCollision
            u.isConstructing = False
            restored = unit.getType()
            _add_supply(p, restored.getRace(), restored.supplyRequired())
    elif ctype == 37:  # Cancel_Morph
        built = UnitType(u.buildType)
        previous = built.whatBuilds()[0]
        if previous.isBuilding():
            p.minerals += int(built.mineralPrice() * 0.75)
            p.gas += int(built.gasPrice() * 0.75)
        else:
            p.minerals += built.mineralPrice()
            p.gas += built.gasPrice()
        if previous.isBuilding() and previous.producesCreep():
            u.order = Orders.InitCreepGrowth
        if u.type != UnitTypes.Zerg_Egg:
            u.type = previous
        u.buildType = UnitTypes.None_
        u.isConstructing = False
        u.isMorphing = False
        u.isCompleted = True
        u.remainingBuildTime = 0
    elif ctype == 38:  # Cancel_Research
        tech = TechType(u.tech)
        p.minerals += tech.mineralPrice()
        p.gas += tech.gasPrice()
        u.remainingResearchTime = 0
        u.tech = TechTypes.None_
    elif ctype in (35, 36):  # Cancel_Train, Cancel_Train_Slot
        if ctype == 36 and command.extra != 0:
            slot = command.extra
            queued = UnitType(u.trainingQueue[slot])
            p.minerals += queued.mineralPrice()
            p.gas += queued.gasPrice()
            for i in range(slot, 4):
                u.trainingQueue[i] = u.trainingQueue[i + 1]
            u.trainingQueueCount -= 1
            return
        queued = UnitType(u.trainingQueue[u.trainingQueueCount - 1])
        p.minerals += queued.mineralPrice()
        p.gas += queued.gasPrice()
        u.buildUnit = -1
        if u.trainingQueueCount == 1:
            u.isIdle = False
            u.isTraining = False
    elif ctype == 39:  # Cancel_Upgrade
        upgrade = UpgradeType(u.upgrade)
        level = owner.getUpgradeLevel(upgrade) + 1
        p.minerals += upgrade.mineralPrice(level)
        p.gas += upgrade.gasPrice(level)
        u.upgrade = UpgradeTypes.None_
        u.remainingUpgradeTime = 0
    elif ctype == 20:  # Cloak
        u.order = Orders.Cloak
        u.energy -= kind.cloakingTech().energyCost()
    elif ctype == 21:  # Decloak
        u.order = Orders.Decloak
    elif ctype == 14:  # Follow
        u.order = Orders.Follow
        u.target = _id(target)
        u.isIdle = False
        u.isMoving = True
    elif ctype == 15:  # Gather
        u.target = _id(target)
        u.isIdle = False
        u.isMoving = True
        u.isGathering = True
        if target.getType().isMineralField():
            u.order = Orders.MoveToMinerals
        elif target.getType().isRefinery():
            u.order = Orders.MoveToGas
    elif ctype == 32:  # Halt_Construction: its first event is the order event
        building = game.getUnit(u.buildUnit)
        if building is not None:
            building._d.buildUnit = -1
        u.buildUnit = -1
        u.order = Orders.ResetCollision
        u.isConstructing = False
        u.buildType = UnitTypes.None_
    elif ctype == 12:  # Hold_Position
        u.isMoving = False
        u.isIdle = False
        u.order = Orders.HoldPosition
    elif ctype == 25:  # Land
        u.order = Orders.BuildingLand
        u.isIdle = False
    elif ctype == 24:  # Lift
        u.order = Orders.BuildingLiftOff
        u.isIdle = False
    elif ctype == 26:  # Load
        if kind == UnitTypes.Terran_Bunker:
            u.order = Orders.PickupBunker
            u.target = _id(target)
        elif kind.spaceProvided():
            u.order = Orders.PickupTransport
            u.target = _id(target)
        elif target.getType().spaceProvided():
            u.order = Orders.EnterTransport
            u.target = _id(target)
        u.isIdle = False
    elif ctype == 5:  # Morph
        morph = UnitType(command.extra)
        if not current_frame:
            u.isCompleted = False
            u.isIdle = False
            u.isConstructing = True
            u.isMorphing = True
            u.buildType = morph
        if kind.isBuilding():
            if not current_frame:
                u.order = Orders.ZergBuildingMorph
                u.type = morph
            p.minerals -= morph.mineralPrice()
            p.gas -= morph.gasPrice()
        else:
            _add_supply(p, morph.getRace(),
                        morph.supplyRequired() * (1 + int(morph.isTwoUnitsInOneEgg())) - kind.supplyRequired())
            if not current_frame:
                u.order = Orders.ZergUnitMorph
                p.minerals -= morph.mineralPrice()
                p.gas -= morph.gasPrice()
                if morph == UnitTypes.Zerg_Lurker_Egg:
                    u.type = UnitTypes.Zerg_Lurker_Egg
                elif morph in (UnitTypes.Zerg_Devourer, UnitTypes.Zerg_Guardian):
                    u.type = UnitTypes.Zerg_Cocoon
                else:
                    u.type = UnitTypes.Zerg_Egg
                u.trainingQueue[u.trainingQueueCount] = morph
                u.trainingQueueCount += 1
    elif ctype in (10, 30):  # Move, Right_Click_Position
        _move_to(u, Orders.Move, command.x, command.y)
        u.isMoving = True
        u.isIdle = False
    elif ctype == 11:  # Patrol
        _move_to(u, Orders.Patrol, command.x, command.y)
        u.isIdle = False
        u.isMoving = True
    elif ctype == 17:  # Repair
        if kind != UnitTypes.Terran_SCV:
            return
        u.order = Orders.Repair
        u.target = _id(target)
        u.isIdle = False
    elif ctype == 6:  # Research
        tech = TechType(command.extra)
        u.order = Orders.ResearchTech
        u.tech = tech
        u.isIdle = False
        u.remainingResearchTime = tech.researchTime()
        p.minerals -= tech.mineralPrice()
        p.gas -= tech.gasPrice()
        p.isResearching[tech] = True
    elif ctype == 16:  # Return_Cargo
        if not u.carryResourceType:
            return
        u.order = Orders.ReturnGas if unit.isCarryingGas() else Orders.ReturnMinerals
        u.isGathering = True
        u.isIdle = False
    elif ctype == 31:  # Right_Click_Unit
        u.target = _id(target)
        u.isIdle = False
        u.isMoving = True
        target_type = target.getType()
        if kind.isWorker() and target_type.isMineralField():
            u.isGathering = True
            u.order = Orders.MoveToMinerals
        elif kind.isWorker() and target_type.isRefinery():
            u.isGathering = True
            u.order = Orders.MoveToGas
        elif (kind.isWorker() and target_type.getRace() == Races.Terran
              and target_type.whatBuilds()[0] == kind and not target.isCompleted()):
            u.order = Orders.ConstructingBuilding
            u.buildUnit = _id(target)
            target._d.buildUnit = _id(unit)
            u.isConstructing = True
            target._d.isConstructing = True
        elif kind.canAttack() and target.getPlayer() is not owner and not target_type.isNeutral():
            u.order = Orders.AttackUnit
        elif kind.canMove():
            u.order = Orders.Follow
    elif ctype == 8:  # Set_Rally_Position
        if not kind.canProduce():
            return
        u.order = Orders.RallyPointTile
        u.rallyPositionX = command.x
        u.rallyPositionY = command.y
        u.rallyUnit = -1
    elif ctype == 9:  # Set_Rally_Unit
        if not kind.canProduce() or target is None or not target._d.exists:
            return
        u.order = Orders.RallyPointUnit
        u.rallyUnit = _id(target)
    elif ctype == 22:  # Siege
        u.order = Orders.Sieging
    elif ctype == 13:  # Stop
        u.order = Orders.Stop
        u.isIdle = True
    elif ctype == 4:  # Train
        trained = UnitType(command.extra)
        if not current_frame:
            p.minerals -= trained.mineralPrice()
            p.gas -= trained.gasPrice()
        u.trainingQueue[u.trainingQueueCount] = trained
        u.trainingQueueCount += 1
        _add_supply(p, trained.getRace(), trained.supplyRequired())
        u.isTraining = True
        u.isIdle = False
        u.remainingTrainTime = trained.buildTime()
        if trained == UnitTypes.Terran_Nuclear_Missile:
            u.secondaryOrder = Orders.Train
    elif ctype == 19:  # Unburrow
        u.order = Orders.Unburrowing
    elif ctype == 27:  # Unload
        u.order = Orders.Unload
        u.target = _id(target)
    elif ctype == 28:  # Unload_All
        if kind == UnitTypes.Terran_Bunker:
            u.order = Orders.Unload
        else:
            _move_to(u, Orders.MoveUnload, command.x, command.y)
    elif ctype == 29:  # Unload_All_Position
        _move_to(u, Orders.MoveUnload, command.x, command.y)
    elif ctype == 23:  # Unsiege
        u.order = Orders.Unsieging
    elif ctype == 7:  # Upgrade
        upgrade = UpgradeType(command.extra)
        u.order = Orders.Upgrade
        u.upgrade = upgrade
        u.isIdle = False
        level = owner.getUpgradeLevel(upgrade)
        u.remainingUpgradeTime = upgrade.upgradeTime(level + 1)
        p.minerals -= upgrade.mineralPrice(level + 1)
        p.gas -= upgrade.gasPrice(level + 1)
        p.isUpgrading[upgrade] = True
    elif ctype == 40:  # Use_Tech
        if TechType(command.extra) == TechTypes.Stim_Packs and u.hitPoints > 10:
            u.hitPoints -= 10
            u.stimTimer = 17
    elif ctype == 41:  # Use_Tech_Position
        tech = TechType(command.extra)
        if not tech.targetsPosition():
            return
        _move_to(u, tech.getOrder(), command.x, command.y)
    elif ctype == 42:  # Use_Tech_Unit
        tech = TechType(command.extra)
        if not tech.targetsUnit():
            return
        u.order = tech.getOrder()
        u.orderTarget = _id(target)
        position = target.getPosition()
        u.targetPositionX = u.orderTargetPositionX = position.x
        u.targetPositionY = u.orderTargetPositionY = position.y
