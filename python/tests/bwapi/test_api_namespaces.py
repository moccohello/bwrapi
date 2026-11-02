import re

import pytest

from bwrapi.bwapi import api_namespaces as api
from bwrapi.bwapi.profile import BWAPI_CLIENT_VERSION
from bwrapi.bwapi.type_ids import CoordinateTypeId, EventTypeId, FlagId, LatencyId
from upstream_root import INCLUDE, requires_upstream


_INCLUDE = INCLUDE


def _header(relative_path: str) -> str:
    source_bytes = (_INCLUDE / relative_path).read_bytes()
    return re.sub(r"/\*.*?\*/|//[^\n]*", "", source_bytes.decode("utf-8"), flags=re.S)


def _enum_items(relative_path: str, namespace_name: str):
    source = _header(relative_path)
    declaration = re.search(
        r"\bnamespace\s+" + re.escape(namespace_name) + r"\s*\{.*?\benum\s+Enum\s*\{",
        source,
        flags=re.S,
    )
    assert declaration is not None
    body = source[declaration.end() : source.index("}", declaration.end())]

    items = []
    next_value = 0
    for raw_item in body.split(","):
        item = raw_item.strip()
        if not item:
            continue
        match = re.fullmatch(r"([A-Za-z_]\w*)\s*(?:=\s*(\d+))?", item)
        assert match is not None, f"unparsed enum member in {relative_path}: {item!r}"
        name, explicit_value = match.groups()
        value = int(explicit_value) if explicit_value is not None else next_value
        items.append((name, value))
        next_value = value + 1
    return items


def _position_items(namespace_name: str, value_type: str, scales: dict[str, int]):
    source = _header("BWAPI/Position.h")
    namespace = re.search(
        r"\bnamespace\s+" + re.escape(namespace_name) + r"\s*\{(.*?)\n\s*\}",
        source,
        flags=re.S,
    )
    assert namespace is not None
    declaration = re.compile(
        r"\bconst\s+" + re.escape(value_type) + r"\s+(\w+)\s*\{([^,]+),([^}]+)\}\s*;"
    )

    def parse_expression(raw: str) -> int:
        expression = raw.strip()
        literal = re.fullmatch(r"(\d+)", expression)
        if literal:
            return int(literal.group(1))
        divided = re.fullmatch(r"(\d+)\s*/\s*([A-Z_]+)", expression)
        assert divided is not None, f"unparsed position expression: {expression!r}"
        numerator, scale_name = divided.groups()
        return int(numerator) // scales[scale_name]

    return [
        (match.group(1), parse_expression(match.group(2)), parse_expression(match.group(3)))
        for match in declaration.finditer(namespace.group(1))
    ]


@requires_upstream
def test_nested_enum_and_unscoped_namespace_members_match_pinned_headers():
    source_specs = (
        ("CoordinateType", "BWAPI/CoordinateType.h", "CoordinateType", CoordinateTypeId),
        ("EventType", "BWAPI/EventType.h", "EventType", EventTypeId),
        ("Flag", "BWAPI/Flag.h", "Flag", FlagId),
        ("Latency", "BWAPI/Latency.h", "Latency", LatencyId),
    )
    expected_total = 0
    for namespace_name, header_path, source_namespace, legacy_enum in source_specs:
        items = _enum_items(header_path, source_namespace)
        expected_total += len(items)
        namespace = getattr(api, namespace_name)
        enum_type = namespace.Enum

        assert list(enum_type.__members__) == [name for name, _ in items]
        assert [int(enum_type[name]) for name, _ in items] == [value for _, value in items]
        assert [int(member) for member in legacy_enum] == [value for _, value in items]
        assert all(getattr(namespace, name) is enum_type[name] for name, _ in items)

        none_item = next((item for item in items if item[0] == "None"), None)
        if none_item is not None:
            member = enum_type["None"]
            assert getattr(enum_type, "None") is member
            assert enum_type.None_ is member
            assert getattr(namespace, "None") is member
            assert namespace.None_ is member
            assert "None_" not in enum_type.__members__

    assert expected_total == 33


@requires_upstream
def test_position_namespaces_match_pinned_position_header_and_sdk_types():
    source = _header("BWAPI/Position.h")
    scales = {
        name: int(value)
        for name, value in re.findall(
            r"\bconst\s+int\s+(POSITION_SCALE|WALKPOSITION_SCALE|TILEPOSITION_SCALE)\s*=\s*(\d+)\s*;",
            source,
        )
    }
    assert scales == {
        "POSITION_SCALE": 1,
        "WALKPOSITION_SCALE": 8,
        "TILEPOSITION_SCALE": 32,
    }

    specs = (
        ("Positions", "Position", api.Position),
        ("WalkPositions", "WalkPosition", api.WalkPosition),
        ("TilePositions", "TilePosition", api.TilePosition),
    )
    for namespace_name, value_type, value_class in specs:
        items = _position_items(namespace_name, value_type, scales)
        assert [name for name, _, _ in items] == ["Invalid", "None", "Unknown", "Origin"]
        namespace = getattr(api, namespace_name)
        for name, x, y in items:
            point = getattr(namespace, name)
            assert type(point) is value_class
            assert (point.x, point.y) == (x, y)
        none_point = getattr(namespace, "None")
        assert namespace.None_ is none_point

    for point_type in (api.Position, api.TilePosition, api.WalkPosition):
        point = point_type(1, 2)
        point.x = 3
        assert (point.x, point.y) == (3, 2)
        assert callable(point.isValid)

    with pytest.raises(AttributeError, match="const"):
        api.WalkPositions.Origin.x = 1


@requires_upstream
def test_tile_and_client_constants_match_pinned_headers_and_profile():
    constants_header = _header("BWAPI/Constants.h")
    tile_size_match = re.search(r"#\s*define\s+TILE_SIZE\s+(\d+)", constants_header)
    assert tile_size_match is not None
    assert api.TILE_SIZE == int(tile_size_match.group(1)) == 32

    client_header = _header("BWAPI.h")
    client_version_match = re.search(r"\bconst\s+int\s+CLIENT_VERSION\s*=\s*(\d+)\s*;", client_header)
    assert client_version_match is not None
    assert api.CLIENT_VERSION == int(client_version_match.group(1)) == BWAPI_CLIENT_VERSION == 10003
