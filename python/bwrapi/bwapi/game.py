"""BWAPI Game over the GameData the in-game server fills (client GameImpl.cpp, Game.cpp).

Every value is read from the shared memory the server rewrites each frame;
the match bookkeeping (unit sets, regions, events) follows the official client
and changes only when `Client.update()` delivers a frame. Commands, text and
shapes are written into the same GameData, where the server takes them when
the client hands the frame back.
"""

from __future__ import annotations

import os
from bisect import bisect_left, bisect_right
from ctypes import c_int32
from operator import index as _index

from . import gamedata as _gd
from .bullet import Bullet
from .containers import Bulletset, Forceset, Playerset, Regionset, Unitset
from .filters import Filter, matches as _matches_predicate
from .force import Force
from .interface import Interface
from .player import Player
from .position import Position, TilePosition
from .region import Region
from .type_ids import EventTypeId
from .types import Error, Errors, GameType, TechType, UnitType, UnitTypes, UpgradeType
from .unit import Unit

_text = _gd.decode_text
_COORD_SCREEN, _COORD_MAP, _COORD_MOUSE = 1, 2, 3
_NONE_POSITION = (32000, 32032)
_UNIT_EVENTS = frozenset((
    EventTypeId.UNIT_DISCOVER, EventTypeId.UNIT_EVADE, EventTypeId.UNIT_SHOW, EventTypeId.UNIT_HIDE,
    EventTypeId.UNIT_CREATE, EventTypeId.UNIT_DESTROY, EventTypeId.UNIT_RENEGADE, EventTypeId.UNIT_MORPH,
    EventTypeId.UNIT_COMPLETE,
))
# Game.cpp damageRatio[DamageType][UnitSizeType], in 1/256.
_DAMAGE_RATIO = (
    (0, 0, 0, 0, 0, 0),
    (0, 128, 192, 256, 0, 0),
    (0, 256, 128, 64, 0, 0),
    (0, 256, 256, 256, 0, 0),
    (0, 256, 256, 256, 0, 0),
    (0, 0, 0, 0, 0, 0),
    (0, 0, 0, 0, 0, 0),
)
_MAX_UNIT_WIDTH = None
_MAX_UNIT_HEIGHT = None


def _xy(value, y=None) -> tuple[int, int]:
    if y is not None:
        return _index(value), _index(y)
    if isinstance(value, tuple):
        return _index(value[0]), _index(value[1])
    return value.x, value.y


def _is_point(value) -> bool:
    return isinstance(value, tuple) or (hasattr(value, "x") and hasattr(value, "y"))


def _points(args, count) -> list[int]:
    """Flattens the leading `count` points of a draw call: Points or x, y pairs."""
    values: list[int] = []
    rest = list(args)
    while len(values) < count * 2:
        item = rest.pop(0)
        if _is_point(item):
            values.extend(_xy(item))
        else:
            values.append(_index(item))
    return [*values, *rest]


class Event:
    """A mutable BWAPI event value. Its unit and player are non-owning handles."""

    __slots__ = ("_type", "_unit", "_player", "_position", "_text", "_winner")

    def __init__(self, type=EventTypeId.NONE, unit=None, player=None, position=None, text=None, winner=False):
        if isinstance(type, Event):
            source = type
            type, unit, player = source._type, source._unit, source._player
            position, text, winner = source._position, source._text, source._winner
        self._type = EventTypeId(type)
        self._unit = unit
        self._player = player
        self._position = Position(*_NONE_POSITION) if position is None else Position(position)
        self._text = None
        self._winner = bool(winner)
        self.setText(text)

    def __eq__(self, other: object) -> bool:
        if not isinstance(other, Event):
            return NotImplemented
        return (self._type == other._type and self._position == other._position and self._unit is other._unit
                and self._player is other._player and self._winner == other._winner and self._text == other._text)

    __hash__ = None

    def __copy__(self) -> Event:
        return Event(self)

    def __deepcopy__(self, memo) -> Event:
        # Unit and Player are handles the Game owns; a copy keeps pointing at them.
        copied = memo[id(self)] = Event(self)
        return copied

    def __repr__(self) -> str:
        return f"Event({self._type.name})"

    @staticmethod
    def MatchStart() -> Event: return Event(EventTypeId.MATCH_START)
    @staticmethod
    def MatchEnd(isWinner: bool) -> Event: return Event(EventTypeId.MATCH_END, winner=isWinner)
    @staticmethod
    def MatchFrame() -> Event: return Event(EventTypeId.MATCH_FRAME)
    @staticmethod
    def MenuFrame() -> Event: return Event(EventTypeId.MENU_FRAME)
    @staticmethod
    def SendText(text: str | None = None) -> Event: return Event(EventTypeId.SEND_TEXT, text=text)
    @staticmethod
    def ReceiveText(player, text: str | None = None) -> Event:
        return Event(EventTypeId.RECEIVE_TEXT, player=player, text=text)
    @staticmethod
    def PlayerLeft(player) -> Event: return Event(EventTypeId.PLAYER_LEFT, player=player)
    @staticmethod
    def NukeDetect(target) -> Event: return Event(EventTypeId.NUKE_DETECT, position=target)
    @staticmethod
    def UnitDiscover(unit) -> Event: return Event(EventTypeId.UNIT_DISCOVER, unit)
    @staticmethod
    def UnitEvade(unit) -> Event: return Event(EventTypeId.UNIT_EVADE, unit)
    @staticmethod
    def UnitShow(unit) -> Event: return Event(EventTypeId.UNIT_SHOW, unit)
    @staticmethod
    def UnitHide(unit) -> Event: return Event(EventTypeId.UNIT_HIDE, unit)
    @staticmethod
    def UnitCreate(unit) -> Event: return Event(EventTypeId.UNIT_CREATE, unit)
    @staticmethod
    def UnitDestroy(unit) -> Event: return Event(EventTypeId.UNIT_DESTROY, unit)
    @staticmethod
    def UnitMorph(unit) -> Event: return Event(EventTypeId.UNIT_MORPH, unit)
    @staticmethod
    def UnitRenegade(unit) -> Event: return Event(EventTypeId.UNIT_RENEGADE, unit)
    @staticmethod
    def SaveGame(gameName: str | None = None) -> Event: return Event(EventTypeId.SAVE_GAME, text=gameName)
    @staticmethod
    def UnitComplete(unit) -> Event: return Event(EventTypeId.UNIT_COMPLETE, unit)

    def getType(self) -> EventTypeId: return self._type
    def getPosition(self) -> Position: return Position(self._position)
    def getText(self) -> str: return "" if self._text is None else self._text
    def getUnit(self) -> Unit | None: return self._unit
    def getPlayer(self) -> Player | None: return self._player
    def isWinner(self) -> bool: return self._winner

    def setType(self, type) -> Event:
        self._type = EventTypeId(type)
        return self

    def setPosition(self, position) -> Event:
        self._position = Position(position)
        return self

    def setText(self, text: str | None) -> Event:
        # The C-string boundary of the official value ends at the first NUL.
        self._text = None if text is None else str(text).split("\0", 1)[0]
        return self

    def setUnit(self, unit) -> Event:
        self._unit = unit
        return self

    def setPlayer(self, player) -> Event:
        self._player = player
        return self

    def setWinner(self, isWinner: bool) -> Event:
        self._winner = bool(isWinner)
        return self


