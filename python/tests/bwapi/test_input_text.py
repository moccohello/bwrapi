import re

import pytest

from bwrapi.bwapi import input_text
from bwrapi.bwapi.types import Colors
from upstream_root import INCLUDE, requires_upstream


_HEADERS = INCLUDE / "BWAPI"


def _cpp_enum_items(header_name: str, enum_name: str, occurrence: int = 0):
    source_path = _HEADERS / header_name
    source_bytes = source_path.read_bytes()
    source = source_bytes.decode("utf-8")
    source = re.sub(r"/\*.*?\*/|//[^\n]*", "", source, flags=re.S)
    declarations = list(
        re.finditer(r"\benum\s+" + re.escape(enum_name) + r"\s*\{", source)
    )
    declaration = declarations[occurrence]
    body = source[declaration.end() : source.index("}", declaration.end())]

    result = []
    next_value = 0
    for raw_item in body.split(","):
        item = raw_item.strip()
        if not item:
            continue
        match = re.fullmatch(
            r"([A-Za-z_]\w*)\s*(?:=\s*(0[xX][0-9A-Fa-f]+|[0-9]+))?", item
        )
        assert match is not None, f"unparsed pinned enum item: {item!r}"
        name, explicit_value = match.groups()
        value = int(explicit_value, 0) if explicit_value is not None else next_value
        result.append((name, value))
        next_value = value + 1
    return result


def _assert_enum_matches(enum_type, source_items):
    assert list(enum_type.__members__) == [name for name, _ in source_items]
    assert [int(enum_type.__members__[name]) for name, _ in source_items] == [
        value for _, value in source_items
    ]


@requires_upstream
def test_input_enums_and_global_constants_match_pinned_header():
    mouse_items = _cpp_enum_items("Input.h", "MouseButton")
    key_items = _cpp_enum_items("Input.h", "Key")

    assert len(mouse_items) == 4
    assert len(key_items) == 229
    _assert_enum_matches(input_text.MouseButton, mouse_items)
    _assert_enum_matches(input_text.Key, key_items)

    expected_globals = {
        name: value
        for name, value in [*key_items, *mouse_items]
        if name.startswith(("K_", "M_"))
    }
    assert {name for name in input_text.__all__ if name.startswith(("K_", "M_"))} == set(
        expected_globals
    )
    for name, value in expected_globals.items():
        member_type = input_text.Key if name.startswith("K_") else input_text.MouseButton
        member = member_type.__members__[name]
        assert int(getattr(input_text, name)) == value
        assert getattr(input_text, name) is member

    # Keep upstream's reserved and undefined spellings as declared, without inventing aliases.
    assert input_text.Key.__members__["__UNDEFINED_7"] == 7
    assert input_text.Key.__members__["__RESERVED_A"] == 10
    assert input_text.Key.__members__["K_UNDEFINED_16"] == 22


@requires_upstream
def test_text_enums_match_pinned_header_and_keep_palette_colors_separate():
    text_items = _cpp_enum_items("Color.h", "Enum", 0)
    size_items = _cpp_enum_items("Color.h", "Enum", 1)

    assert len(text_items) == 27
    assert len(size_items) == 4
    _assert_enum_matches(input_text.Text.Enum, text_items)
    _assert_enum_matches(input_text.Text.Size.Enum, size_items)
    assert issubclass(input_text.Text.Enum, int)
    assert issubclass(input_text.Text.Size.Enum, int)

    assert [int(member) for member in input_text.Text.Size.Enum] == [0, 1, 2, 3]
    assert int(input_text.Text.Enum.Red) == 6
    assert int(input_text.Text.Enum.Yellow) == 3
    assert int(Colors.Red) == 111
    assert int(Colors.Yellow) == 135
    assert input_text.Text.Enum.Red is not Colors.Red


@requires_upstream
def test_text_namespace_aliases_match_pinned_header_enum_members():
    text_items = _cpp_enum_items("Color.h", "Enum", 0)
    size_items = _cpp_enum_items("Color.h", "Enum", 1)

    assert [getattr(input_text.Text, name) for name, _ in text_items] == [
        input_text.Text.Enum[name] for name, _ in text_items
    ]
    assert all(
        getattr(input_text.Text, name) is input_text.Text.Enum[name]
        for name, _ in text_items
    )
    assert [getattr(input_text.Text.Size, name) for name, _ in size_items] == [
        input_text.Text.Size.Enum[name] for name, _ in size_items
    ]
    assert all(
        getattr(input_text.Text.Size, name) is input_text.Text.Size.Enum[name]
        for name, _ in size_items
    )



def test_text_is_color_matches_bwapi_44_codes_for_all_values_zero_through_31():
    expected = set(range(2, 9)) | set(range(14, 18)) | set(range(21, 32))
    assert {code for code in range(32) if input_text.Text.isColor(code)} == expected
    assert input_text.Text.isColor(input_text.Text.Enum.Red)
    assert not input_text.Text.isColor(input_text.Text.Enum.Previous)
    assert not input_text.Text.isColor(input_text.Text.Enum.Invisible)
    assert not input_text.Text.isColor(input_text.Text.Enum.Align_Right)
    assert not input_text.Text.isColor(input_text.Text.Enum.Invisible2)


def test_text_is_color_uses_integer_index_and_rejects_non_integral_inputs():
    class IndexValue:
        def __index__(self):
            return 25

    assert input_text.Text.isColor(IndexValue())
    assert not input_text.Text.isColor(True)
    assert not input_text.Text.isColor(-1)
    assert not input_text.Text.isColor(32)
    assert not input_text.Text.isColor(255)
    with pytest.raises(TypeError):
        input_text.Text.isColor(2.0)
    with pytest.raises(TypeError):
        input_text.Text.isColor("2")
