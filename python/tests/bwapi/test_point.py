import copy
import ast
import hashlib
import json
import os
import random
import shutil
import subprocess
from pathlib import Path

import pytest

from bwrapi.bwapi import global_game
from bwrapi.bwapi.api_namespaces import Positions, TilePositions, WalkPositions
from bwrapi.bwapi.position import (
    Point, Position, TilePosition, WalkPosition,
)
from upstream_root import SOURCE, requires_upstream


_ROOT = Path(__file__).resolve().parents[3]
_UPSTREAM = SOURCE
_PINNED = {
    "include/BWAPI/Position.h": "84bd6df2c4349e591681746ab000b644345d793d6efed0cea14386413f34314d",
    "BWAPILIB/Source/Position.cpp": "b415528f850e8139d92f08395e198326689867ebcc7d6524474c72f5012116f0",
}
_ORACLE_EVIDENCE = {}


class _Game:
    _retired = False

    def __init__(self, width, height): self.width, self.height = width, height
    def mapWidth(self): return self.width
    def mapHeight(self): return self.height


@pytest.fixture(autouse=True)
def _restore_global():
    previous = global_game._get_current_binding()
    global_game._set_current_game(None)
    yield
    global_game._set_current_game(previous[0], owner=previous[1])


@pytest.mark.parametrize("cls,scale", [(Point, 1), (Position, 1), (WalkPosition, 8), (TilePosition, 32)])
def test_named_int_point_defaults_mutability_and_owned_copies(cls, scale):
    point = cls()
    assert (point.x, point.y) == (0, 0)
    point.x, point.y = 5.7, -8.2
    assert (point.x, point.y) == (5, -8)
    assert str(point) == "(5,-8)"
    for copied in (cls(point), copy.copy(point), copy.deepcopy(point)):
        assert copied is not point and copied == point
        copied.x = 99
        assert point.x == 5
    assert list(cls.list([point])) == [point]
    with pytest.raises(TypeError): hash(point)


