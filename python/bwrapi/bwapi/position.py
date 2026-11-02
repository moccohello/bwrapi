"""Owned, mutable values of BWAPI's Point<int, 1/8/32>.

Arithmetic follows defined 32-bit int behaviour. Where C++ signed overflow
would be undefined, the operation raises OverflowError instead. Arbitrary
C++ template arguments T and stream input are not part of these three
named int bindings.
"""

from collections import deque
from math import isfinite, sqrt
from numbers import Real
from operator import index

from .global_game import _get_current_game

_MISSING = object()
_I32_MIN, _I32_MAX = -(1 << 31), (1 << 31) - 1


def _int32(value):
    if type(value) is int:  # The common case, without the numeric ABC checks.
        if not _I32_MIN <= value <= _I32_MAX:
            raise OverflowError("Point operation exceeds the defined signed 32-bit range")
        return value
    if isinstance(value, Real):
        if not isfinite(value):
            raise ValueError("Point coordinates must be finite")
        value = int(value)
    else:
        value = index(value)
    if not _I32_MIN <= value <= _I32_MAX:
        raise OverflowError("Point operation exceeds the defined signed 32-bit range")
    return value


def _divide(numerator, denominator):
    if numerator == _I32_MIN and denominator == -1:
        raise OverflowError("signed 32-bit division is undefined for INT_MIN / -1")
    quotient = abs(numerator) // abs(denominator)
    return -quotient if (numerator < 0) != (denominator < 0) else quotient


def _scale(point):
    scale = getattr(point, "_scale", None)
    if scale in (1, 8, 32) and hasattr(point, "x") and hasattr(point, "y"):
        return scale
    raise TypeError("expected a BWAPI point")