class Game(Interface):
    """The BWAPI Game of one client connection (`Broodwar` while connected)."""

    def __init__(self, data: _gd.GameData, client=None) -> None:
        self._data = data
        self._client = client
        self._client_info = None
        self._interface_events = None
        self._retired = False
        self._last_error = Error()
        self._text_size = 1  # Text::Size::Default
        self._forces = tuple(Force(self, i) for i in range(_gd.MAX_FORCES))
        self._players = tuple(Player(self, i) for i in range(_gd.MAX_PLAYERS))
        self._units = tuple(Unit(self, i) for i in range(_gd.MAX_UNITS))
        self._bullets = tuple(Bullet(self, i) for i in range(_gd.MAX_BULLETS))
        self._region_array: list[Region | None] = [None] * _gd.MAX_REGIONS
        self._finder = None
        self._clear_all()

    def _retire(self) -> None:
        self._retired = True

    # ---- match bookkeeping (client GameImpl) --------------------------------------------------
    def _clear_all(self) -> None:
        self._in_game = False
        self._start_locations: list[TilePosition] = []
        self._force_set = Forceset()
        self._player_set = Playerset()
        self._bullet_set = Bulletset()
        self._accessible = Unitset()
        self._minerals = Unitset()
        self._geysers = Unitset()
        self._neutral_units = Unitset()
        self._static_minerals = Unitset()
        self._static_geysers = Unitset()
        self._static_neutral_units = Unitset()
        self._selected = Unitset()
        self._pylons = Unitset()
        self._events: list[Event] = []
        self._nuke_dots: list[Position] = []
        self._self = self._enemy = self._neutral = None
        self._allies = Playerset()
        self._enemies = Playerset()
        self._observers = Playerset()
        for unit in self._units:
            unit._clear()
        for player in self._players:
            player._units = Unitset()
        self._regions = Regionset()
        self._region_array = [None] * _gd.MAX_REGIONS

    def _on_update(self) -> None:
        """Client::update after the server's ready: the frame's events drive the match state."""
        data = self._data
        for i in range(data.eventCount):
            kind = data.events[i].type
            if kind == EventTypeId.MATCH_START:
                self._on_match_start()
            if kind in (EventTypeId.MATCH_FRAME, EventTypeId.MENU_FRAME):
                self._on_match_frame()
        if self._in_game and not data.isInGame:
            self._clear_all()

    def _on_match_start(self) -> None:
        self._clear_all()
        self._in_game = True
        data = self._data
        self._force_set = Forceset(self._forces[i] for i in range(1, data.forceCount))
        self._player_set = Playerset(self._players[i] for i in range(data.playerCount))
        for i in range(data.initialUnitCount):
            unit = self._units[i]
            if unit.exists():
                self._accessible.add(unit)
            unit._save_initial_state()
        self._start_locations = [TilePosition(data.startLocations[i].x, data.startLocations[i].y)
                                 for i in range(data.startLocationCount)]
        for i in range(data.regionCount):
            region = Region(self, i)
            self._region_array[i] = region
            self._regions.add(region)
        for i in range(data.regionCount):
            self._region_array[i]._set_neighbors()
        self._self = self.getPlayer(data.self)
        self._enemy = self.getPlayer(data.enemy)
        self._neutral = self.getPlayer(data.neutral)
        self._on_match_frame()
        self._static_minerals = Unitset(self._minerals)
        self._static_geysers = Unitset(self._geysers)
        self._static_neutral_units = Unitset(self._neutral_units)
        self._text_size = 1

    def _on_match_frame(self) -> None:
        data = self._data
        self._finder = None
        self._bullet_set = Bulletset(bullet for bullet in self._bullets if bullet.exists())
        self._nuke_dots = [Position(data.nukeDots[i].x, data.nukeDots[i].y) for i in range(data.nukeDotCount)]
        events = []
        for i in range(data.eventCount):
            raw = data.events[i]
            events.append(self._make_event(raw))
            kind = raw.type
            if kind == EventTypeId.UNIT_DISCOVER:
                self._discover(self._units[raw.v1], True)
            elif kind == EventTypeId.UNIT_EVADE:
                self._discover(self._units[raw.v1], False)
            elif kind == EventTypeId.UNIT_RENEGADE:
                unit = self._units[raw.v1]
                for player in self._player_set:
                    player._units.discard(unit)
                owner = unit.getPlayer()
                if owner is not None:
                    owner._units.add(unit)
            elif kind == EventTypeId.UNIT_MORPH:
                unit = self._units[raw.v1]
                if unit.getType() == UnitTypes.Resource_Vespene_Geyser:
                    self._geysers.add(unit)
                    self._neutral_units.add(unit)
                elif unit.getType().isRefinery():
                    self._geysers.discard(unit)
                    self._neutral_units.discard(unit)
        self._events = events
        for unit in self._accessible:
            unit._connected = Unitset()
            unit._loaded = Unitset()
        for unit in self._accessible:
            kind = unit.getType()
            if kind == UnitTypes.Zerg_Larva and (hatchery := unit.getHatchery()) is not None:
                hatchery._connected.add(unit)
            if kind == UnitTypes.Protoss_Interceptor and (carrier := unit.getCarrier()) is not None:
                carrier._connected.add(unit)
            if (transport := unit.getTransport()) is not None:
                transport._loaded.add(unit)
        self._selected = Unitset(unit for i in range(data.selectedUnitCount)
                                 if (unit := self.getUnit(data.selectedUnits[i])) is not None)
        self._update_player_sets()
        self._process_interface_events()

    def _discover(self, unit: Unit, discovered: bool) -> None:
        change = Unitset.add if discovered else Unitset.discard
        change(self._accessible, unit)
        owner = unit.getPlayer()
        if owner is None:
            return
        change(owner._units, unit)
        kind = unit.getType()
        if owner.isNeutral():
            change(self._neutral_units, unit)
            if kind.isMineralField():
                change(self._minerals, unit)
            elif kind == UnitTypes.Resource_Vespene_Geyser:
                change(self._geysers, unit)
        elif owner is self._self and kind == UnitTypes.Protoss_Pylon:
            change(self._pylons, unit)

    def _update_player_sets(self) -> None:
        me = self._self
        self._allies, self._enemies, self._observers = Playerset(), Playerset(), Playerset()
        if me is None:
            return
        for player in self._player_set:
            if player.leftGame() or player.isDefeated() or player is me:
                continue
            if me.isAlly(player):
                self._allies.add(player)
            if me.isEnemy(player):
                self._enemies.add(player)
            if player.isObserver():
                self._observers.add(player)

    def _process_interface_events(self) -> None:
        self._update_interface_events()
        for unit in self._accessible:
            unit._update_interface_events() if unit.exists() else unit._clear_interface_events()
        for force in self._force_set:
            force._update_interface_events()
        for bullet in self._bullet_set:
            bullet._update_interface_events() if bullet.exists() else bullet._clear_interface_events()
        for region in self._regions:
            region._update_interface_events()
        for player in self._player_set:
            player._update_interface_events()

    def _make_event(self, raw) -> Event:
        kind = EventTypeId(raw.type)
        event = Event(kind)
        if kind == EventTypeId.MATCH_END:
            event.setWinner(raw.v1 != 0)
        elif kind == EventTypeId.NUKE_DETECT:
            event.setPosition(Position(raw.v1, raw.v2))
        elif kind == EventTypeId.PLAYER_LEFT:
            event.setPlayer(self.getPlayer(raw.v1))
        elif kind in (EventTypeId.SAVE_GAME, EventTypeId.SEND_TEXT):
            event.setText(_text(self._data.eventStrings[raw.v1].value))
        elif kind == EventTypeId.RECEIVE_TEXT:
            event.setPlayer(self.getPlayer(raw.v1))
            event.setText(_text(self._data.eventStrings[raw.v2].value))
        elif kind in _UNIT_EVENTS:
            event.setUnit(self.getUnit(raw.v1))
        return event

    # ---- GameData writes from the client to the server ---------------------------------------
    # GameImpl.cpp addString/addCommand/addShape guard these arrays with assert only:
    # a full frame drops the write instead of raising.
    def _add_string(self, text: str) -> int | None:
        data = self._data
        index = data.stringCount
        if index >= _gd.MAX_STRINGS:
            return None
        # StrCopy strncpy(dst, src, 1023): back the byte cut off to a UTF-8 character boundary.
        raw = text.encode("utf-8")
        if len(raw) > 1023:
            cut = 1023
            while cut > 0 and (raw[cut] & 0xC0) == 0x80:
                cut -= 1
            raw = raw[:cut]
        data.strings[index].value = raw
        data.stringCount = index + 1
        return index

    def _add_command(self, type: int, value1: int = 0, value2: int = 0) -> None:
        data = self._data
        if data.commandCount >= _gd.MAX_COMMANDS:
            return
        command = data.commands[data.commandCount]
        command.type, command.value1, command.value2 = type, value1, value2
        data.commandCount += 1

    def _add_unit_command(self, type: int, unit: int, target: int, x: int, y: int, extra: int) -> None:
        data = self._data
        if data.unitCommandCount >= _gd.MAX_UNIT_COMMANDS:
            return
        command = data.unitCommands[data.unitCommandCount]
        command.type, command.unitIndex, command.targetIndex = type, unit, target
        command.x, command.y, command.extra = x, y, extra
        data.unitCommandCount += 1

    def _add_shape(self, type, ctype, x1, y1, x2=0, y2=0, extra1=0, extra2=0, color=0, is_solid=False) -> None:
        data = self._data
        if not data.hasGUI or data.shapeCount >= _gd.MAX_SHAPES:
            return
        shape = data.shapes[data.shapeCount]
        shape.type, shape.ctype, shape.x1, shape.y1, shape.x2, shape.y2 = type, int(ctype), x1, y1, x2, y2
        shape.extra1, shape.extra2, shape.color, shape.isSolid = extra1, extra2, int(color), bool(is_solid)
        data.shapeCount += 1

    # ---- sets and handles -------------------------------------------------------------------
    def getForces(self) -> Forceset: return Forceset(self._force_set)
    def getPlayers(self) -> Playerset: return Playerset(self._player_set)
    def getAllUnits(self) -> Unitset: return Unitset(self._accessible)
    def getMinerals(self) -> Unitset: return Unitset(self._minerals)
    def getGeysers(self) -> Unitset: return Unitset(self._geysers)
    def getNeutralUnits(self) -> Unitset: return Unitset(self._neutral_units)
    def getStaticMinerals(self) -> Unitset: return Unitset(self._static_minerals)
    def getStaticGeysers(self) -> Unitset: return Unitset(self._static_geysers)
    def getStaticNeutralUnits(self) -> Unitset: return Unitset(self._static_neutral_units)
    def getBullets(self) -> Bulletset: return Bulletset(self._bullet_set)
    def getNukeDots(self) -> list[Position]: return list(self._nuke_dots)
    def getEvents(self) -> list[Event]: return list(self._events)
    def getStartLocations(self) -> list[TilePosition]: return list(self._start_locations)
    def getAllRegions(self) -> Regionset: return Regionset(self._regions)

    def getSelectedUnits(self) -> Unitset:
        self.setLastError()
        return Unitset(self._selected)

    def getForce(self, forceID: int) -> Force | None:
        return self._forces[forceID] if 0 <= forceID < len(self._forces) else None

    def getPlayer(self, playerID: int) -> Player | None:
        return self._players[playerID] if 0 <= playerID < len(self._players) else None

    def getUnit(self, unitID: int) -> Unit | None:
        return self._units[unitID] if 0 <= unitID < len(self._units) else None

    def indexToUnit(self, unitIndex: int) -> Unit | None:
        if not 0 <= unitIndex < _gd.MAX_UNIT_INDEX:
            return None
        return self.getUnit(self._data.unitArray[unitIndex])

    def getRegion(self, regionID: int) -> Region | None:
        return self._region_array[regionID] if 0 <= regionID < self._data.regionCount else None

    def self(self) -> Player | None: return self._self
    def enemy(self) -> Player | None: return self._enemy
    def neutral(self) -> Player | None: return self._neutral
    def allies(self) -> Playerset: return self._allies
    def enemies(self) -> Playerset: return self._enemies
    def observers(self) -> Playerset: return self._observers

    # ---- frame values -------------------------------------------------------------------------
    def getGameType(self) -> GameType: return GameType(self._data.gameType)
    def getLatency(self) -> int: return self._data.latency
    def getFrameCount(self) -> int: return self._data.frameCount
    def getReplayFrameCount(self) -> int: return self._data.replayFrameCount
    def getFPS(self) -> int: return self._data.fps
    def getAverageFPS(self) -> float: return self._data.averageFPS
    def getMousePosition(self) -> Position: return Position(self._data.mouseX, self._data.mouseY)

    def getMouseState(self, button) -> bool:
        button = _index(button)
        return 0 <= button < _gd.MOUSE_MAX and self._data.mouseState[button]

    def getKeyState(self, key) -> bool:
        key = _index(key)
        return 0 <= key < _gd.KEY_MAX and self._data.keyState[key]

    def getScreenPosition(self) -> Position: return Position(self._data.screenX, self._data.screenY)

    def isFlagEnabled(self, flag: int) -> bool:
        return 0 <= flag < _gd.FLAG_MAX and self._data.flags[flag]

    def isInGame(self) -> bool: return self._data.isInGame
    def isMultiplayer(self) -> bool: return self._data.isMultiplayer
    def isBattleNet(self) -> bool: return self._data.isBattleNet
    def isPaused(self) -> bool: return self._data.isPaused
    def isReplay(self) -> bool: return self._data.isReplay
    def getLatencyFrames(self) -> int: return self._data.latencyFrames
    def getLatencyTime(self) -> int: return self._data.latencyTime
    def getRemainingLatencyFrames(self) -> int: return self._data.remainingLatencyFrames
    def getRemainingLatencyTime(self) -> int: return self._data.remainingLatencyTime
    def getRevision(self) -> int: return self._data.revision
    def getClientVersion(self) -> int: return self._data.client_version
    def isDebug(self) -> bool: return self._data.isDebug
    def isLatComEnabled(self) -> bool: return self._data.hasLatCom
    def isGUIEnabled(self) -> bool: return self._data.hasGUI
    def getInstanceNumber(self) -> int: return self._data.instanceID
    def getAPM(self, includeSelects: bool = False) -> int:
        return self._data.botAPM_selects if includeSelects else self._data.botAPM_noselects
    def elapsedTime(self) -> int: return self._data.elapsedTime
    def countdownTimer(self) -> int: return self._data.countdownTimer
    def getLastEventTime(self) -> int: return 0
    def getRandomSeed(self) -> int: return self._data.randomSeed

    def getLastError(self) -> Error: return self._last_error

    def setLastError(self, e=Errors.None_) -> bool:
        self._last_error = Error(e)
        return self._last_error == Errors.None_

    # ---- map ----------------------------------------------------------------------------------
    def mapWidth(self) -> int: return self._data.mapWidth
    def mapHeight(self) -> int: return self._data.mapHeight
    def mapFileName(self) -> str: return _text(self._data.mapFileName)
    def mapPathName(self) -> str: return _text(self._data.mapPathName)
    def mapName(self) -> str: return _text(self._data.mapName)
    def mapHash(self) -> str: return _text(self._data.mapHash)

    def _valid_position(self, x: int, y: int) -> bool:
        # Position::isValid against this game's map, not the global Broodwar.
        return 0 <= x < self._data.mapWidth * 32 and 0 <= y < self._data.mapHeight * 32

    def _tile(self, x, y) -> tuple[int, int] | None:
        x, y = _xy(x, y)
        if x < 0 or y < 0 or x >= self._data.mapWidth or y >= self._data.mapHeight:
            return None
        return x, y

    def isWalkable(self, x, y=None) -> bool:
        x, y = _xy(x, y)
        if x < 0 or y < 0 or x >= self._data.mapWidth * 4 or y >= self._data.mapHeight * 4:
            return False
        return self._data.isWalkable[x][y]

    def getGroundHeight(self, x, y=None) -> int:
        tile = self._tile(x, y)
        return 0 if tile is None else self._data.getGroundHeight[tile[0]][tile[1]]

    def isBuildable(self, x, y=None, includeBuildings: bool = False) -> bool:
        if type(y) is bool:
            y, includeBuildings = None, y
        tile = self._tile(x, y)
        if tile is None:
            return False
        tx, ty = tile
        return self._data.isBuildable[tx][ty] and (not includeBuildings or not self._data.isOccupied[tx][ty])

    def isVisible(self, x, y=None) -> bool:
        tile = self._tile(x, y)
        return tile is not None and self._data.isVisible[tile[0]][tile[1]]

    def isExplored(self, x, y=None) -> bool:
        tile = self._tile(x, y)
        return tile is not None and self._data.isExplored[tile[0]][tile[1]]

    def hasCreep(self, x, y=None) -> bool:
        tile = self._tile(x, y)
        return tile is not None and self._data.hasCreep[tile[0]][tile[1]]

    def getRegionAt(self, x, y=None) -> Region | None:
        x, y = _xy(x, y)
        if not self._valid_position(x, y):
            self.setLastError(Errors.Invalid_Parameter)
            return None
        data = self._data
        region_id = data.mapTileRegionId[x // 32][y // 32]
        if region_id & 0x2000:
            split = region_id & 0x1FFF
            if split >= len(data.mapSplitTilesMiniTileMask):
                return None
            shift = (x & 0x1F) // 8 + ((y & 0x1F) // 8) * 4
            if (data.mapSplitTilesMiniTileMask[split] >> shift) & 1:
                return self.getRegion(data.mapSplitTilesRegion2[split])
            return self.getRegion(data.mapSplitTilesRegion1[split])
        return self.getRegion(region_id)

    def hasPath(self, source, destination) -> bool:
        source, destination = Position(*_xy(source)), Position(*_xy(destination))
        if self._valid_position(source.x, source.y) and self._valid_position(destination.x, destination.y):
            a, b = self.getRegionAt(source), self.getRegionAt(destination)
            if a is not None and b is not None and a.getRegionGroupID() == b.getRegionGroupID():
                return self.setLastError()
        return self.setLastError(Errors.Unreachable_Location)

    # ---- unit search (Templates::iterateUnitFinder) -------------------------------------------
    def _finder_columns(self):
        if self._finder is None:
            data = self._data
            count = data.unitSearchSize
            columns = []
            for array in (data.xUnitSearch, data.yUnitSearch):
                flat = (c_int32 * (2 * len(array))).from_buffer(array)[:2 * count]
                columns.append((flat[1::2], flat[0::2]))
            self._finder = columns
        return self._finder

    def _units_in_rectangle(self, left, top, right, bottom):
        global _MAX_UNIT_WIDTH, _MAX_UNIT_HEIGHT
        if _MAX_UNIT_WIDTH is None:
            _MAX_UNIT_WIDTH, _MAX_UNIT_HEIGHT = UnitTypes.maxUnitWidth(), UnitTypes.maxUnitHeight()
        (x_values, x_ids), (y_values, y_ids) = self._finder_columns()
        r, b = right, bottom
        width_extended = right - left + 1 < _MAX_UNIT_WIDTH
        # The official check compares top - bottom, which holds for every non-empty rectangle.
        height_extended = top - bottom + 1 < _MAX_UNIT_HEIGHT
        if width_extended:
            r += _MAX_UNIT_WIDTH
        if height_extended:
            b += _MAX_UNIT_HEIGHT
        x_start = bisect_left(x_values, left)
        x_end = bisect_right(x_values, r, x_start)
        y_start = bisect_left(y_values, top)
        y_end = bisect_right(y_values, b, y_start)
        flags: dict[int, int] = {}
        units = self._units
        for unit_id in x_ids[x_start:x_end]:
            if flags.get(unit_id, 0) == 0:
                if not width_extended or (0 <= unit_id < len(units) and units[unit_id].getLeft() <= right):
                    flags[unit_id] = 1
        for unit_id in y_ids[y_start:y_end]:
            if flags.get(unit_id, 0) == 1:
                if not height_extended or (0 <= unit_id < len(units) and units[unit_id].getTop() <= bottom):
                    flags[unit_id] = 2
        for unit_id in x_ids[x_start:x_end]:
            if flags.get(unit_id, 0) == 2 and 0 <= unit_id < len(units) and units[unit_id].exists():
                yield units[unit_id]
            flags[unit_id] = 0

    def getUnitsInRectangle(self, left, top, right=None, bottom=None, pred=None) -> Unitset:
        if _is_point(left):  # (Position topLeft, Position bottomRight, pred)
            pred = right
            (left, top), (right, bottom) = _xy(left), _xy(top)
        return Unitset(unit for unit in self._units_in_rectangle(left, top, right, bottom)
                       if _matches_predicate(unit, pred))

    def getClosestUnitInRectangle(self, center, pred=None, left=0, top=0, right=999999, bottom=999999) -> Unit | None:
        center = Position(*_xy(center))
        best, best_distance = None, 99999999
        for unit in self._units_in_rectangle(left, top, right, bottom):
            if _matches_predicate(unit, pred):
                distance = unit.getDistance(center)
                if distance < best_distance:
                    best, best_distance = unit, distance
        return best

    def getBestUnit(self, best, pred, center, radius: int = 999999) -> Unit | None:
        x, y = _xy(center)
        top_left = Position(x - radius, y - radius).makeValid()
        bottom_right = Position(x + radius, y + radius).makeValid()
        result = None
        for unit in self._units_in_rectangle(top_left.x, top_left.y, bottom_right.x, bottom_right.y):
            if _matches_predicate(unit, pred):
                result = unit if result is None else best(result, unit)
        return result

    def getUnitsOnTile(self, tileX, tileY=None, pred=None) -> Unitset:
        if _is_point(tileX):  # (TilePosition tile, pred)
            (x, y), pred = _xy(tileX), tileY
        else:
            x, y = _xy(tileX, tileY)
        if self._tile(x, y) is None:
            return Unitset()
        return self.getUnitsInRectangle(x * 32, y * 32, x * 32 + 32, y * 32 + 32, pred)

    def getUnitsInRadius(self, x, y=None, radius=None, pred=None) -> Unitset:
        if _is_point(x):  # (Position center, radius, pred), positional or by keyword
            if y is not None:
                radius, pred = y, pred if radius is None else radius
            x, y = _xy(x)
        center = Position(x, y)
        return self.getUnitsInRectangle(
            x - radius, y - radius, x + radius, y + radius,
            lambda unit: unit.getDistance(center) <= radius and _matches_predicate(unit, pred))

    def getClosestUnit(self, center, pred=None, radius: int = 999999) -> Unit | None:
        center = Position(*_xy(center))
        return self.getClosestUnitInRectangle(
            center, lambda unit: unit.getDistance(center) <= radius and _matches_predicate(unit, pred),
            center.x - radius, center.y - radius, center.x + radius, center.y + radius)

    # ---- damage (Game.cpp) --------------------------------------------------------------------
    def getDamageFrom(self, fromType, toType, fromPlayer=None, toPlayer=None) -> int:
        return _damage(UnitType(fromType), UnitType(toType), fromPlayer, toPlayer or self._self)

    def getDamageTo(self, toType, fromType, toPlayer=None, fromPlayer=None) -> int:
        return _damage(UnitType(fromType), UnitType(toType), fromPlayer or self._self, toPlayer)

    # ---- commands to the server ---------------------------------------------------------------
    def issueCommand(self, units, command) -> bool:
        success = False
        for unit in units:
            success = unit.issueCommand(command) or success
        return success

    def setScreenPosition(self, x, y=None) -> None:
        self._add_command(_gd.CommandType.SetScreenPosition, *_xy(x, y))

    def pingMinimap(self, x, y=None) -> None:
        self._add_command(_gd.CommandType.PingMinimap, *_xy(x, y))

    def enableFlag(self, flag: int) -> None:
        if 0 <= flag < _gd.FLAG_MAX and not self._data.flags[flag]:
            self._add_command(_gd.CommandType.EnableFlag, flag)

    def printf(self, format: str, *args) -> None:
        text = (format % args if args else format)[:255]
        if (index := self._add_string(text)) is not None:
            self._add_command(_gd.CommandType.Printf, index)

    vPrintf = printf

    def sendText(self, format: str, *args) -> None:
        self.sendTextEx(False, format, *args)

    vSendText = sendText

    def sendTextEx(self, toAllies: bool, format: str, *args) -> None:
        text = (format % args if args else format)[:255]
        if (index := self._add_string(text)) is not None:
            self._add_command(_gd.CommandType.SendText, index, 1 if toAllies else 0)

    vSendTextEx = sendTextEx

    def pauseGame(self) -> None: self._add_command(_gd.CommandType.PauseGame)
    def resumeGame(self) -> None: self._add_command(_gd.CommandType.ResumeGame)
    def leaveGame(self) -> None: self._add_command(_gd.CommandType.LeaveGame)
    def restartGame(self) -> None: self._add_command(_gd.CommandType.RestartGame)
    def setLocalSpeed(self, speed: int) -> None: self._add_command(_gd.CommandType.SetLocalSpeed, speed)

    def setFrameSkip(self, frameSkip: int) -> None:
        self.setLastError()
        if frameSkip > 0:
            self._add_command(_gd.CommandType.SetFrameSkip, frameSkip)
            return
        self.setLastError(Errors.Invalid_Parameter)

    def setLatCom(self, isEnabled: bool) -> None:
        self._data.hasLatCom = bool(isEnabled)
        self._add_command(_gd.CommandType.SetLatCom, 1 if isEnabled else 0)

    def setGUI(self, enabled: bool) -> None:
        self._data.hasGUI = bool(enabled)
        self._add_command(_gd.CommandType.SetGui, 1 if enabled else 0)

    def setCommandOptimizationLevel(self, level: int) -> None:
        self._add_command(_gd.CommandType.SetCommandOptimizerLevel, level)

    def setMap(self, mapFileName: str) -> bool:
        if not mapFileName or len(mapFileName.encode("utf-8")) >= 260:
            return self.setLastError(Errors.Invalid_Parameter)
        if not os.path.isfile(mapFileName):
            return self.setLastError(Errors.File_Not_Found)
        if (index := self._add_string(mapFileName)) is not None:
            self._add_command(_gd.CommandType.SetMap, index)
        return self.setLastError()

    def setAlliance(self, player, allied: bool = True, alliedVictory: bool = True) -> bool:
        if self._self is None or self.isReplay() or player is None or player is self._self:
            return self.setLastError(Errors.Invalid_Parameter)
        self._add_command(_gd.CommandType.SetAllies, player.getID(), (2 if alliedVictory else 1) if allied else 0)
        return self.setLastError()

    def setVision(self, player, enabled: bool = True) -> bool:
        if player is None:
            return self.setLastError(Errors.Invalid_Parameter)
        if not self.isReplay() and (self._self is None or player is self._self):
            return self.setLastError(Errors.Invalid_Parameter)
        self._add_command(_gd.CommandType.SetVision, player.getID(), 1 if enabled else 0)
        return self.setLastError()

    def setRevealAll(self, reveal: bool = True) -> bool:
        if not self.isReplay():
            return self.setLastError(Errors.Invalid_Parameter)
        self._add_command(_gd.CommandType.SetRevealAll, 1 if reveal else 0)
        return self.setLastError()

    # ---- drawing ------------------------------------------------------------------------------
    def setTextSize(self, size: int = 1) -> None:
        self._text_size = min(max(_index(size), 0), 3)

    def drawText(self, ctype, x, y, format: str, *args) -> None:
        if not self._data.hasGUI:
            return
        text = (format % args if args else format)[:2047]
        if (index := self._add_string(text)) is not None:
            self._add_shape(_gd.ShapeType.Text, ctype, x, y, 0, 0, index, self._text_size)

    vDrawText = drawText

    def _draw_text_at(self, ctype, args) -> None:
        if _is_point(args[0]):
            x, y = _xy(args[0])
            args = args[1:]
        else:
            x, y, args = args[0], args[1], args[2:]
        self.drawText(ctype, x, y, *args)

    def drawTextMap(self, *args) -> None: self._draw_text_at(_COORD_MAP, args)
    def drawTextMouse(self, *args) -> None: self._draw_text_at(_COORD_MOUSE, args)
    def drawTextScreen(self, *args) -> None: self._draw_text_at(_COORD_SCREEN, args)

    def drawBox(self, ctype, *args) -> None:
        left, top, right, bottom, color, *solid = _points(args, 2)
        self._add_shape(_gd.ShapeType.Box, ctype, left, top, right, bottom, 0, 0, color, bool(solid and solid[0]))

    def drawTriangle(self, ctype, *args) -> None:
        ax, ay, bx, by, cx, cy, color, *solid = _points(args, 3)
        self._add_shape(_gd.ShapeType.Triangle, ctype, ax, ay, bx, by, cx, cy, color, bool(solid and solid[0]))

    def drawCircle(self, ctype, *args) -> None:
        x, y, radius, color, *solid = _points(args, 1)
        self._add_shape(_gd.ShapeType.Circle, ctype, x, y, 0, 0, radius, 0, color, bool(solid and solid[0]))

    def drawEllipse(self, ctype, *args) -> None:
        x, y, xrad, yrad, color, *solid = _points(args, 1)
        self._add_shape(_gd.ShapeType.Ellipse, ctype, x, y, 0, 0, xrad, yrad, color, bool(solid and solid[0]))

    def drawDot(self, ctype, *args) -> None:
        x, y, color = _points(args, 1)
        self._add_shape(_gd.ShapeType.Dot, ctype, x, y, 0, 0, 0, 0, color, False)

    def drawLine(self, ctype, *args) -> None:
        x1, y1, x2, y2, color = _points(args, 2)
        self._add_shape(_gd.ShapeType.Line, ctype, x1, y1, x2, y2, 0, 0, color, False)

    def drawBoxMap(self, *args) -> None: self.drawBox(_COORD_MAP, *args)
    def drawBoxMouse(self, *args) -> None: self.drawBox(_COORD_MOUSE, *args)
    def drawBoxScreen(self, *args) -> None: self.drawBox(_COORD_SCREEN, *args)
    def drawTriangleMap(self, *args) -> None: self.drawTriangle(_COORD_MAP, *args)
    def drawTriangleMouse(self, *args) -> None: self.drawTriangle(_COORD_MOUSE, *args)
    def drawTriangleScreen(self, *args) -> None: self.drawTriangle(_COORD_SCREEN, *args)
    def drawCircleMap(self, *args) -> None: self.drawCircle(_COORD_MAP, *args)
    def drawCircleMouse(self, *args) -> None: self.drawCircle(_COORD_MOUSE, *args)
    def drawCircleScreen(self, *args) -> None: self.drawCircle(_COORD_SCREEN, *args)
    def drawEllipseMap(self, *args) -> None: self.drawEllipse(_COORD_MAP, *args)
    def drawEllipseMouse(self, *args) -> None: self.drawEllipse(_COORD_MOUSE, *args)
    def drawEllipseScreen(self, *args) -> None: self.drawEllipse(_COORD_SCREEN, *args)
    def drawDotMap(self, *args) -> None: self.drawDot(_COORD_MAP, *args)
    def drawDotMouse(self, *args) -> None: self.drawDot(_COORD_MOUSE, *args)
    def drawDotScreen(self, *args) -> None: self.drawDot(_COORD_SCREEN, *args)
    def drawLineMap(self, *args) -> None: self.drawLine(_COORD_MAP, *args)
    def drawLineMouse(self, *args) -> None: self.drawLine(_COORD_MOUSE, *args)
    def drawLineScreen(self, *args) -> None: self.drawLine(_COORD_SCREEN, *args)

    # ---- placement and production (Templates.h, Game.cpp) -------------------------------------

    def hasPowerPrecise(self, position, y=None, unitType=UnitType()) -> bool:
        if not isinstance(position, int):
            point = Position(*_xy(position))
            if y is not None:
                unitType = y
            x, y = point.x, point.y
        else:
            x = position
            if y is None:
                raise TypeError("a y coordinate is required")
        unit_type = UnitType(unitType)
        if unit_type < 228 and (not unit_type.requiresPsi() or not unit_type.isBuilding()):
            return True
        player = self.self()
        if player is None:
            return False
        # The true span of each row of Shared/Templates.h bPsiFieldMask.
        rows = ((5, 10), (2, 13), (1, 14), (0, 15), (0, 15), (0, 15), (0, 15), (1, 14), (2, 13), (5, 10))
        for unit in player.getUnits():
            if unit.getType() != 156 or not unit.exists() or not unit.isCompleted():
                continue
            point = unit.getPosition()
            dx, dy = x - point.x, y - point.y
            if abs(dx) >= 256 or abs(dy) >= 160:
                continue
            lo, hi = rows[(dy + 160) // 32]
            if lo <= (dx + 256) // 32 <= hi:
                return True
        return False

    def hasPower(self, *args, unitType=UnitType()) -> bool:
        if not args:
            raise TypeError("a tile coordinate is required")
        if isinstance(args[0], int):
            if len(args) < 2:
                raise TypeError("a tile y coordinate is required")
            x, y, *tail = args
        else:
            x, y = _xy(args[0], None)
            tail = list(args[1:])
        if len(tail) in (2, 3):
            width, height = tail[:2]
            if len(tail) == 3:
                unitType = tail[2]
            return self.hasPowerPrecise(x * 32 + width * 16, y * 32 + height * 16, unitType)
        if len(tail) > 1:
            raise TypeError("invalid arguments for the hasPower overloads")
        unit_type = UnitType(tail[0] if tail else unitType)
        if unit_type < 228:
            return self.hasPowerPrecise(x * 32 + unit_type.tileWidth() * 16, y * 32 + unit_type.tileHeight() * 16, unit_type)
        return self.hasPowerPrecise(x * 32, y * 32, UnitType())

    def canBuildHere(self, position, unit_type, builder=None, checkExplored=False) -> bool:
        unit_type = UnitType(unit_type)
        x, y = _xy(position, None)
        self.setLastError(Errors.Unbuildable_Location)
        if builder is not None and builder.game is not self:
            raise ValueError("the builder belongs to another match")
        if builder is not None and unit_type.isAddon():
            x, y = x + 4, y + 1
        right, bottom = x + unit_type.tileWidth(), y + unit_type.tileHeight()
        if x < 0 or y < 0 or x >= self.mapWidth() or y >= self.mapHeight() or right * 32 - 1 < 0 or bottom * 32 - 1 < 0 or right > self.mapWidth() or bottom > self.mapHeight():
            return False
        position = TilePosition(x, y)
        if unit_type.isRefinery():
            for geyser in self.getGeysers():
                if geyser.getTilePosition() == position:
                    if geyser.isVisible() and geyser.getType() != 188:
                        return False
                    return self.setLastError()
            return False
        for tx in range(x, right):
            for ty in range(y, bottom):
                if not self.isBuildable(tx, ty) or (checkExplored and not self.isExplored(tx, ty)):
                    return False
        center = Position(x * 32 + unit_type.tileWidth() * 16, y * 32 + unit_type.tileHeight() * 16)
        if builder is not None:
            if not builder.getType().isBuilding():
                if not builder.hasPath(center):
                    return False
            elif not builder.getType().isFlyingBuilding() and unit_type != 134 and not unit_type.isFlagBeacon():
                return False
        if unit_type != 214:
            for unit in self.getUnitsInRectangle(x * 32, y * 32, right * 32, bottom * 32):
                if unit.isFlying() or unit.isLoaded() or (unit is builder and unit_type != 134):
                    continue
                if (unit.getLeft() <= center.x + unit_type.dimensionRight()
                        and unit.getTop() <= center.y + unit_type.dimensionDown()
                        and unit.getRight() >= center.x - unit_type.dimensionLeft()
                        and unit.getBottom() >= center.y - unit_type.dimensionUp()):
                    if not (unit_type.isAddon() and unit.getType().canMove()):
                        return False
            needs_creep = unit_type.requiresCreep()
            if unit_type.getRace() != 0 or needs_creep:
                for tx in range(x, right):
                    for ty in range(y, bottom):
                        if needs_creep != self.hasCreep(tx, ty):
                            return False
            if unit_type.requiresPsi() and not self.hasPower(position, unit_type):
                return False
        if unit_type.isResourceDepot():
            for mineral in self.getStaticMinerals():
                tile = mineral.getInitialTilePosition()
                if (self.isVisible(tile) or self.isVisible(tile.x + 1, tile.y)) and not mineral.exists():
                    continue
                if x - 5 < tile.x < x + 7 and y - 4 < tile.y < y + 6:
                    return False
            for geyser in self.getStaticGeysers():
                tile = geyser.getInitialTilePosition()
                if x - 7 < tile.x < x + 7 and y - 5 < tile.y < y + 6:
                    return False
        if builder is not None and not builder.getType().isAddon() and unit_type.isAddon():
            if not self.canBuildHere(TilePosition(x - 4, y - 1), builder.getType(), builder, checkExplored):
                return False
        return self.setLastError()

    def getBuildLocation(self, unit_type, desiredPosition, maxRange=64, creep=False) -> TilePosition:
        unit_type = UnitType(unit_type)
        dx, dy = _xy(desiredPosition, None)
        self.setLastError()
        if not unit_type.isBuilding():
            self.setLastError(Errors.Incompatible_UnitType)
            return TilePosition(1000, 1000)
        trim = unit_type not in (106, 154, 131, 214)
        if unit_type == 156:
            target = self.getClosestUnit(Position(dx * 32, dy * 32), Filter.IsOwned & ~Filter.IsPowered)
            if target is not None:
                point = target.getPosition()
                dx, dy = point.x // 32, point.y // 32
                trim = False
        # PlacementReserve is always 64x64; maxRange only limits the search area.
        width = min(max(maxRange, 0), 64)
        lower = 32 - width // 2
        points = [(x, y) for y in range(lower, lower + width) for x in range(lower, lower + width)]
        sx, sy = dx - 32, dy - 32
        available = set()
        for x, y in points:
            if ((not unit_type.canBuildAddon() or self.canBuildHere(TilePosition(sx + x + 4, sy + y + 1), UnitType(124)))
                    and self.canBuildHere(TilePosition(sx + x, sy + y), unit_type)):
                available.add((x, y))
        for x, y in points:
            if not self.hasPath(Position(dx * 32, dy * 32), Position((sx + x) * 32, (sy + y) * 32)):
                available.discard((x, y))
        available = {(x, y) for x, y in available if 0 <= sx + x < self.mapWidth() and 0 <= sy + y < self.mapHeight()}

        def reserve_rectangle(left, top, right, bottom):
            available.difference_update((x, y) for x, y in tuple(available) if left <= x < right and top <= y < bottom)

        def reserve_structure(unit, padding):
            point = unit.getPosition()
            extra = unit.getType()
            pw, ph = extra.tileWidth() + padding * 2, extra.tileHeight() + padding * 2
            left = point.x // 32 - unit_type.tileWidth() - pw // 2 - 1 - dx + 32
            top = point.y // 32 - unit_type.tileHeight() - ph // 2 - 1 - dy + 32
            reserve_rectangle(left, top, left + unit_type.tileWidth() + pw + 1, top + unit_type.tileHeight() + ph + 1)

        if available:
            backup = available.copy()
            height = self.getGroundHeight(dx, dy)
            for x, y in points:
                if self.getGroundHeight(sx + x, sy + y) != height:
                    available.discard((x, y))
            if not available:
                available = backup
            player = self.self()
            if not unit_type.isResourceDepot():
                if not unit_type.isAddon():
                    backup = available.copy()
                    for unit in player.getUnits():
                        kind = unit.getType()
                        if (unit.exists() and (unit.isCompleted() or (kind.producesLarva() and unit.isMorphing()))
                                and kind.isBuilding() and (kind.isResourceDepot() or kind.isRefinery())):
                            reserve_structure(unit, 2)
                    if unit_type != 125:
                        for unit in self.getNeutralUnits():
                            if unit.exists() and unit.getType().isResourceContainer():
                                reserve_structure(unit, 2)
                    if not available:
                        available = backup
                backup = available.copy()
                for unit in player.getUnits():
                    if unit.exists() and unit.getType().canBuildAddon():
                        tile = unit.getTilePosition()
                        left, top = tile.x + 4 - sx, tile.y + 1 - sy
                        reserve_rectangle(left, top, left + 2, top + 2)
                if not available:
                    available = backup
                if unit_type not in (156, 125, 124, 162, 143):
                    backup = available.copy()
                    for unit in player.getUnits():
                        if unit.exists():
                            reserve_structure(unit, 1 if unit.getType() in (111, 125, 143) else 2)
                    # ReserveDefault's direction loop clears again only cells that were 0.
                    if not available:
                        available = backup
        # ReservePlacement does not use the creep argument either.
        if trim:
            backup = available.copy()
            available = {(x, y) for x, y in available if x not in (32, 31, 33, 30, 29, 28, 27) and y not in (32, 31, 33, 30, 29, 28)}
            if not available:
                available = backup
        best = fallback = TilePosition(1000, 1001)
        best_distance = fallback_distance = 999999
        for x, y in sorted(available, key=lambda p: (p[1], p[0])):
            longest, shortest = sorted((abs(32 - x), abs(32 - y)), reverse=True)
            minor = (3 * shortest) >> 3
            distance = longest if shortest <= longest >> 2 else (minor >> 5) + minor + longest - (longest >> 4) - (longest >> 6)
            if distance < best_distance:
                if distance <= maxRange:
                    best_distance, best = distance, TilePosition(sx + x, sy + y)
                elif distance < fallback_distance:
                    fallback_distance, fallback = distance, TilePosition(sx + x, sy + y)
        return fallback if best == TilePosition(1000, 1001) else best

    def canMake(self, unit_type, builder=None) -> bool:
        unit_type = UnitType(unit_type)
        self.setLastError()
        player = self.self()
        if player is None:
            return self.setLastError(Errors.Unit_Not_Owned)
        if not player.isUnitAvailable(unit_type):
            return self.setLastError(Errors.Access_Denied)
        required = unit_type.whatBuilds()[0]
        if builder is not None:
            if builder.game is not self or builder.getPlayer() is not player:
                return self.setLastError(Errors.Unit_Not_Owned)
            builder_type = builder.getType()
            if unit_type == 134 and builder_type == 134:
                if not builder.isCompleted():
                    return self.setLastError(Errors.Unit_Busy)
                if builder.getNydusExit() is not None:
                    return self.setLastError(Errors.Unknown)
                return True
            if required == 35 and builder_type.producesLarva():
                if not builder.getLarva():
                    return self.setLastError(Errors.Unit_Does_Not_Exist)
            elif builder_type != required:
                return self.setLastError(Errors.Incompatible_UnitType)
            if builder_type in (72, 82):
                capacity = 4 + (4 if player.getUpgradeLevel(43) > 0 or builder_type == 82 else 0)
                if builder.getInterceptorCount() + len(builder.getTrainingQueue()) >= capacity:
                    return self.setLastError(Errors.Insufficient_Space)
            elif builder_type in (83, 81):
                capacity = 5 + (5 if player.getUpgradeLevel(36) > 0 or builder_type == 81 else 0)
                if builder.getScarabCount() + len(builder.getTrainingQueue()) >= capacity:
                    return self.setLastError(Errors.Insufficient_Space)
        if player.minerals() < unit_type.mineralPrice():
            return self.setLastError(Errors.Insufficient_Minerals)
        if player.gas() < unit_type.gasPrice():
            return self.setLastError(Errors.Insufficient_Gas)
        race = unit_type.getRace()
        supply = unit_type.supplyRequired() * (2 if unit_type.isTwoUnitsInOneEgg() else 1)
        if supply > 0 and player.supplyTotal(race) < player.supplyUsed(race) + supply - (required.supplyRequired() if required.getRace() == race else 0):
            return self.setLastError(Errors.Insufficient_Supply)
        addon = UnitType()
        for required_unit, count in sorted(unit_type.requiredUnits().items()):
            if required_unit.isAddon():
                addon = required_unit
            if not player.hasUnitTypeRequirement(required_unit, count):
                return self.setLastError(Errors.Insufficient_Tech)
        if unit_type.requiredTech() != 44 and not player.hasResearched(unit_type.requiredTech()):
            return self.setLastError(Errors.Insufficient_Tech)
        if builder is not None and addon != 228 and addon.whatBuilds()[0] == unit_type.whatBuilds()[0]:
            actual_addon = builder.getAddon()
            if actual_addon is None or actual_addon.getType() != addon:
                return self.setLastError(Errors.Insufficient_Tech)
        return True

    def canResearch(self, tech, unit=None, checkCanIssueCommandType=True) -> bool:
        tech = TechType(tech)
        player = self.self()
        if player is None:
            return self.setLastError(Errors.Unit_Not_Owned)
        if unit is not None:
            if unit.game is not self or unit.getPlayer() is not player:
                return self.setLastError(Errors.Unit_Not_Owned)
            if not unit.getType().isSuccessorOf(tech.whatResearches()):
                return self.setLastError(Errors.Incompatible_UnitType)
            if checkCanIssueCommandType and (unit.isLifted() or not unit.isIdle() or not unit.isCompleted()):
                return self.setLastError(Errors.Unit_Busy)
        if player.isResearching(tech):
            return self.setLastError(Errors.Currently_Researching)
        if player.hasResearched(tech):
            return self.setLastError(Errors.Already_Researched)
        if not player.isResearchAvailable(tech):
            return self.setLastError(Errors.Access_Denied)
        if player.minerals() < tech.mineralPrice():
            return self.setLastError(Errors.Insufficient_Minerals)
        if player.gas() < tech.gasPrice():
            return self.setLastError(Errors.Insufficient_Gas)
        if not player.hasUnitTypeRequirement(tech.requiredUnit()):
            return self.setLastError(Errors.Insufficient_Tech)
        return self.setLastError()

    def canUpgrade(self, upgrade, unit=None, checkCanIssueCommandType=True) -> bool:
        upgrade = UpgradeType(upgrade)
        player = self.self()
        if player is None:
            return self.setLastError(Errors.Unit_Not_Owned)
        if unit is not None:
            if unit.game is not self or unit.getPlayer() is not player:
                return self.setLastError(Errors.Unit_Not_Owned)
            if not unit.getType().isSuccessorOf(upgrade.whatUpgrades()):
                return self.setLastError(Errors.Incompatible_UnitType)
            if checkCanIssueCommandType and (unit.isLifted() or not unit.isIdle() or not unit.isCompleted()):
                return self.setLastError(Errors.Unit_Busy)
        level = player.getUpgradeLevel(upgrade) + 1
        if not player.hasUnitTypeRequirement(upgrade.whatUpgrades()):
            return self.setLastError(Errors.Unit_Does_Not_Exist)
        if not player.hasUnitTypeRequirement(upgrade.whatsRequired(level)):
            return self.setLastError(Errors.Insufficient_Tech)
        if player.isUpgrading(upgrade):
            return self.setLastError(Errors.Currently_Upgrading)
        if level - 1 >= player.getMaxUpgradeLevel(upgrade):
            return self.setLastError(Errors.Fully_Upgraded)
        if player.minerals() < upgrade.mineralPrice(level):
            return self.setLastError(Errors.Insufficient_Minerals)
        if player.gas() < upgrade.gasPrice(level):
            return self.setLastError(Errors.Insufficient_Gas)
        return self.setLastError()


def _damage(from_type: UnitType, to_type: UnitType, from_player, to_player) -> int:
    """Game.cpp getDamageFromImpl."""
    weapon = from_type.airWeapon() if to_type.isFlyer() else from_type.groundWeapon()
    if weapon in (130, 131):  # WeaponTypes::None, Unknown
        return 0
    damage = from_player.damage(weapon) if from_player is not None else weapon.damageAmount() * weapon.damageFactor()
    if weapon.damageType() != 4 and to_player is not None:  # DamageTypes::Ignore_Armor
        damage -= min(damage, to_player.armor(to_type))
    return damage * _DAMAGE_RATIO[weapon.damageType()][to_type.size()] // 256
