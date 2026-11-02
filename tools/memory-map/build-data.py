"""기존 장부와 DAT 조사 원문에서 읽기 전용 메모리 지도 데이터를 만든다."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
from datetime import datetime, timezone
from pathlib import Path


HERE = Path(__file__).resolve().parent
DEFAULT_LEDGER = HERE.parents[1] / "docs" / "memory" / "memory-addresses.json"


def local_path(value: str) -> Path:
    if os.name != "nt" and re.match(r"^[A-Za-z]:[/\\]", value):
        value = "/mnt/" + value[0].lower() + "/" + value[3:].replace("\\", "/")
    path = Path(value)
    return path if path.is_absolute() else (HERE / path).resolve()


SOURCE_CONFIG = json.loads((HERE / "source-paths.json").read_text(encoding="utf-8"))
DISCOVERY_ROOT = local_path(SOURCE_CONFIG["discoveryRoot"])
DISCOVERY = DISCOVERY_ROOT / "location-discovery"
FAMILIES = {
    "units": ("유닛 DAT", "functions/unit-dat-descriptors-0xda9820.txt", 54, None),
    "weapons": ("무기 DAT", "functions/weapon-dat-descriptors-0xdaa0a0.txt", 24, 130),
    "flingy": ("flingy DAT", "structures/flingy-dat-descriptors.txt", 7, 209),
    "sprites": ("sprites DAT", "structures/sprites-dat-descriptors.txt", 6, None),
    "images": ("images DAT", "structures/images-dat-descriptors.txt", 14, 999),
    "orders": ("orders DAT", "structures/orders-dat-descriptors.txt", 19, 189),
    "tech": ("기술 DAT", "functions/tech-dat-descriptors-0xda9700.txt", 11, 44),
    "upgrades": ("업그레이드 DAT", "functions/upgrade-dat-descriptors-0xda9f00.txt", 12, 61),
    "portdata": ("portdata DAT", "structures/port-map-dat-descriptors.txt", 6, 110),
    "mapdata": ("mapdata DAT", "structures/port-map-dat-descriptors.txt", 1, 65),
}


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def hex_offset(value: object) -> str | None:
    if isinstance(value, str) and re.fullmatch(r"0[xX][0-9a-fA-F]+", value):
        return f"0x{int(value, 16):X}"
    return None


def size_of(data_type: str) -> int | None:
    pointer = re.match(r"^pointer(32|64)(?:\[(\d+)\])?(?=$|\s|->|;)", data_type.strip())
    if pointer:
        return (int(pointer.group(1)) // 8) * int(pointer.group(2) or 1)
    match = re.fullmatch(r"(u|i)(8|16|32|64)(?:\[(\d+)\])?", data_type.strip())
    if not match:
        return None
    width = int(match.group(2)) // 8
    return width * int(match.group(3) or 1)


def clean_text(value: object) -> str:
    if value is None:
        return ""
    if isinstance(value, str):
        return value.strip()
    return json.dumps(value, ensure_ascii=False, separators=(",", ":"))


def evidence_state(items: list[dict]) -> str:
    text = " ".join(item["text"] for item in items)
    if re.search(r"(?:PID\s*\d+|실게임|실측).*?(?:frame\s*\d+|프레임\s*\d+|관측)", text, re.I) and not re.search(
        r"(?:실게임|실제\s*로드값|함수\s*실행).*?미관측", text
    ):
        return "live_recorded"
    if re.search(r"저장\s*(?:PE|이미지)|정적|RVA|descriptor|역어셈블", text, re.I):
        return "static_recorded"
    if re.search(r"reader|profile|source|소스|코드|OFFSET_ASSERT", text, re.I):
        return "source_recorded"
    return "not_recorded"


def basis_for(section: str) -> str:
    return {
        "module_relative": "module",
        "player_identity_entry": "player_identity_entry",
        "bwgame": "bwgame",
        "cunit": "cunit_record",
        "corder": "corder",
        "csprite": "csprite",
        "cimage_attack_inputs": "cimage",
        "bullet": "bullet",
        "active_tile": "active_tile",
        "sai_pathing": "sai_pathing",
        "command_profile": "module",
        "diagnostic_relations": "module",
    }.get(section, section)


LABELS = {
    "hit_points_raw": "체력 원시값", "minerals": "미네랄", "gas": "가스",
    "cumulative_gas": "누적 가스", "cumulative_minerals": "누적 미네랄",
    "map_width": "지도 너비", "map_height": "지도 높이",
    "frame_count": "프레임 수", "owner_id": "소유 플레이어 ID",
    "sprite_ptr": "Sprite 포인터", "primary_image_ptr": "주 Image 포인터",
    "position_x": "위치 X", "position_y": "위치 Y",
    "target_x": "목표 X", "target_y": "목표 Y",
    "source_player": "발사 플레이어", "weapon_id": "무기 ID",
    "order_id": "명령 ID", "type_id": "유형 ID",
    "larva_timer_candidate": "라바 생성 타이머 후보",
    "order_queue_timer": "명령 큐 타이머",
    "resource_cargo_kind": "자원 운반 종류",
}

OFFSET_NAMES = {
    "replay_scenario_chk": ["byte_size", "buffer_pointer"],
    "map_pixel_dimensions": ["width", "height"],
    "screen_pixel_coordinates": ["x", "y"],
    "drag_selection_state": ["is_selecting", "select_start_x", "select_start_y"],
    "step_ai_regions_indices": ["player_index", "region_index"],
    "net_player_id_maps": ["net_player_to_unique", "net_player_to_game"],
    "targeted_order_state": ["is_targeting", "unit_order", "ground_order", "fow_order"],
    "fow_sprite_record_payload": ["unit_type_id", "cloned_sprite_pointer"],
}


def semantic_state(meaning: str) -> str:
    if not meaning or "미명명" in meaning or "미분류" in meaning:
        return "unclassified"
    if any(term in meaning for term in ("의미 미확정", "단위 미확정", "공식 필드명 미확정", "의미 불명")):
        return "partial"
    return "documented"


def main() -> None:
    parser = argparse.ArgumentParser(description="기존 장부에서 상세 메모리 지도 JSON 재생성")
    parser.add_argument("ledger", nargs="?", type=Path, default=DEFAULT_LEDGER)
    parser.add_argument("--output", type=Path, default=HERE / "memory-map.json")
    args = parser.parse_args()
    ledger_path = args.ledger.expanduser().resolve()
    ledger = json.loads(ledger_path.read_text(encoding="utf-8"))

    sources: list[dict] = []
    def register(path: Path, sid: str) -> None:
        if path.exists() and all(source["id"] != sid for source in sources):
            sources.append({"id": sid, "path": str(path), "sha256": sha256(path)})
    register(ledger_path, "ledger")
    register(HERE / "display-names.json", "display_names")
    register(HERE / "source-paths.json", "source_paths")
    display_names = json.loads((HERE / "display-names.json").read_text(encoding="utf-8"))
    for section in ("functions", "structures"):
        register(DISCOVERY / section / "result.json", f"{section}_result")
        register(DISCOVERY / section / "unknown-source-audit.md", f"{section}_audit")

    groups = [
        {"id": "unit", "label": "유닛", "parentId": None},
        {"id": "player", "label": "플레이어", "parentId": None},
        {"id": "bwgame", "label": "BWGame", "parentId": None},
        {"id": "map", "label": "지도", "parentId": None},
        {"id": "dat", "label": "DAT", "parentId": None},
        {"id": "functions", "label": "함수·명령", "parentId": None},
        {"id": "other", "label": "기타", "parentId": None},
    ]
    for family, (label, _, _, _) in FAMILIES.items():
        groups.append({"id": f"dat_{family}", "label": label, "parentId": "dat"})
    nodes: list[dict] = []
    edges: list[dict] = []
    by_id: dict[str, dict] = {}
    by_module_offset: dict[str, str] = {}

    def add_node(
        *, nid: str, key: str, label: str, group: str, kind: str,
        raw: object, source_ref: str, basis: str | None = None,
        offset: str | None = None, data_type: str = "", meaning: str = "",
        conditions: str = "", limitations: list[str] | None = None,
        evidence: list[dict] | None = None, legacy_status: str = "",
        location_state: str | None = None, meaning_state: str | None = None,
        size_bytes: int | None = None, aliases: list[dict] | None = None,
    ) -> dict:
        if nid in by_id:
            return by_id[nid]
        entry_evidence = evidence or []
        address = {"basis": basis, "offset": offset} if basis and offset else None
        if address is not None and aliases:
            address["aliases"] = aliases
        if location_state is None:
            location_state = "recorded" if address else "not_recorded"
        if meaning_state is None:
            meaning_state = semantic_state(meaning)
        node = {
            "id": nid, "label": label, "key": key, "groupId": group, "kind": kind,
            "address": address, "sizeBytes": size_bytes if size_bytes is not None else size_of(data_type),
            "dataType": data_type, "meaning": meaning, "conditions": conditions,
            "locationState": location_state, "meaningState": meaning_state,
            "evidenceState": evidence_state(entry_evidence), "legacyStatus": legacy_status,
            "limitations": limitations or [], "evidence": entry_evidence,
            "sourceRef": source_ref, "raw": raw if isinstance(raw, dict) else {"value": raw},
        }
        nodes.append(node)
        by_id[nid] = node
        if basis == "module" and offset and kind not in ("descriptor", "function"):
            by_module_offset.setdefault(offset, nid)
        return node

    def ledger_node(path: str, key: str, raw: object, group: str, basis: str,
                    kind: str = "field") -> None:
        obj = raw if isinstance(raw, dict) else {"value": raw}
        address_raw = obj.get("address")
        offset = next((hex_offset(obj.get(k)) for k in
                       ("offset", "pointer_module_offset", "module_offset", "function_rva", "getter_rva")
                       if hex_offset(obj.get(k))), None)
        if offset is None and isinstance(address_raw, dict):
            offset = hex_offset(address_raw.get("offset"))
            basis = clean_text(address_raw.get("basis")) or basis
        if offset is None:
            offset = hex_offset(address_raw)
        if offset is None and kind == "function":
            offset = hex_offset(obj.get("allocate_rva")) or hex_offset(obj.get("upgrade_cost_rva"))
        if offset is None and isinstance(raw, str) and (
            path.startswith("command_profile.rvas.")
            or path.startswith("bullet.root.fields.")
            or path.startswith("map_pathing.active_tile.byte_layout.")
            or path.startswith("map_pathing.sai_pathing.")
        ):
            offset = hex_offset(raw)
        is_xor_constant = key.endswith("_xor_key")
        if is_xor_constant:
            offset = None
        if "pointer_module_offset" in obj or "module_offset" in obj:
            basis = "module"
        if path == "module_relative.fow_sprite_record_payload":
            basis = "fow_sprite_record"
        if path.startswith("cunit.fields") and offset:
            # 장부의 CUnit 필드는 record base 기준이며 native Unit은 record+0x10이다.
            native = int(offset, 16) - 0x10
            aliases = [{"basis": "native_unit", "offset": f"0x{native:X}"}] if native >= 0 else None
        else:
            aliases = None
        evidence = ([{"kind": "원문", "text": clean_text(obj["evidence"]),
                      "ref": f"ledger#{path}"}] if obj.get("evidence") else [])
        limitations = [clean_text(obj["limits"])] if obj.get("limits") else []
        meaning = clean_text(obj.get("meaning") or obj.get("output") or "")
        conditions = clean_text(obj.get("input") or obj.get("condition") or obj.get("index") or "")
        data_type = clean_text(obj.get("type") or obj.get("value_type") or "")
        derived = clean_text(obj.get("derived"))
        indirect = bool(offset and "*(module + offset)" in derived)
        if indirect:
            kind = "pointer"
            data_type = "pointer64"
            meaning = meaning or "간접 접근으로 읽는 값: " + derived
        if path.startswith("diagnostic_relations"):
            kind = "function"
        if kind == "bitmask":
            offset = None
        dynamic = address_raw == "runtime_discovered"
        has_offsets = isinstance(obj.get("offsets"), list)
        is_constant = is_xor_constant or (bool(derived) and offset is None) or key in (
            "stride", "record_stride", "record_bytes", "read_window", "read_bytes",
            "record_capacity", "pool_capacity", "public_queue_limit", "linked_node_limit",
            "selection_pointer_adjustment", "module_size",
        )
        diagnostic_rvas = ([(name, value) for name, value in obj.items()
                            if path.startswith("diagnostic_relations.") and name.endswith("_rva")
                            and isinstance(value, (str, list))])
        if has_offsets or is_constant:
            kind = "constant" if is_constant else "collection"
        if path in ("bullet.identity", "map_pathing.active_tile.byte_layout",
                    "map_pathing.active_tile.semantics") or path.startswith(
                        "csprite.cimage_attack_inputs.") and key in ("source", "live_scope", "evidence"):
            kind = "metadata"
        state = ("dynamic" if dynamic else "unknown" if kind == "question" else "not_applicable" if kind in
                 ("bitmask", "metadata", "constant", "collection") or
                 (path.startswith("diagnostic_relations.") and offset is None) else None)
        node = add_node(nid=f"ledger:{path}", key=key, label=LABELS.get(key, key.replace("_", " ")),
                 group=group, kind=kind, raw=obj, source_ref=f"ledger#{path}",
                 basis=basis, offset=offset, data_type=data_type, meaning=meaning,
                 conditions=conditions, limitations=limitations, evidence=evidence,
                 legacy_status=clean_text(obj.get("status")),
                 location_state=state,
                 aliases=aliases)
        if is_xor_constant:
            node["meaning"] = "채팅 context 포인터 복호화용 XOR 상수. 메모리 주소가 아니다."
            node["meaningState"] = "documented"
        if indirect and node["address"]:
            node["address"]["note"] = (
                "표시된 RVA와 8바이트 폭은 포인터 저장 슬롯이다. 실제 값 접근식: "
                + derived + ". 역참조된 객체의 +8에 있는 u32와 슬롯 자체를 구분한다."
            )
        if has_offsets:
            names = OFFSET_NAMES.get(key, [])
            types = obj.get("type") if isinstance(obj.get("type"), list) else []
            for index, source_offset in enumerate(obj["offsets"]):
                child_offset = hex_offset(source_offset)
                if child_offset is None and isinstance(source_offset, str) and source_offset.startswith("+"):
                    child_offset = hex_offset(source_offset[1:])
                child_key = names[index] if index < len(names) else f"entry_{index + 1}"
                child_type = clean_text(types[index]) if index < len(types) else (
                    data_type.removesuffix(" each") if data_type.endswith(" each") else data_type
                )
                scalar_type = re.match(r"^(?:[ui](?:8|16|32|64)|pointer(?:32|64))\b", child_type)
                if scalar_type:
                    child_type = scalar_type.group()
                child_path = f"{path}.offsets.{child_key}"
                add_node(nid=f"ledger:{child_path}", key=child_key,
                         label=LABELS.get(child_key, child_key.replace("_", " ")),
                         group=group, kind="field", raw={"offset": source_offset, "parent": path,
                                                            "type": child_type},
                         source_ref=f"ledger#{path}.offsets[{index}]", basis=basis,
                         offset=child_offset, data_type=child_type, meaning=meaning,
                         conditions=conditions, limitations=limitations, evidence=evidence,
                         legacy_status=clean_text(obj.get("status")),
                         location_state=None if child_offset else "not_recorded")
        for field_name, value in diagnostic_rvas:
            values = value if isinstance(value, list) else re.findall(r"0x[0-9A-Fa-f]+", value)
            for index, candidate in enumerate(values):
                child_offset = hex_offset(candidate)
                if not child_offset:
                    continue
                child_key = field_name if len(values) == 1 else f"{field_name}_{index + 1}"
                child_path = f"{path}.{child_key}"
                add_node(nid=f"ledger:{child_path}", key=child_key,
                         label=child_key.replace("_", " "), group=group, kind="function",
                         raw={"source_key": field_name, "rva": candidate, "parent": path},
                         source_ref=f"ledger#{path}.{field_name}", basis="module",
                         offset=child_offset, meaning=f"{key}의 {field_name} 함수 주소",
                         evidence=evidence, limitations=limitations,
                         legacy_status=clean_text(obj.get("status")))

    for key, raw in ledger.get("module_relative", {}).items():
        low = key.lower()
        dat_prefixes = ("unit_", "weapon_", "flingy_", "sprites_", "images_",
                        "orders_", "tech_", "upgrade_", "portdata_", "mapdata_")
        group = ("dat" if "_dat_" in low or
                 (low.startswith(dat_prefixes) and low.endswith("_table")) else
                 "map" if any(x in low for x in ("tile", "map", "terrain", "creep", "region")) else
                 "player" if any(x in low for x in ("player", "force", "supply", "upgrade", "tech")) else
                 "functions" if any(x in low for x in ("command_", "ui_selection")) else
                 "unit" if any(x in low for x in ("unit", "sprite", "bullet")) else "other")
        ledger_node(f"module_relative.{key}", key, raw, group, "module", "global")
    for section, group in (("player_identity_entry", "player"), ("bwgame", "bwgame"),
                           ("cunit", "unit"), ("corder", "unit"),
                           ("csprite", "unit"), ("bullet", "unit")):
        obj = ledger.get(section, {})
        if not isinstance(obj, dict):
            continue
        for key, value in obj.items():
            if key in ("fields", "status_bits", "movement_bits", "cimage_attack_inputs") and isinstance(value, dict):
                sub_basis = "cimage" if key == "cimage_attack_inputs" else basis_for(section)
                for field, field_raw in value.items():
                    ledger_node(f"{section}.{key}.{field}", field, field_raw, group,
                                sub_basis, "bitmask" if key in ("status_bits", "movement_bits") else "field")
            elif key in ("root", "region", "identity") and isinstance(value, dict):
                ledger_node(f"{section}.{key}", key, value, group, basis_for(section), "structure")
                if section == "bullet" and key == "root":
                    for field, field_raw in value.get("fields", {}).items():
                        ledger_node(f"bullet.root.fields.{field}", field, field_raw,
                                    group, "bullet_root")
            elif key == "unresolved" and isinstance(value, list):
                for index, item in enumerate(value):
                    ledger_node(f"{section}.unresolved.{index}", f"unresolved_{index}", item,
                                group, basis_for(section), "question")
            elif key not in ("fields", "status_bits", "movement_bits", "cimage_attack_inputs"):
                ledger_node(f"{section}.{key}", key, value, group, basis_for(section), "metadata")
    for branch in ("active_tile", "sai_pathing"):
        obj = ledger.get("map_pathing", {}).get(branch, {})
        if isinstance(obj, dict):
            ledger_node(f"map_pathing.{branch}", branch, obj, "map", basis_for(branch), "structure")
            for key, value in obj.items():
                if isinstance(value, dict):
                    ledger_node(f"map_pathing.{branch}.{key}", key, value, "map", basis_for(branch))
                elif key in ("tile_grid", "split_table", "region_table"):
                    ledger_node(f"map_pathing.{branch}.{key}", key, value, "map", basis_for(branch))
            if branch == "active_tile":
                for byte_key, detail in obj.get("byte_layout", {}).items():
                    byte_offset = f"0x{int(byte_key):X}"
                    add_node(nid=f"ledger:map_pathing.active_tile.byte_{byte_key}",
                             key=f"byte_{byte_key}", label=f"ActiveTile byte {byte_key}",
                             group="map", kind="field", raw={"value": detail},
                             source_ref=f"ledger#map_pathing.active_tile.byte_layout.{byte_key}",
                             basis="active_tile", offset=byte_offset, data_type="u8",
                             meaning=clean_text(detail), legacy_status=clean_text(obj.get("status")))
    command = ledger.get("command_profile", {})
    for key, value in command.items():
        if isinstance(value, dict):
            for child, raw in value.items():
                ledger_node(f"command_profile.{key}.{child}", child, raw,
                            "functions", "module", "function" if key == "rvas" else "metadata")
        else:
            ledger_node(f"command_profile.{key}", key, value, "functions", "module", "metadata")
    for key, raw in ledger.get("diagnostic_relations", {}).items():
        ledger_node(f"diagnostic_relations.{key}", key, raw, "functions", "module", "function")

    def add_edge(source: str, target: str, etype: str, label: str,
                 evidence: str, inferred: bool = False) -> None:
        if source not in by_id or target not in by_id:
            return
        eid = f"{etype}:{source}:{target}"
        if any(edge["id"] == eid for edge in edges):
            return
        edges.append({"id": eid, "source": source, "target": target,
                      "type": etype, "label": label, "evidence": evidence,
                      "inferred": inferred})

    # 구조체와 원문 fields의 포함 관계를 따로 기록한다. 구조체 자체에 고정 RVA를 부여하지 않는다.
    structure_groups = {
        "player_identity_entry": "player", "bwgame": "bwgame", "cunit": "unit",
        "corder": "unit", "csprite": "unit", "cimage": "unit", "bullet": "unit",
        "active_tile": "map", "sai_pathing": "map",
    }
    for section, group in structure_groups.items():
        root_id = f"structure:{section}"
        add_node(nid=root_id, key=section, label=section.upper() if section == "cunit" else section,
                 group=group, kind="structure", raw={"source_section": section},
                 source_ref=f"ledger#{section}", meaning=f"{section} 구조 필드 묶음",
                 location_state="not_applicable", meaning_state="documented")
        prefixes = ([f"ledger:{section}.fields."] if section != "cimage" else
                    ["ledger:csprite.cimage_attack_inputs."])
        if section == "active_tile":
            prefixes = ["ledger:map_pathing.active_tile.byte_", "ledger:map_pathing.active_tile.tile_grid"]
        if section == "sai_pathing":
            prefixes = ["ledger:map_pathing.sai_pathing."]
        for node in list(nodes):
            if any(node["id"].startswith(prefix) for prefix in prefixes):
                add_edge(root_id, node["id"], "includes", "필드 포함",
                         f"{node['sourceRef']}: {section} 구조 하위 항목", False)
    for node in list(nodes):
        path = node["id"].removeprefix("ledger:")
        if ".offsets." in path:
            parent = f"ledger:{path.split('.offsets.')[0]}"
            add_edge(parent, node["id"], "includes", "개별 위치",
                     f"{node['sourceRef']}: offsets 배열 원문", False)
        elif path.startswith("diagnostic_relations.") and path.count(".") >= 2:
            parent = f"ledger:{'.'.join(path.split('.')[:2])}"
            add_edge(parent, node["id"], "includes", "함수 주소",
                     f"{node['sourceRef']}: 개별 RVA 원문", False)

    for pointer_id, target_id in (
        ("ledger:cunit.fields.sprite_ptr", "structure:csprite"),
        ("ledger:bullet.fields.sprite_ptr", "structure:csprite"),
        ("ledger:csprite.fields.primary_image_ptr", "structure:cimage"),
    ):
        add_edge(pointer_id, target_id, "points_to", "포인터 참조",
                 f"{by_id[pointer_id]['sourceRef']}: 필드명과 pointer 타입", True)

    # 현재 사실은 장부를 따르며, 관측 파일은 참조 근거로만 등록한다.
    register(DISCOVERY_ROOT / "larva-internal" / "result.json", "larva_observation")
    # DAT 원문은 descriptor 항목 자체의 전수 목록이다. 기존 장부 표와 RVA로 결합한다.
    dat_counts: dict[str, int] = {}
    for family, (label, relative, expected, default_count) in FAMILIES.items():
        path = DISCOVERY / relative
        if not path.exists():
            continue
        register(path, f"dat_{family}_source")
        lines = path.read_text(encoding="utf-8").splitlines()
        count = 0
        for line_no, line in enumerate(lines, 1):
            if "->" not in line and "descRVA=" not in line:
                continue
            desc = re.search(r"descRVA=(0x[0-9a-fA-F]+)", line)
            target = re.search(r"targetRVA=(0x[0-9a-fA-F]+)", line)
            if not desc or not target:
                pair = re.match(r"^\s*-\s*(0x[0-9a-fA-F]+)\s*(?:->|→)\s*(0x[0-9a-fA-F]+)", line)
                if pair:
                    desc_hex, target_hex = pair.groups()
                else:
                    pair = re.match(
                        r"^\s*-\s*(0x[0-9a-fA-F]+)\s*(?:->|→)\s*VA0x[0-9a-fA-F]+/RVA(0x[0-9a-fA-F]+)",
                        line,
                    )
                    if not pair:
                        continue
                    desc_hex, target_hex = pair.groups()
            else:
                desc_hex, target_hex = desc.group(1), target.group(1)
            desc_offset, target_offset = hex_offset(desc_hex), hex_offset(target_hex)
            if family == "portdata" and not (0xDA9520 <= int(desc_offset, 16) < 0xDA95B0):
                continue
            if family == "mapdata" and int(desc_offset, 16) != 0xDD4EA0:
                continue
            width_match = re.search(r"\bwidth\s*=?\s*(\d+)", line)
            compact = re.search(r"\((\d+)\s*[×x]\s*(\d+)\s*,\s*f\d+\)", line)
            width = int(width_match.group(1)) if width_match else (int(compact.group(1)) if compact else None)
            count_match = re.search(r"\bcount\s*=?\s*(\d+)", line)
            entries = int(count_match.group(1)) if count_match else (int(compact.group(2)) if compact else default_count)
            if not width:
                continue
            count += 1
            ref = f"{relative}:{line_no}"
            descriptor_id = f"dat:{family}:descriptor:{desc_offset}"
            add_node(nid=descriptor_id, key=f"{family}_descriptor_{desc_offset}",
                     label=f"{label} descriptor {desc_offset}", group=f"dat_{family}",
                     kind="descriptor", raw={"line": line.strip(), "width": width, "count": entries},
                     source_ref=ref, basis="module", offset=desc_offset,
                     data_type="descriptor stride 0x18", meaning="DAT 로더 대상 위치·형태 기록",
                     limitations=["저장 이미지 정적 정보. 실제 로드값·게임 효과 미관측."],
                     evidence=[{"kind": "저장 PE", "text": line.strip(), "ref": ref}],
                     legacy_status="", meaning_state="documented", size_bytes=0x18)
            target_id = by_module_offset.get(target_offset)
            if target_id is None:
                target_id = f"dat:{family}:target:{target_offset}"
                add_node(nid=target_id, key=f"{family}_target_{target_offset}",
                         label=f"{label} 대상 {target_offset}", group=f"dat_{family}",
                         kind="table", raw={"line": line.strip(), "width": width, "count": entries},
                         source_ref=ref, basis="module", offset=target_offset,
                         data_type=f"u{width * 8}[{entries}]" if entries else f"u{width * 8}[]",
                         meaning="공식 필드명 미분류", limitations=["독립 소비·실제 로드값·게임 효과는 이 항목에서 확인하지 않음."],
                         evidence=[{"kind": "저장 PE", "text": line.strip(), "ref": ref}],
                         legacy_status="", meaning_state="unclassified",
                         size_bytes=width * entries if entries else None)
            elif entries and by_id[target_id]["sizeBytes"] is None:
                by_id[target_id]["sizeBytes"] = width * entries
                by_id[target_id]["evidence"].append(
                    {"kind": "저장 PE descriptor", "text": line.strip(), "ref": ref}
                )
                by_id[target_id]["evidenceState"] = evidence_state(by_id[target_id]["evidence"])
            add_edge(descriptor_id, target_id, "loads_into", "로드 대상",
                     f"{ref}: descriptor 대상 RVA·폭·개수", False)
        dat_counts[family] = count
        if count != expected:
            raise ValueError(f"{family} descriptor 수가 기대값 {expected}와 다름: {count}")

    # 장부의 진단 함수는 같은 장부에 기재된 입력 RVA와만 연결한다.
    for node in nodes:
        if not node["id"].startswith("ledger:diagnostic_relations."):
            continue
        raw = node["raw"]
        for field in ("input", "output"):
            value = clean_text(raw.get(field))
            for match in re.finditer(r"0x[0-9A-Fa-f]+", value):
                target_id = by_module_offset.get(hex_offset(match.group()))
                if target_id and target_id != node["id"]:
                    add_edge(node["id"], target_id, "references", "주소 참조",
                             f"{node['sourceRef']} {field}: {match.group()}", True)

    for question in ledger.get("memory_map_questions", []):
        if question.get("state", "open") == "resolved":
            continue
        qid = question["id"]
        add_node(nid=f"question:{qid}", key=qid, label=question["label"],
                 group=question.get("group", "other"), kind="question", raw=question,
                 source_ref=f"ledger#memory_map_questions.{qid}",
                 meaning=question.get("meaning", ""), limitations=[question.get("limits", "")],
                 location_state=question.get("locationState", "unknown"), meaning_state="partial",
                 evidence=[{"kind": "장부의 조사 근거", "text": question.get("evidence", ""),
                            "ref": f"ledger#memory_map_questions.{qid}"}])

    for group in groups:
        group.update(display_names.get("groups", {}).get(group["id"], {}))
    group_labels = {group["id"]: group["label"] for group in groups}
    for node in nodes:
        if node["key"] in display_names.get("labels", {}):
            node["label"] = display_names["labels"][node["key"]]
        elif node["kind"] == "descriptor":
            node["label"] = f"{group_labels.get(node['groupId'], '기본 설정')} · 표 로딩 정보 {node['address']['offset']}"
        elif node["id"].startswith("dat:") and "target" in node["id"]:
            node["label"] = f"{group_labels.get(node['groupId'], '기본 설정')} · 표 데이터 {node['address']['offset']}"
    result = {
        "schemaVersion": 1,
        "generatedAt": datetime.now(timezone.utc).isoformat(),
        "title": "StarCraft: Remastered 원천 메모리 지도",
        "target": ledger.get("target", "StarCraft: Remastered Windows x64"),
        "scopeNotes": [
            "기존 장부의 모든 주소는 기준별 상대 위치다. 절대 프로세스 주소는 저장하지 않는다.",
            "locationState=recorded는 위치 기재만 뜻하며 값·의미·실게임 실행 확인을 뜻하지 않는다.",
            "legacyStatus의 accepted는 source profile 구현, validated는 당시 관측, candidate는 진단용이다. 발견률로 사용하지 않는다.",
            "저장 PE DAT descriptor 154개는 기존 위치도 포함하며 실제 로드값과 게임 효과를 확인하지 않는다.",
            "모듈 항목의 단일 표시 그룹은 원문 key 기반 탐색 분류이며, 메모리 소속을 증명하지 않는다.",
            f"기존 장부 적용 범위: {clean_text(ledger.get('ledger_scope'))}",
            f"기존 장부 주소 기준: {clean_text(ledger.get('address_basis'))}",
            f"기존 장부 코드 출처: {clean_text(ledger.get('source_files'))}",
            f"기존 장부 release 관측: {clean_text(ledger.get('current_release_observation'))}",
        ],
        "sources": sources, "groups": groups, "nodes": nodes, "edges": edges,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    pending = args.output.with_name(args.output.name + ".pending")
    pending.write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    pending.replace(args.output)
    print(f"{args.output}: {len(nodes)} nodes, {len(edges)} edges, DAT {sum(dat_counts.values())}")


if __name__ == "__main__":
    main()
