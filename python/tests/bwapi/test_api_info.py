import ast
import re
import xml.etree.ElementTree as ET
from pathlib import Path

from bwrapi.bwapi import BWAPI_getRevision, BWAPI_isDebug, Tournament
from bwrapi.bwapi import api_info
from bwrapi.bwapi.profile import BWAPI_IS_DEBUG, BWAPI_REVISION
from upstream_root import SOURCE, requires_upstream


_REPO = Path(__file__).resolve().parents[3]
_UPSTREAM = SOURCE


def _pinned_text(relative_path: str) -> str:
    raw = (_UPSTREAM / relative_path).read_bytes()
    return raw.decode("utf-8")


def _enum_items(source: str, enum_name: str):
    source = re.sub(r"/\*.*?\*/|//[^\n]*", "", source, flags=re.S)
    match = re.search(r"\benum\s+" + re.escape(enum_name) + r"\s*\{", source)
    assert match is not None
    body = source[match.end() : source.index("}", match.end())]
    items = []
    next_value = 0
    for raw_item in body.split(","):
        item = raw_item.strip()
        if not item:
            continue
        parsed = re.fullmatch(
            r"([A-Za-z_]\w*)\s*(?:=\s*(0[xX][0-9A-Fa-f]+|[0-9]+))?", item
        )
        assert parsed is not None, f"unparsed pinned enum item: {item!r}"
        name, explicit = parsed.groups()
        value = int(explicit, 0) if explicit is not None else next_value
        items.append((name, value))
        next_value = value + 1
    return items


def _stub_assignments(class_node: ast.ClassDef) -> list[tuple[str, int]]:
    result = []
    for statement in class_node.body:
        if isinstance(statement, ast.Assign) and len(statement.targets) == 1:
            target = statement.targets[0]
            if isinstance(target, ast.Name) and isinstance(statement.value, ast.Constant):
                result.append((target.id, statement.value.value))
    return result


@requires_upstream
def test_revision_matches_pinned_header_cpp_host_and_python_game_consumer():
    revision_header = _pinned_text("include/svnrev.h")
    revision_match = re.search(r"\bSVN_REV\s*=\s*(\d+)\s*;", revision_header)
    assert revision_match is not None
    source_revision = int(revision_match.group(1))
    assert source_revision == 5030

    api_header = _pinned_text("include/BWAPI.h")
    assert re.search(r"\bint\s+BWAPI_getRevision\s*\(\s*\)\s*;", api_header)
    assert re.search(r"\bbool\s+BWAPI_isDebug\s*\(\s*\)\s*;", api_header)
    api_source = _pinned_text("BWAPILIB/Source/BWAPI.cpp")
    assert re.search(
        r"int\s+BWAPI::BWAPI_getRevision\s*\(\s*\)\s*\{\s*return\s+SVN_REV\s*;\s*\}",
        api_source,
    )
    assert re.search(
        r"bool\s+BWAPI::BWAPI_isDebug\s*\(\s*\)\s*\{\s*return\s+BUILD_DEBUG\s*==\s*1\s*;\s*\}",
        api_source,
    )

    game_source = _pinned_text("BWAPI/Source/BWAPI/GameImpl.cpp")
    assert re.search(
        r"int\s+GameImpl::getRevision\s*\(\s*\)\s+const\s*\{\s*return\s+SVN_REV\s*;\s*\}",
        game_source,
    )
    host_header = (_REPO / "host" / "runtime" / "legacy44-host" / "legacy44_common.h").read_text()
    assert re.search(r"kBwapiRevision\s*=\s*5030\s*;", host_header)

    assert BWAPI_REVISION == source_revision
    assert BWAPI_getRevision() == source_revision


@requires_upstream
def test_debug_query_reflects_python_release_profile_and_official_macro_contract():
    version_header = _pinned_text("include/starcraftver.h")
    assert re.search(
        r"#ifdef\s+_DEBUG\s+#define\s+BUILD_STR\s+\"DEBUG\"\s+#define\s+BUILD_DEBUG\s+1"
        r"\s+#else\s+#define\s+BUILD_STR\s+\"RELEASE\"\s+#define\s+BUILD_DEBUG\s+0\s+#endif",
        version_header,
    )
    project = ET.fromstring(_pinned_text("BWAPILIB/BWAPILIB.vcxproj"))
    release_groups = [
        node
        for node in project.iter()
        if node.tag.rsplit("}", 1)[-1] == "ItemDefinitionGroup"
        and node.get("Condition") == "'$(Configuration)|$(Platform)'=='Release|Win32'"
    ]
    assert len(release_groups) == 1
    compile_node = next(
        node
        for node in release_groups[0]
        if node.tag.rsplit("}", 1)[-1] == "ClCompile"
    )
    definitions = next(
        child.text or ""
        for child in compile_node
        if child.tag.rsplit("}", 1)[-1] == "PreprocessorDefinitions"
    ).split(";")
    assert "NDEBUG" in definitions
    assert "_DEBUG" not in definitions

    assert BWAPI_IS_DEBUG is False
    assert BWAPI_isDebug() is False


@requires_upstream
def test_tournament_action_ids_match_pinned_header_names_and_values():
    source_items = _enum_items(
        _pinned_text("include/BWAPI/TournamentAction.h"), "ActionID"
    )
    assert len(source_items) == 13
    assert list(Tournament.ActionID.__members__) == [name for name, _ in source_items]
    assert [int(Tournament.ActionID.__members__[name]) for name, _ in source_items] == [
        value for _, value in source_items
    ]
    assert [int(getattr(Tournament, name)) for name, _ in source_items] == [
        value for _, value in source_items
    ]
    assert all(
        getattr(Tournament, name) is Tournament.ActionID[name]
        for name, _ in source_items
    )


@requires_upstream
def test_api_info_is_public_and_stub_matches_functions_and_action_enum():
    import bwrapi.bwapi as public_api

    assert public_api.BWAPI_getRevision is api_info.BWAPI_getRevision
    assert public_api.BWAPI_isDebug is api_info.BWAPI_isDebug
    assert public_api.Tournament is api_info.Tournament
    assert {"BWAPI_getRevision", "BWAPI_isDebug", "Tournament"} <= set(public_api.__all__)
    assert api_info.__all__ == ["BWAPI_getRevision", "BWAPI_isDebug", "Tournament"]