def _convert_coordinate(value, from_scale, to_scale):
    if from_scale == to_scale:
        return _int32(value)
    if from_scale > to_scale:
        return _int32(value * (from_scale // to_scale))
    divisor = to_scale // from_scale
    # C++ performs division in FromT before casting to the destination int.
    return _int32(value / divisor) if isinstance(value, float) else _int32(_divide(index(value), divisor))


class _PointMethods:
    __slots__ = ()
    x: int
    y: int
    _scale = 1
    list = deque

    def __init__(self, x=_MISSING, y=_MISSING):
        if x is _MISSING and y is _MISSING:
            x, y = 0, 0
        elif y is _MISSING:
            source_scale = _scale(x)
            x, y = (_convert_coordinate(x.x, source_scale, self._scale),
                    _convert_coordinate(x.y, source_scale, self._scale))
        elif x is _MISSING:
            raise TypeError("Point needs both x and y")
        object.__setattr__(self, "_readonly", False)
        self._store(x, y)

    def __setattr__(self, name, value):
        self._ensure_mutable()
        if name in ("x", "y"):
            object.__setattr__(self, name, _int32(value))
        else:
            object.__setattr__(self, name, value)

    def __delattr__(self, name):
        self._ensure_mutable()
        object.__delattr__(self, name)

    def _ensure_mutable(self):
        if getattr(self, "_readonly", False):
            raise AttributeError("this BWAPI point is const; construct a point copy to modify it")

    def _store(self, x, y):
        self._ensure_mutable()
        x, y = _int32(x), _int32(y)
        object.__setattr__(self, "x", x)
        object.__setattr__(self, "y", y)
        return self

    def _operand(self, other):
        if _scale(other) != self._scale:
            raise TypeError("Point operands require the same scale; convert the other point explicitly")
        return _int32(other.x), _int32(other.y)

    def __copy__(self): return type(self)(self.x, self.y)
    def __deepcopy__(self, memo):
        copied = self.__copy__()
        memo[id(self)] = copied
        return copied

    def __hash__(self):
        if not getattr(self, "_readonly", False):
            raise TypeError("mutable Point values are unhashable")
        return hash((self.x, self.y))

    def __eq__(self, other):
        try:
            x, y = self._operand(other)
        except TypeError:
            return NotImplemented
        return self.x == x and self.y == y

    def __ne__(self, other):
        equal = self.__eq__(other)
        return NotImplemented if equal is NotImplemented else not equal

    def __lt__(self, other):
        try:
            x, y = self._operand(other)
        except TypeError:
            return NotImplemented
        return (self.x, self.y) < (x, y)

    def __str__(self): return f"({self.x},{self.y})"
    def __repr__(self): return f"{type(self).__name__}({self.x}, {self.y})"
    def __bool__(self): return self.isValid()

    def __iadd__(self, other):
        x, y = self._operand(other)
        return self._store(self.x + x, self.y + y)
    def __add__(self, other):
        result = self.__copy__()
        return result.__iadd__(other)
    def __isub__(self, other):
        x, y = self._operand(other)
        return self._store(self.x - x, self.y - y)
    def __sub__(self, other):
        result = self.__copy__()
        return result.__isub__(other)
    def __imul__(self, value):
        value = _int32(value)
        return self._store(self.x * value, self.y * value)
    def __mul__(self, value):
        result = self.__copy__()
        return result.__imul__(value)
    def __ior__(self, value):
        value = _int32(value)
        return self._store(self.x | value, self.y | value)
    def __or__(self, value):
        result = self.__copy__()
        return result.__ior__(value)
    def __iand__(self, value):
        value = _int32(value)
        return self._store(self.x & value, self.y & value)
    def __and__(self, value):
        result = self.__copy__()
        return result.__iand__(value)
    def __ixor__(self, value):
        value = _int32(value)
        return self._store(self.x ^ value, self.y ^ value)
    def __xor__(self, value):
        result = self.__copy__()
        return result.__ixor__(value)
    def __itruediv__(self, value):
        value = _int32(value)
        if value == 0:
            return self._store(32000 // self._scale, 32000 // self._scale)
        return self._store(_divide(self.x, value), _divide(self.y, value))
    def __truediv__(self, value):
        result = self.__copy__()
        return result.__itruediv__(value)
    def __imod__(self, value):
        value = _int32(value)
        if value == 0:
            return self._store(32000 // self._scale, 32000 // self._scale)
        return self._store(self.x - _divide(self.x, value) * value,
                           self.y - _divide(self.y, value) * value)
    def __mod__(self, value):
        result = self.__copy__()
        return result.__imod__(value)

    def isValid(self):
        if self.x < 0 or self.y < 0:
            return False
        game = _get_current_game()
        if game is None:
            return self.x < 8192 // self._scale and self.y < 8192 // self._scale
        if self.x >= game.mapWidth() * 32 // self._scale:
            return False
        return self.y < game.mapHeight() * 32 // self._scale

    def makeValid(self):
        self._ensure_mutable()
        game = _get_current_game()
        width, height = (256, 256) if game is None else (game.mapWidth(), game.mapHeight())
        return self._store(min(max(self.x, 0), width * 32 // self._scale - 1),
                           min(max(self.y, 0), height * 32 // self._scale - 1))

    def getDistance(self, position): return (self - position).getLength()
    def getLength(self):
        x, y = float(self.x), float(self.y)
        return sqrt(x * x + y * y)
    def getApproxDistance(self, position):
        x, y = self._operand(position)
        dx, dy = _int32(self.x - x), _int32(self.y - y)
        if dx == _I32_MIN or dy == _I32_MIN:
            raise OverflowError("C++ abs(INT_MIN) is outside the defined range")
        maximum, minimum = max(abs(dx), abs(dy)), min(abs(dx), abs(dy))
        if minimum <= maximum >> 2:
            return maximum
        minimum_calc = ((3 * minimum) & 0xFFFFFFFF) >> 3
        result = ((minimum_calc >> 5) + minimum_calc + maximum
                  - (maximum >> 4) - (maximum >> 6)) & 0xFFFFFFFF
        return result if result <= _I32_MAX else result - (1 << 32)

    def setMax(self, max_x, max_y=_MISSING):
        x, y = self._operand(max_x) if max_y is _MISSING else (_int32(max_x), _int32(max_y))
        return self._store(min(self.x, x), min(self.y, y))
    def setMin(self, min_x, min_y=_MISSING):
        x, y = self._operand(min_x) if min_y is _MISSING else (_int32(min_x), _int32(min_y))
        return self._store(max(self.x, x), max(self.y, y))


class Point(_PointMethods):
    """A Point<int,1> value on the same pixel scale as Position."""
    __slots__ = ("x", "y", "_readonly")


class Position(Point):
    __slots__ = ()


class TilePosition(_PointMethods):
    __slots__ = ("x", "y", "_readonly")
    _scale = 32


class WalkPosition(_PointMethods):
    __slots__ = ("x", "y", "_readonly")
    _scale = 8


def _readonly_copy(value):
    point_class = {1: Position, 8: WalkPosition, 32: TilePosition}[_scale(value)]
    copied = point_class(value)
    object.__setattr__(copied, "_readonly", True)
    return copied


__all__ = ["Point", "Position", "TilePosition", "WalkPosition"]