@pytest.mark.parametrize("cls,namespace,scale", [(Position, Positions, 1), (WalkPosition, WalkPositions, 8), (TilePosition, TilePositions, 32)])
def test_constants_const_value_copies_and_zero_divisor(cls, namespace, scale):
    expected = {"Invalid": (32000 // scale, 32000 // scale),
                "None": (32000 // scale, 32032 // scale),
                "Unknown": (32000 // scale, 32064 // scale), "Origin": (0, 0)}
    for name, coordinates in expected.items():
        constant = getattr(namespace, name)
        assert isinstance(constant, cls) and (constant.x, constant.y) == coordinates
        assert hash(constant) == hash(coordinates)
        with pytest.raises(AttributeError): constant.x = 2
        with pytest.raises(AttributeError): constant.makeValid()
        with pytest.raises(AttributeError): constant.__iadd__(cls(1, 1))
        copied = cls(constant)
        copied.x = 3
        assert constant.x == coordinates[0]
    for result in (cls(-13, 17) / 0, cls(-13, 17) % 0):
        assert result == namespace.Invalid
    assert namespace.None_ is getattr(namespace, "None")


@pytest.mark.parametrize("source_class", [Position, WalkPosition, TilePosition])
@pytest.mark.parametrize("destination_class", [Position, WalkPosition, TilePosition])
def test_explicit_scale_conversion_and_cpp_negative_truncation(source_class, destination_class):
    source = source_class(-33, 63)
    from_scale, to_scale = source._scale, destination_class._scale
    if from_scale > to_scale:
        expected = (-33 * (from_scale // to_scale), 63 * (from_scale // to_scale))
    else:
        ratio = to_scale // from_scale
        expected = (-(33 // ratio), 63 // ratio)
    result = destination_class(source)
    assert (result.x, result.y) == expected
    assert (source.x, source.y) == (-33, 63)


@pytest.mark.parametrize("cls,scale", [(Position, 1), (WalkPosition, 8), (TilePosition, 32)])
def test_validity_bounds_inplace_references_and_unknown(cls, scale):
    maximum = 8192 // scale - 1
    assert bool(cls(maximum, maximum))
    assert not bool(cls(maximum + 1, 0)) and not bool(cls(-1, 0))
    point = cls(-4, maximum + 10)
    assert point.makeValid() is point and point == cls(0, maximum)
    global_game._set_current_game(_Game(8, 4))
    assert point.makeValid() is point and point == cls(0, 128 // scale - 1)
    assert not cls(256 // scale, 0).isValid()
    class Unknown(_Game):
        def mapWidth(self): return 0  # official default for an unknown map
    global_game._set_current_game(Unknown(8, 4))
    assert cls(-1, 0).isValid() is False
    assert cls(0, 0).isValid() is False


def test_lexicographic_point_operators_and_cpp_remainder_signs():
    assert Position(1, 9) < Position(2, -9)
    assert Position(1, 2) < Position(1, 3)
    assert Position(-13, 17) / -5 == Position(2, -3)
    assert Position(-13, 17) % -5 == Position(-3, 2)
    point = Position(3, 9)
    assert point.setMax(2, 12) is point and point == Position(2, 9)
    assert point.setMin(Position(4, 8)) is point and point == Position(4, 9)
    assert point.setMax(Position(10, 2)) is point and point == Position(4, 2)
    assert point.setMin(0, 7) is point and point == Position(4, 7)
    for name, argument in (("__iadd__", Position(1, 1)), ("__isub__", Position(1, 1)),
                           ("__imul__", 2), ("__itruediv__", 2), ("__imod__", 3),
                           ("__ior__", 1), ("__iand__", 7), ("__ixor__", 2)):
        assert getattr(point, name)(argument) is point
    with pytest.raises(TypeError): Position(1, 2) + TilePosition(1, 2)


def test_defined_i32_range_does_not_fabricate_signed_overflow():
    with pytest.raises(OverflowError): Position(1 << 31, 0)
    with pytest.raises(OverflowError): Position((1 << 31) - 1, 0) + Position(1, 0)
    with pytest.raises(OverflowError): Position(-(1 << 31), 0) / -1
    with pytest.raises(OverflowError): Position(-(1 << 31), 0) % -1
    with pytest.raises(OverflowError): Position(-(1 << 31), 0).getApproxDistance(Position(0, 0))
    with pytest.raises(ValueError): Position(float("nan"), 0)


def test_stub_named_bindings_and_public_return_types_are_concrete():
    stub = ast.parse((_ROOT / "python/bwrapi/bwapi/position.pyi").read_text())
    classes = {node.name: node for node in stub.body if isinstance(node, ast.ClassDef)}
    assert {"Point", "Position", "WalkPosition", "TilePosition"} <= set(classes)
    methods = {node.name: node for node in classes["_PointMethods"].body if isinstance(node, ast.FunctionDef)}
    required = {"__init__", "__bool__", "__eq__", "__ne__", "__lt__", "__str__",
                "__add__", "__iadd__", "__sub__", "__isub__", "__mul__", "__imul__",
                "__truediv__", "__itruediv__", "__mod__", "__imod__", "__or__", "__ior__",
                "__and__", "__iand__", "__xor__", "__ixor__", "isValid", "makeValid",
                "getDistance", "getLength", "getApproxDistance", "setMax", "setMin"}
    assert required <= methods.keys()
    assert all(methods[name].returns is not None for name in required)
    assert ast.unparse(methods["getDistance"].returns) == "float"
    assert ast.unparse(methods["getApproxDistance"].returns) == "int"
    assert ast.unparse(methods["makeValid"].returns) == "Self"
    assert ast.unparse(classes["Position"].bases[0]) == "Point"
    for name in ("Point", "WalkPosition", "TilePosition"):
        assert ast.unparse(classes[name].bases[0]) == "_PointMethods"


_ORACLE = r'''
#include <BWAPI/Position.h>
#include <BWAPI/Game.h>
#include <iomanip>
#include <sstream>
namespace BWAPI { Game* BroodwarPtr=nullptr; Game* Broodwar=nullptr; }
template<class P> void out(const P& p) { std::cout << '[' << p.x << ',' << p.y << ']'; }
template<int S> void row(int x,int y,int u,int v,int k,int width,int height) {
  using P=BWAPI::Point<int,S>;
  BWAPI::Game game{width,height};
  BWAPI::BroodwarPtr=BWAPI::Broodwar=width<0?nullptr:&game;
  P a{x,y}, b{u,v}, c;
  std::cout << std::setprecision(17) << '{';
  std::cout << "\"default\":"; out(c);
  std::cout << ",\"sum\":"; out(a+b);
  std::cout << ",\"difference\":"; out(a-b);
  std::cout << ",\"product\":"; out(a*k);
  std::cout << ",\"quotient\":"; out(a/k);
  std::cout << ",\"remainder\":"; out(a%k);
  std::cout << ",\"or\":"; out(a|k);
  std::cout << ",\"and\":"; out(a&k);
  std::cout << ",\"xor\":"; out(a^k);
  std::cout << ",\"comparison\":[" << (a==b) << ',' << (a!=b) << ',' << (a<b) << ']';
  std::cout << ",\"valid\":" << a.isValid();
  c=a; c.makeValid(); std::cout << ",\"clamped\":"; out(c);
  std::cout << ",\"distance\":" << a.getDistance(b);
  std::cout << ",\"length\":" << a.getLength();
  std::cout << ",\"approx\":" << a.getApproxDistance(b);
  c=a; c.setMin(u,v); std::cout << ",\"min_pair\":"; out(c);
  c=a; c.setMax(u,v); std::cout << ",\"max_pair\":"; out(c);
  c=a; c.setMin(b); std::cout << ",\"min_point\":"; out(c);
  c=a; c.setMax(b); std::cout << ",\"max_point\":"; out(c);
  c=a; bool refs=(&(c+=b)==&c); refs=refs&&(&(c-=b)==&c);
  refs=refs&&(&(c*=k)==&c); c=a; refs=refs&&(&(c/=k)==&c);
  c=a; refs=refs&&(&(c%=k)==&c); c=a; refs=refs&&(&(c|=k)==&c);
  refs=refs&&(&(c&=k)==&c); refs=refs&&(&(c^=k)==&c);
  std::cout << ",\"references\":" << refs;
  std::cout << ",\"pixel\":"; out(BWAPI::Position(a));
  std::cout << ",\"walk\":"; out(BWAPI::WalkPosition(a));
  std::cout << ",\"tile\":"; out(BWAPI::TilePosition(a));
  std::cout << ",\"float_cast\":"; out(P(BWAPI::Point<double,S>{5.7,-8.2}));
  std::ostringstream stream; stream << a;
  std::cout << ",\"stream\":\"" << stream.str() << "\"}\n";
}
int main() {
  int scale,x,y,u,v,k,w,h;
  while(std::cin>>scale>>x>>y>>u>>v>>k>>w>>h) {
    if(scale==1) row<1>(x,y,u,v,k,w,h);
    else if(scale==8) row<8>(x,y,u,v,k,w,h);
    else row<32>(x,y,u,v,k,w,h);
  }
}
'''


def _linux_path(path):
    rendered = str(path).replace("\\", "/")
    if rendered.startswith("//wsl$/Ubuntu-22.04/"):
        return "/" + rendered.split("//wsl$/Ubuntu-22.04/", 1)[1]
    if len(rendered) > 2 and rendered[1] == ":":
        return "/mnt/" + rendered[0].lower() + rendered[2:]
    return rendered


def _run_oracle(tmp_path, inputs):
    golden_path = os.environ.get("BWRAPI_POINT_ORACLE_GOLDEN")
    if golden_path:
        golden = json.loads(Path(golden_path).read_text())["oracle"]
        assert golden["inputs_sha256"] == hashlib.sha256(inputs.encode()).hexdigest()
        assert golden["oracle_source_sha256"] == hashlib.sha256(_ORACLE.encode()).hexdigest()
        assert golden["pinned_sources"] == _PINNED
        _ORACLE_EVIDENCE.update(golden)
        return golden["expected_rows"]
    if os.name == "nt":
        prefix = ["wsl.exe", "-d", "Ubuntu-22.04", "--"]
        compiler = subprocess.run(prefix + ["sh", "-c", "command -v g++"], capture_output=True, text=True)
        if compiler.returncode:
            pytest.fail("WSL g++ is unavailable; provide the pinned C++ golden fixture via BWRAPI_POINT_ORACLE_GOLDEN")
        compiler_path = compiler.stdout.strip()
    else:
        prefix = []
        compiler_path = shutil.which("g++")
        if compiler_path is None:
            pytest.fail("g++ is unavailable; provide the pinned C++ golden fixture via BWRAPI_POINT_ORACLE_GOLDEN")
    stub_dir = tmp_path / "include/BWAPI"
    stub_dir.mkdir(parents=True)
    (stub_dir / "Game.h").write_text(
        "#pragma once\nnamespace BWAPI { struct Game { int width,height; int mapWidth()const{return width;} int mapHeight()const{return height;} }; extern Game* BroodwarPtr; extern Game* Broodwar; }\n"
    )
    main = tmp_path / "point-oracle.cpp"
    main.write_text(_ORACLE)
    executable = tmp_path / "point-oracle"
    command = prefix + [compiler_path, "-std=c++17", "-O0", "-Wno-unknown-pragmas",
                        "-I" + _linux_path(tmp_path / "include"), "-I" + _linux_path(_UPSTREAM / "include"),
                        _linux_path(main), _linux_path(_UPSTREAM / "BWAPILIB/Source/Position.cpp"),
                        "-o", _linux_path(executable)]
    compiled = subprocess.run(command, capture_output=True, text=True, timeout=60)
    assert compiled.returncode == 0, compiled.stdout + compiled.stderr
    output = subprocess.run(prefix + [_linux_path(executable)], input=inputs, capture_output=True, text=True, timeout=30)
    assert output.returncode == 0, output.stderr
    rows = [json.loads(line) for line in output.stdout.splitlines()]
    version = subprocess.run(prefix + [compiler_path, "--version"], capture_output=True, text=True, timeout=10)
    assert version.returncode == 0, version.stderr
    _ORACLE_EVIDENCE.update({
        "compiler": version.stdout.splitlines()[0], "compiled_and_executed": True,
        "inputs_sha256": hashlib.sha256(inputs.encode()).hexdigest(),
        "oracle_source_sha256": hashlib.sha256(_ORACLE.encode()).hexdigest(),
        "pinned_sources": _PINNED, "inputs": inputs, "expected_rows": rows,
        "matrix_rows": len(rows), "bounds": "unbound 256x256 or isolated current-map stub",
    })
    return rows


@requires_upstream
def test_independent_pinned_cpp_oracle_named_point_matrix(tmp_path):
    generator = random.Random(105)
    cases = []
    for scale in (1, 8, 32):
        for values in ((-33,63,7,-9,-5), (0,0,0,0,0), (32000,32032,3,4,1),
                       (31,32,31,33,2), (-17,-13,-17,-13,0)):
            for bounds in ((-1,-1), (8,4)):
                cases.append((scale, *values, *bounds))
        for _ in range(100):
            cases.append((scale, *(generator.randint(-1000,1000) for _ in range(4)),
                          generator.randint(-7,7), generator.randint(1,256), generator.randint(1,256)))
    rows = _run_oracle(tmp_path, "".join(" ".join(map(str, case)) + "\n" for case in cases))
    assert len(rows) == len(cases) == 330
    classes = {1: Position, 8: WalkPosition, 32: TilePosition}
    for case, expected in zip(cases, rows):
        scale,x,y,u,v,k,width,height = case
        cls = classes[scale]
        global_game._set_current_game(None if width < 0 else _Game(width,height))
        a,b = cls(x,y),cls(u,v)
        reference_results = []
        for method, argument in (("__iadd__",b), ("__isub__",b), ("__imul__",k),
                                 ("__itruediv__",k), ("__imod__",k), ("__ior__",k),
                                 ("__iand__",k), ("__ixor__",k)):
            owned = cls(a)
            reference_results.append(getattr(owned, method)(argument) is owned)
        actual = {
            "default": [cls().x,cls().y],
            "sum": a+b, "difference": a-b, "product": a*k, "quotient": a/k,
            "remainder": a%k, "or": a|k, "and": a&k, "xor": a^k,
            "comparison": [int(a==b), int(a!=b), int(a<b)], "valid": int(a.isValid()),
            "clamped": cls(a).makeValid(), "distance": a.getDistance(b), "length": a.getLength(),
            "approx": a.getApproxDistance(b), "min_pair": cls(a).setMin(u,v),
            "max_pair": cls(a).setMax(u,v), "min_point": cls(a).setMin(b),
            "max_point": cls(a).setMax(b), "references": int(all(reference_results)),
            "pixel": Position(a), "walk": WalkPosition(a), "tile": TilePosition(a),
            "float_cast": cls(5.7,-8.2), "stream": str(a),
        }
        for name,value in actual.items():
            if hasattr(value, "x"):
                value = [value.x,value.y]
            if name in ("distance","length"):
                assert value == pytest.approx(expected[name], rel=1e-14), (case,name,value,expected[name])
            else:
                assert value == expected[name], (case,name,value,expected[name])
