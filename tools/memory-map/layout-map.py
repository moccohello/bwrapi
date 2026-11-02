"""crates/layout/src/lib.rs의 Layout::X64·X86 리터럴을 읽어 x64와 x86을 나란히 보여 주는 HTML 한 장을 만든다.

표준 라이브러리만 쓴다. 출력은 외부 자원이 없는 단일 파일이다.
"""

from __future__ import annotations

import argparse
import html
import re
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
DEFAULT_SOURCE = HERE.parents[1] / "crates" / "layout" / "src" / "lib.rs"
DEFAULT_OUT = HERE.parents[1] / "build" / "memory-map.html"
WIDTHS = ("X64", "X86")


# ---------------------------------------------------------------- Rust 리터럴 해석

class RustLiteral:
    """`pub const X64: Self = Self { ... };` 같은 구조체 리터럴을 dict로 읽는다."""

    NUMBER = re.compile(
        r"-?(?:0[xX][0-9a-fA-F_]+|\d[\d_]*(?:\.\d+)?)(?:_?(?:u8|u16|u32|u64|usize|i8|i16|i32|i64|isize|f32|f64))?"
    )
    IDENT = re.compile(r"[A-Za-z_][A-Za-z0-9_]*(?:::[A-Za-z_][A-Za-z0-9_]*)*")

    def __init__(self, text: str, pos: int):
        self.text = text
        self.pos = pos

    def skip(self) -> None:
        text = self.text
        while self.pos < len(text):
            if text[self.pos].isspace():
                self.pos += 1
            elif text.startswith("//", self.pos):
                end = text.find("\n", self.pos)
                self.pos = len(text) if end < 0 else end + 1
            elif text.startswith("/*", self.pos):
                end = text.find("*/", self.pos)
                self.pos = len(text) if end < 0 else end + 2
            else:
                return

    def expect(self, ch: str) -> None:
        self.skip()
        if not self.text.startswith(ch, self.pos):
            raise ValueError(f"'{ch}'가 필요함: {self.text[self.pos:self.pos + 40]!r}")
        self.pos += len(ch)

    def accept(self, ch: str) -> bool:
        self.skip()
        if self.text.startswith(ch, self.pos):
            self.pos += len(ch)
            return True
        return False

    def number(self, token: str):
        token = re.sub(r"_?(?:u8|u16|u32|u64|usize|i8|i16|i32|i64|isize)$", "", token)
        if token.endswith(("f32", "f64")):
            return float(token[:-3].rstrip("_").replace("_", ""))
        clean = token.replace("_", "").rstrip("_")
        if "." in clean:
            return float(clean)
        return int(clean, 16) if clean.lstrip("-").lower().startswith("0x") else int(clean)

    def value(self):
        self.skip()
        text = self.text
        if text.startswith("&", self.pos):
            self.pos += 1
            self.skip()
        if text.startswith("[", self.pos):
            self.pos += 1
            items = []
            while not self.accept("]"):
                items.append(self.value())
                self.accept(",")
            return items
        match = self.NUMBER.match(text, self.pos)
        if match:
            self.pos = match.end()
            return self.number(match.group())
        match = self.IDENT.match(text, self.pos)
        if not match:
            raise ValueError(f"값을 읽을 수 없음: {text[self.pos:self.pos + 40]!r}")
        name = match.group()
        self.pos = match.end()
        self.skip()
        if name == "None":
            return None
        if name == "Some":
            self.expect("(")
            inner = self.value()
            self.expect(")")
            return inner
        if name in ("true", "false"):
            return name == "true"
        if text.startswith("{", self.pos):
            self.pos += 1
            fields = {}
            while not self.accept("}"):
                self.skip()
                key = self.IDENT.match(text, self.pos)
                if not key:
                    raise ValueError(f"필드 이름을 읽을 수 없음: {text[self.pos:self.pos + 40]!r}")
                self.pos = key.end()
                self.expect(":")
                fields[key.group()] = self.value()
                self.accept(",")
            return fields
        return name


def parse_layouts(text: str) -> dict[str, dict]:
    result = {}
    for name in WIDTHS:
        match = re.search(rf"pub const {name}: Self = (?=Self \{{)", text)
        if not match:
            raise SystemExit(f"Layout::{name} 리터럴을 찾지 못함")
        result[name] = RustLiteral(text, match.end()).value()
    return result


def parse_structs(text: str) -> dict[str, dict]:
    """구조체 정의의 `///` 문서를 모아 필드 설명으로 쓴다."""
    structs: dict[str, dict] = {}
    doc: list[str] = []
    current = None
    for raw in text.splitlines():
        line = raw.strip()
        if line.startswith("///"):
            doc.append(line[3:].strip())
            continue
        if current is None:
            match = re.match(r"pub struct (\w+)\s*\{", line)
            if match:
                current = match.group(1)
                structs[current] = {"doc": " ".join(doc), "fields": {}}
            if not line.startswith("#["):
                doc = []
            continue
        if line == "}":
            current = None
            doc = []
        elif line.startswith("//"):
            continue
        else:
            match = re.match(r"pub (\w+):\s*(.+?),?$", line)
            if match:
                structs[current]["fields"][match.group(1)] = (" ".join(doc), match.group(2))
            doc = []
    return structs


# ---------------------------------------------------------------- 평탄화와 분류

def flatten(value, path: str, out: dict) -> None:
    if isinstance(value, dict):
        for key, item in value.items():
            flatten(item, f"{path}.{key}" if path else key, out)
    elif isinstance(value, list):
        for index, item in enumerate(value):
            flatten(item, f"{path}[{index}]", out)
    else:
        out[path] = value


def present(value) -> bool:
    return isinstance(value, (int, float, bool))


def lookup(flat: dict, path: str):
    """(값, 없는 이유). 값이 있으면 이유는 None."""
    if path in flat:
        value = flat[path]
        if value is None:
            return None, "None"
        return value, None
    parts = re.split(r"(?<=[\w\]])\.", path)
    for cut in range(len(parts) - 1, 0, -1):
        prefix = ".".join(parts[:cut])
        if prefix in flat and flat[prefix] is None:
            return None, "섹션 None"
    return None, "항목 없음"


RVA_TOP = {"scalars", "players", "selection", "commands"}
RVA_EXPLICIT = {
    "cunit.array_pointer_rva", "cunit.capacity_pointer_rva",
    "cunit.native_unit_list_header_module_offset",
    "cunit.unit_lists.invisible_list_first_module_rva",
    "cunit.unit_lists.pylon_list_first_module_rva",
    "bullet.root_block_module_offset",
    "map.active_tile_pointer_module_offset", "map.sai_pathing_pointer_module_offset",
    "replay.header", "recorder.object", "menu_root.pointer", "mouse.pointer",
    "network_provider", "chat.dialog", "chat.close", "save.name", "save.tick",
    "fonts.table", "fonts.cache_glyph", "fonts.pages", "fonts.page_texture",
    "render.renderer", "render.vertices", "render.blank_texture",
}
RVA_EXCLUDE = {
    "players.storm_player_bytes", "commands.chat_context.xor_key", "commands.chat_context.offset",
}
CODE_PATHS = {
    "commands.select_units", "commands.enqueue", "commands.target_order_builder",
    "commands.target_encoder", "commands.move_screen", "commands.print_text",
    "commands.send_text", "commands.end_game", "commands.recompute_turn_durations",
    "chat.close", "fonts.cache_glyph", "fonts.page_texture",
}

SECTION_LABELS = {
    "scalars": "세션·플레이어·화면 스칼라",
    "cunit": "유닛 풀·목록",
    "bullet": "총알 풀",
    "map": "지도·경로",
    "players": "로비·네트워크 플레이어",
    "selection": "선택 표",
    "replay": "리플레이 헤더",
    "recorder": "리플레이 기록기",
    "menu_root": "BWGame 루트(인코딩)",
    "commands": "명령 함수·전역",
    "mouse": "마우스",
    "chat": "채팅 상자",
    "network_provider": "네트워크 제공자",
    "save": "저장",
    "fonts": "글꼴",
    "render": "그리기 큐",
    "resource_writes": "수리·환급 명령 주소",
}

# 구조체 배치도. fields는 (경로 → 표시 이름) 판정 함수다.
CUNIT_LIST_NAMES = ["active", "hidden", "dying", "revealer"]


def _cunit_field(path: str):
    if path.startswith("cunit.unit_lists.") and path.endswith("_offset"):
        return "list." + path[len("cunit.unit_lists."):-len("_offset")]
    if path.startswith("cunit.") and path.endswith("_offset") and "." not in path[6:]:
        return path[6:-len("_offset")]
    return None


def _header_field(path: str):
    match = re.fullmatch(r"cunit\.unit_lists\.header_list_(first|last)_offsets\[(\d+)\]", path)
    if not match:
        return None
    index = int(match.group(2))
    name = CUNIT_LIST_NAMES[index] if index < len(CUNIT_LIST_NAMES) else str(index)
    return f"{name}.{match.group(1)}"


def _suffix_field(prefix: str, names=None):
    def pick(path: str):
        if not path.startswith(prefix):
            return None
        rest = path[len(prefix):]
        if "." in rest:
            return None
        if names is not None and rest not in names:
            return None
        return rest[:-len("_offset")] if rest.endswith("_offset") else rest
    return pick


RECORDS = [
    dict(id="cunit", title="CUnit 레코드", size="cunit.record_stride", field=_cunit_field,
         note="풀 한 칸. 같은 offset을 쓰는 항목은 종류에 따라 달라지는 union 자리다."),
    dict(id="unit_header", title="유닛 목록 머리 블록", size="cunit.native_unit_list_header_bytes",
         field=_header_field, note="active·hidden·dying·revealer 목록의 첫·끝 포인터 자리."),
    dict(id="corder", title="COrder 노드", size="corder.read_bytes", field=_suffix_field("corder.")),
    dict(id="csprite", title="CSprite", size=None, field=_suffix_field("csprite."),
         note="크기는 Layout에 없고 읽는 필드의 끝까지만 보인다."),
    dict(id="cimage", title="CImage", size=None, field=_suffix_field("cimage."),
         note="크기는 Layout에 없고 읽는 필드의 끝까지만 보인다."),
    dict(id="region", title="지역(region) 레코드", size="map.region_stride",
         field=_suffix_field("map.", {"region_neighbors_offset", "region_center_offset",
                                      "region_bounds_offset", "region_properties_offset"})),
    dict(id="sai", title="SAI 경로 할당 안의 표", size="map.sai_pathing_allocation_bytes",
         field=_suffix_field("map.", {"tile_grid", "split_table", "region_table"}),
         note="할당 크기가 커서 비례 막대로 그린다."),
    dict(id="recorder", title="리플레이 기록기 객체", size=None, field=_suffix_field("recorder.", {
         "enabled", "buffer", "used", "open_frame", "last_frame"})),
    dict(id="chat_dialog", title="채팅 대화상자", size=None, field=_suffix_field("chat.", {
         "dialog_first_control", "dialog_event"})),
    dict(id="chat_control", title="채팅 컨트롤", size=None, field=_suffix_field("chat.", {
         "control_next", "control_id", "control_text", "control_text_length"})),
    dict(id="chat_event", title="채팅 이벤트", size=None, field=_suffix_field("chat.", {
         "event_key", "event_kind"})),
    dict(id="font_object", title="글꼴 객체", size=None, field=_suffix_field("fonts.", {
         "glyphs", "glyph_set", "outline"})),
    dict(id="glyph_block", title="glyph 블록(FONT 머리)", size=None, field=_suffix_field("fonts.", {
         "height_byte"})),
    dict(id="glyph_set", title="glyph 집합", size=None, field=_suffix_field("fonts.", {"glyph_tree"})),
    dict(id="glyph_node", title="glyph 트리 노드", size=None, field=_suffix_field("fonts.", {
         "node_left", "node_right", "node_key", "node_glyph"})),
    dict(id="glyph_record", title="glyph 레코드", size=None, field=_suffix_field("fonts.", {
         "glyph_page", "glyph_bearing", "glyph_size", "glyph_atlas", "glyph_advance"})),
    dict(id="draw_command", title="그리기 명령", size="render.command_bytes",
         field=_suffix_field("render.command.")),
    dict(id="vertex_block", title="vertex·index buffer 블록", size=None, field=_suffix_field("render.", {
         "vertex_padding", "index_buffer"})),
    dict(id="draw_queue", title="그리기 queue 객체", size=None, field=_suffix_field("render.", {
         "queue_sort", "queue_count"}), note="queue 안의 정렬 목록과 명령 수 자리."),
]


def is_rva(path: str, top: str) -> bool:
    if path in RVA_EXCLUDE:
        return False
    if path.startswith("commands.chat_context."):
        return path.endswith(".pointer")
    return path in RVA_EXPLICIT or "_rva" in path or "module_offset" in path or top in RVA_TOP


def classify(path: str):
    """('rva'|'record'|'const', 그룹, 표시 이름)."""
    top = re.split(r"[.\[]", path, maxsplit=1)[0]
    if path.startswith("resource_writes.") or is_rva(path, top):
        return "rva", top, path
    for record in RECORDS:
        label = record["field"](path)
        if label is not None:
            return "record", record["id"], label
    return "const", top, path


# ---------------------------------------------------------------- 출력 형식

def fmt(value) -> str:
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, float):
        return repr(value)
    if isinstance(value, int):
        return f"0x{value:X}" if value > 9 else str(value)
    return str(value)


def esc(value) -> str:
    return html.escape(str(value), quote=True)


def doc_for(structs: dict, path: str) -> str:
    current = "Layout"
    doc = ""
    for part in re.split(r"(?<=[\w\]])\.", path):
        name = re.sub(r"\[\d+\]$", "", part)
        field = structs.get(current, {}).get("fields", {}).get(name)
        if not field:
            return ""
        doc, type_text = field
        match = re.search(r"(\w+Layout)\b", type_text)
        current = match.group(1) if match else ""
    return doc


def hue(index: int) -> int:
    return (index * 47) % 360


# ---------------------------------------------------------------- 모델

class Entry:
    def __init__(self, path, kind, group, label):
        self.path, self.kind, self.group, self.label = path, kind, group, label
        self.value = {}
        self.reason = {}

    def status(self) -> str:
        a, b = (present(self.value.get(w)) for w in WIDTHS)
        return "both" if a and b else "x64" if a else "x86" if b else "none"


def build_entries(layouts: dict, flats: dict) -> list[Entry]:
    keys: list[str] = []
    seen = set()
    for width in WIDTHS:
        other = flats[WIDTHS[1 - WIDTHS.index(width)]]
        for path, value in flats[width].items():
            if value is None and any(k.startswith(path + ".") or k.startswith(path + "[") for k in other):
                continue
            if path not in seen:
                seen.add(path)
                keys.append(path)
    entries = []
    for path in keys:
        kind, group, label = classify(path)
        entry = Entry(path, kind, group, label)
        for width in WIDTHS:
            value, reason = lookup(flats[width], path)
            entry.value[width] = value
            entry.reason[width] = reason
        entries.append(entry)
    return entries


def cell(entry: Entry, width: str, module_size=None, with_bar=False) -> str:
    value = entry.value[width]
    if value is None:
        return f'<td class="miss" data-w="{width}"><span class="badge">없음</span> <small>{esc(entry.reason[width])}</small></td>'
    if isinstance(value, list):
        text = "[" + ", ".join(fmt(v) for v in value) + "]"
    else:
        text = fmt(value)
        if isinstance(value, int) and value > 255 and not with_bar:
            text += f' <small>({value})</small>'
    bar = ""
    if with_bar and module_size:
        pct = min(100.0, value / module_size * 100)
        bar = f'<span class="pos"><i style="left:{pct:.3f}%"></i></span>'
    return f'<td data-w="{width}"><code>{text}</code>{bar}</td>'


STATUS_TEXT = {"both": "양쪽", "x64": "x64만", "x86": "x86만", "none": "양쪽 없음"}


def render_rva_section(entries, structs, modules, group_index) -> str:
    out = []
    groups: dict[str, list[Entry]] = {}
    for entry in entries:
        if entry.kind == "rva":
            groups.setdefault(entry.group, []).append(entry)
    for group, items in groups.items():
        color = hue(group_index[group])
        rows = []
        for entry in items:
            doc = doc_for(structs, entry.path)
            addr = {w: (entry.value[w] if isinstance(entry.value[w], int) else -1) for w in WIDTHS}
            rows.append(
                f'<tr class="row s-{entry.status()}" data-key="{esc(entry.path)}" data-sec="{esc(group)}" '
                f'data-a64="{addr["X64"]}" data-a86="{addr["X86"]}" data-order="{len(rows)}">'
                f'<th scope="row" title="{esc(doc)}"><code>{esc(entry.path)}</code></th>'
                + cell(entry, "X64", modules["X64"], True) + cell(entry, "X86", modules["X86"], True)
                + f'<td><span class="st st-{entry.status()}">{STATUS_TEXT[entry.status()]}</span></td></tr>'
            )
        both = sum(1 for e in items if e.status() == "both")
        out.append(
            f'<section class="sec" id="rva-{esc(group)}" data-sec="{esc(group)}" style="--c:hsl({color} var(--sat) var(--lum))">'
            f'<h3><span class="dot"></span>{esc(SECTION_LABELS.get(group, group))} <small>{esc(group)} · {len(items)}개 · 양쪽 {both}</small></h3>'
            f'<table class="grid"><thead><tr><th>항목</th><th>x64 RVA</th><th>x86 RVA</th><th>상태</th></tr></thead>'
            f'<tbody>{"".join(rows)}</tbody></table></section>'
        )
    return "".join(out)


def tracks(entries, modules, group_index) -> str:
    out = []
    for width in WIDTHS:
        for code in (False, True):
            items = [
                e for e in entries
                if e.kind == "rva" and isinstance(e.value[width], int)
                and ((e.path in CODE_PATHS or e.path.startswith("resource_writes.")) == code)
            ]
            label = f'{width.lower()} · {"코드(함수·명령 주소)" if code else "전역 데이터"}'
            if not items:
                out.append(f'<div class="track"><b>{label}</b><div class="axis empty">항목 없음</div></div>')
                continue
            lo = min(e.value[width] for e in items)
            hi = max(e.value[width] for e in items)
            span = max(hi - lo, 1)
            ticks = []
            for e in items:
                pct = (e.value[width] - lo) / span * 100
                ticks.append(
                    f'<a class="tick" href="#" data-key="{esc(e.path)}" data-sec="{esc(e.group)}" '
                    f'style="left:{pct:.3f}%;--c:hsl({hue(group_index[e.group])} var(--sat) var(--lum))" '
                    f'title="{esc(e.path)} = {fmt(e.value[width])}"></a>'
                )
            out.append(
                f'<div class="track"><b>{label}</b> <small>{len(items)}개 · 모듈 크기 {fmt(modules[width])}</small>'
                f'<div class="axis">{"".join(ticks)}</div>'
                f'<div class="range"><span>{fmt(lo)}</span><span>{fmt(hi)}</span></div></div>'
            )
    return "".join(out)


def strip_pieces(fields, total, row_bytes):
    """필드 목록을 offset 순으로 (offset, 이름들) 묶음으로 만들어 행별 조각으로 자른다."""
    groups: dict[int, list[str]] = {}
    for label, offset in fields:
        groups.setdefault(offset, []).append(label)
    starts = sorted(groups)
    segments = []
    if starts and starts[0] > 0:
        segments.append((0, starts[0], None))
    for index, start in enumerate(starts):
        end = starts[index + 1] if index + 1 < len(starts) else total
        end = max(end, start + 1)
        segments.append((start, min(end, max(total, start + 1)), groups[start]))
    rows: dict[int, list] = {}
    for tone, (start, end, names) in enumerate(segments):
        cursor = start
        first = True
        while cursor < end:
            row = cursor // row_bytes
            stop = min(end, (row + 1) * row_bytes)
            rows.setdefault(row, []).append((cursor - row * row_bytes, stop - cursor, names, tone, first, start))
            first = False
            cursor = stop
    return rows


def render_strip(fields, total, width_label) -> str:
    if not fields:
        return f'<div class="strip-box"><b>{width_label}</b><div class="note">필드 없음</div></div>'
    proportional = total > 0x400
    row_bytes = total if proportional else 64
    if not proportional and total <= 64:
        row_bytes = max(8, (total + 7) // 8 * 8)
    rows = strip_pieces(fields, total, row_bytes)
    html_rows = []
    for row_index in range(0, (total - 1) // row_bytes + 1):
        pieces = []
        for offset, length, names, tone, first, start in rows.get(row_index, []):
            left = offset / row_bytes * 100
            wide = length / row_bytes * 100
            if names is None:
                pieces.append(f'<span class="seg gap" style="left:{left:.3f}%;width:{wide:.3f}%" title="미기재 0x{start:X}부터"></span>')
                continue
            name = " / ".join(names)
            union = " u" if len(names) > 1 else ""
            text = esc(name) if first else ""
            pieces.append(
                f'<span class="seg t{tone % 2}{union}" style="left:{left:.3f}%;width:{wide:.3f}%" '
                f'title="{esc(name)} @ 0x{start:X}">{text}</span>'
            )
        label = "" if proportional else f'<span class="rowoff">{row_index * row_bytes:03X}</span>'
        html_rows.append(f'<div class="srow">{label}<div class="sbar">{"".join(pieces)}</div></div>')
    return (
        f'<div class="strip-box"><b>{width_label}</b> <small>{"비례 막대" if proportional else f"한 줄 {row_bytes}바이트"} · 전체 0x{total:X}</small>'
        f'{"".join(html_rows)}</div>'
    )


def render_records(entries, flats, layouts, structs) -> str:
    by_record: dict[str, list[Entry]] = {}
    for entry in entries:
        if entry.kind == "record":
            by_record.setdefault(entry.group, []).append(entry)
    out = []
    for record in RECORDS:
        items = by_record.get(record["id"], [])
        if not items:
            continue
        strips = []
        sizes = {}
        for width in WIDTHS:
            fields = [(e.label, e.value[width]) for e in items if isinstance(e.value[width], int)]
            ptr = layouts[width]["pointer_bytes"]
            size = None
            if record["size"]:
                size, _ = lookup(flats[width], record["size"])
            derived = max((offset for _, offset in fields), default=0) + ptr
            total = size if isinstance(size, int) and size > 0 else derived
            sizes[width] = (size if isinstance(size, int) else None, total)
            strips.append(render_strip(fields, total, width.lower()))
        size_row = ""
        if record["size"]:
            size_entry = next((e for e in entries if e.path == record["size"]), None)
            if size_entry:
                size_row = (f'<tr class="row s-{size_entry.status()}"><th scope="row"><code>{esc(size_entry.path)}</code> <small>크기</small></th>'
                            + cell(size_entry, "X64") + cell(size_entry, "X86")
                            + f'<td><span class="st st-{size_entry.status()}">{STATUS_TEXT[size_entry.status()]}</span></td></tr>')
        rows = []
        for entry in items:
            doc = doc_for(structs, entry.path)
            rows.append(
                f'<tr class="row s-{entry.status()}" data-key="{esc(entry.path)}">'
                f'<th scope="row" title="{esc(doc)}"><code>{esc(entry.label)}</code></th>'
                + cell(entry, "X64") + cell(entry, "X86")
                + f'<td><span class="st st-{entry.status()}">{STATUS_TEXT[entry.status()]}</span></td></tr>'
            )
        missing = [w.lower() for w in WIDTHS if not any(present(e.value[w]) for e in items)]
        warn = f'<p class="warn">{", ".join(missing)}: 이 구조의 값이 Layout에 없음</p>' if missing else ""
        note = f'<p class="note">{esc(record["note"])}</p>' if record.get("note") else ""
        derived_note = ""
        if not record["size"] or any(sizes[w][0] is None for w in WIDTHS):
            derived_note = '<p class="note">크기 값이 없는 쪽은 마지막 필드 끝(offset + 포인터 폭)까지만 그린다.</p>'
        out.append(
            f'<section class="rec" id="rec-{record["id"]}"><h3>{esc(record["title"])} <small>{esc(record["id"])} · 필드 {len(items)}개</small></h3>'
            f'{note}{warn}<div class="pair">{strips[0]}{strips[1]}</div>{derived_note}'
            f'<table class="grid"><thead><tr><th>필드</th><th>x64 offset</th><th>x86 offset</th><th>상태</th></tr></thead>'
            f'<tbody>{size_row}{"".join(rows)}</tbody></table></section>'
        )
    return "".join(out)


def render_consts(entries, structs) -> str:
    groups: dict[str, list[Entry]] = {}
    for entry in entries:
        if entry.kind == "const":
            groups.setdefault(entry.group, []).append(entry)
    # 크기를 가진 항목은 record 쪽 표에서 이미 보이므로 중복 없이 여기서만 나열한다.
    out = []
    for group, items in groups.items():
        rows = "".join(
            f'<tr class="row s-{e.status()}"><th scope="row" title="{esc(doc_for(structs, e.path))}"><code>{esc(e.path)}</code></th>'
            + cell(e, "X64") + cell(e, "X86")
            + f'<td><span class="st st-{e.status()}">{STATUS_TEXT[e.status()]}</span></td></tr>'
            for e in items
        )
        out.append(
            f'<section class="sec const"><h3>{esc(SECTION_LABELS.get(group, group))} <small>{esc(group)} · {len(items)}개</small></h3>'
            f'<table class="grid"><thead><tr><th>항목</th><th>x64</th><th>x86</th><th>상태</th></tr></thead><tbody>{rows}</tbody></table></section>'
        )
    return "".join(out)


CSS = """
:root{color-scheme:light dark;--bg:#f6f7f9;--fg:#1c2230;--muted:#5d6678;--card:#fff;--line:#d9dde5;--x64:#2f6fd6;--x86:#c9731b;
--miss:#8b93a3;--hl:#fff3c4;--sat:62%;--lum:46%;--ok:#2d8a57;--stripe:rgba(120,130,150,.18)}
@media(prefers-color-scheme:dark){:root:not([data-theme=light]){--bg:#12151c;--fg:#e6e9f0;--muted:#98a2b6;--card:#1b2029;--line:#2c3340;--x64:#6ea3ff;--x86:#f0a35a;--miss:#7d8698;--hl:#4a4320;--lum:62%;--ok:#59c88a;--stripe:rgba(160,170,190,.16)}}
:root[data-theme=dark]{color-scheme:dark;--bg:#12151c;--fg:#e6e9f0;--muted:#98a2b6;--card:#1b2029;--line:#2c3340;--x64:#6ea3ff;--x86:#f0a35a;--miss:#7d8698;--hl:#4a4320;--lum:62%;--ok:#59c88a;--stripe:rgba(160,170,190,.16)}
*{box-sizing:border-box}body{margin:0;background:var(--bg);color:var(--fg);font:14px/1.5 system-ui,"Segoe UI","Malgun Gothic",sans-serif}
header{padding:20px 16px 8px;max-width:1280px;margin:0 auto}h1{margin:0 0 4px;font-size:22px}header p{margin:2px 0;color:var(--muted)}
main{max-width:1280px;margin:0 auto;padding:0 16px 48px}nav{position:sticky;top:0;z-index:5;background:var(--bg);border-bottom:1px solid var(--line);padding:8px 16px;display:flex;gap:14px;flex-wrap:wrap;align-items:center}
nav a{color:var(--fg);text-decoration:none;font-weight:600}nav a:hover{text-decoration:underline}nav .sp{flex:1}
h2{font-size:18px;margin:32px 0 8px}h3{font-size:15px;margin:0 0 8px;display:flex;gap:8px;align-items:baseline;flex-wrap:wrap}h3 small{color:var(--muted);font-weight:400}
.cards{display:grid;grid-template-columns:repeat(auto-fit,minmax(150px,1fr));gap:10px;margin:12px 0}
.card,.sec,.rec{background:var(--card);border:1px solid var(--line);border-radius:10px;padding:12px}.sec,.rec{margin:12px 0}
.card b{display:block;font-size:22px}.card span{color:var(--muted)}
.dot{width:10px;height:10px;border-radius:50%;background:var(--c);align-self:center}
.sec{border-left:4px solid var(--c,var(--line))}
.legend{display:flex;gap:14px;flex-wrap:wrap;color:var(--muted);margin:6px 0}.legend i{display:inline-block;width:14px;height:14px;vertical-align:-2px;border-radius:3px;margin-right:4px}
.lg64{background:var(--x64)}.lg86{background:var(--x86)}.lgmiss{background:repeating-linear-gradient(45deg,var(--stripe) 0 4px,transparent 4px 8px);border:1px solid var(--miss)}
.controls{display:flex;gap:10px;flex-wrap:wrap;align-items:center;margin:10px 0}.controls input,.controls select{background:var(--card);color:var(--fg);border:1px solid var(--line);border-radius:6px;padding:5px 8px;font:inherit}
.chips{display:flex;gap:6px;flex-wrap:wrap;margin:8px 0}.chip{border:1px solid var(--line);border-radius:999px;padding:2px 10px;cursor:pointer;background:var(--card);display:inline-flex;gap:6px;align-items:center;user-select:none}
.chip.off{opacity:.4;text-decoration:line-through}.chip .dot{align-self:center}
.track{margin:10px 0}.axis{position:relative;height:30px;border:1px solid var(--line);border-radius:6px;background:var(--card);margin-top:4px}
.axis.empty{display:flex;align-items:center;justify-content:center;color:var(--muted)}
.tick{position:absolute;top:3px;bottom:3px;width:3px;margin-left:-1px;background:var(--c);border-radius:1px}.tick:hover,.tick.hl{outline:2px solid var(--fg);z-index:2}
.tick.hide{display:none}.range{display:flex;justify-content:space-between;color:var(--muted);font-size:12px;font-family:ui-monospace,Consolas,monospace}
.tablewrap{overflow-x:auto}table.grid{width:100%;border-collapse:collapse}.grid th,.grid td{border-top:1px solid var(--line);padding:4px 8px;text-align:left;vertical-align:top}
.grid thead th{color:var(--muted);font-weight:600;border-top:0}.grid tbody th{font-weight:500;max-width:340px;word-break:break-all}
code{font-family:ui-monospace,Consolas,"Cascadia Mono",monospace;font-size:13px}small{color:var(--muted)}
td.miss{background:repeating-linear-gradient(45deg,var(--stripe) 0 4px,transparent 4px 8px);color:var(--miss)}
.badge{border:1px solid var(--miss);border-radius:4px;padding:0 5px;font-size:12px}
.st{border-radius:999px;padding:1px 8px;font-size:12px;border:1px solid var(--line)}.st-x64{color:var(--x64);border-color:var(--x64)}.st-x86{color:var(--x86);border-color:var(--x86)}.st-both{color:var(--ok);border-color:var(--ok)}.st-none{color:var(--miss)}
.row.hl{background:var(--hl)}.row.hide{display:none}.sec.hide{display:none}
.pos{display:block;position:relative;height:6px;margin-top:3px;background:var(--stripe);border-radius:3px;min-width:80px}.pos i{position:absolute;top:-2px;width:3px;height:10px;background:var(--fg);margin-left:-1px;border-radius:1px}
td[data-w=X64] .pos i{background:var(--x64)}td[data-w=X86] .pos i{background:var(--x86)}
.pair{display:grid;grid-template-columns:1fr 1fr;gap:12px;margin:8px 0}@media(max-width:820px){.pair{grid-template-columns:1fr}}
.strip-box b{color:var(--x64)}.pair .strip-box:nth-child(2) b{color:var(--x86)}
.srow{display:flex;align-items:center;gap:4px;margin:2px 0}.rowoff{width:30px;text-align:right;color:var(--muted);font:11px ui-monospace,Consolas,monospace}
.sbar{position:relative;height:24px;flex:1;border:1px solid var(--line);border-radius:4px;overflow:hidden;background:var(--card)}
.seg{position:absolute;top:0;bottom:0;font-size:11px;line-height:22px;padding:0 3px;overflow:hidden;white-space:nowrap;text-overflow:ellipsis;border-right:1px solid var(--card)}
.t0{background:color-mix(in srgb,var(--x64) 32%,var(--card))}.t1{background:color-mix(in srgb,var(--x64) 55%,var(--card))}
.pair .strip-box:nth-child(2) .t0{background:color-mix(in srgb,var(--x86) 32%,var(--card))}.pair .strip-box:nth-child(2) .t1{background:color-mix(in srgb,var(--x86) 55%,var(--card))}
.seg.u{box-shadow:inset 0 -3px 0 var(--fg)}.seg.gap{background:repeating-linear-gradient(45deg,var(--stripe) 0 4px,transparent 4px 8px)}
.note{color:var(--muted);margin:2px 0}.warn{color:var(--x86);margin:2px 0;font-weight:600}
footer{color:var(--muted);margin-top:32px}
"""

JS = """
(function(){
var $=function(s,r){return Array.prototype.slice.call((r||document).querySelectorAll(s))};
function hl(key,on){$('[data-key]').forEach(function(e){if(e.getAttribute('data-key')===key)e.classList.toggle('hl',on)})}
document.addEventListener('mouseover',function(e){var t=e.target.closest('[data-key]');if(t)hl(t.getAttribute('data-key'),true)});
document.addEventListener('mouseout',function(e){var t=e.target.closest('[data-key]');if(t)hl(t.getAttribute('data-key'),false)});
document.addEventListener('click',function(e){var t=e.target.closest('.tick');if(!t)return;e.preventDefault();
 var r=$('tr.row[data-key]').filter(function(x){return x.getAttribute('data-key')===t.getAttribute('data-key')})[0];if(r)r.scrollIntoView({block:'center',behavior:'smooth'})});
var off={};
function apply(){var q=($('#q')[0].value||'').toLowerCase();
 $('.sec[data-sec]').forEach(function(s){var hide=off[s.getAttribute('data-sec')];s.classList.toggle('hide',!!hide)});
 $('.tick').forEach(function(t){t.classList.toggle('hide',!!off[t.getAttribute('data-sec')])});
 $('tr.row').forEach(function(r){var m=!q||r.textContent.toLowerCase().indexOf(q)>=0;r.classList.toggle('hide',!m)})}
$('.chip').forEach(function(c){c.addEventListener('click',function(){var k=c.getAttribute('data-sec');off[k]=!off[k];c.classList.toggle('off',!!off[k]);apply()})});
$('#q')[0].addEventListener('input',apply);
$('#sort')[0].addEventListener('change',function(){var mode=this.value;
 $('.sec[data-sec] tbody').forEach(function(tb){var rows=$('tr',tb);
  rows.sort(function(a,b){if(mode==='order')return a.getAttribute('data-order')-b.getAttribute('data-order');
   var k=mode==='a64'?'data-a64':'data-a86';var x=+a.getAttribute(k),y=+b.getAttribute(k);
   if(x<0)x=Infinity;if(y<0)y=Infinity;return x-y||a.getAttribute('data-order')-b.getAttribute('data-order')});
  rows.forEach(function(r){tb.appendChild(r)})})});
$('#theme')[0].addEventListener('click',function(){var r=document.documentElement;
 var dark=r.getAttribute('data-theme')?r.getAttribute('data-theme')==='dark':matchMedia('(prefers-color-scheme: dark)').matches;
 r.setAttribute('data-theme',dark?'light':'dark')});
})();
"""


def render(layouts, flats, structs, source: Path) -> tuple[str, dict]:
    entries = build_entries(layouts, flats)
    modules = {w: layouts[w]["module_size"] for w in WIDTHS}
    groups = []
    for entry in entries:
        if entry.kind == "rva" and entry.group not in groups:
            groups.append(entry.group)
    group_index = {g: i for i, g in enumerate(groups)}

    def counts(kind):
        items = [e for e in entries if e.kind == kind]
        return len(items), sum(e.status() == "both" for e in items), sum(e.status() == "x64" for e in items), sum(e.status() == "x86" for e in items)

    rva = counts("rva")
    rec = counts("record")
    const = counts("const")
    chips = "".join(
        f'<span class="chip" data-sec="{esc(g)}" style="--c:hsl({hue(group_index[g])} var(--sat) var(--lum))"><span class="dot"></span>{esc(SECTION_LABELS.get(g, g))}</span>'
        for g in groups
    )
    summary = "".join(
        f'<div class="card"><span>{title}</span><b>{total}</b><small>양쪽 {both} · x64만 {only64} · x86만 {only86}</small></div>'
        for title, (total, both, only64, only86) in (("모듈 RVA 항목", rva), ("구조체 필드", rec), ("기타 상수·크기", const))
    )
    body = f"""
<header>
<h1>BWRAPI 메모리 지도</h1>
<p>SCR x64와 x86을 같은 항목끼리 나란히 보여 줍니다. 원천: <code>crates/layout/src/lib.rs</code>의 <code>Layout::X64</code>·<code>Layout::X86</code> ({esc(source.name)} 파싱 결과)</p>
<p>이 화면은 Layout에 적힌 값만 보여 줍니다. 게임에서 그 값을 읽어 확인했는지는 뜻하지 않습니다.</p>
</header>
<nav><a href="#rva">모듈 전역 주소</a><a href="#records">구조체 배치</a><a href="#consts">기타 상수·크기</a><span class="sp"></span><button id="theme" type="button">밝게/어둡게</button></nav>
<main>
<div class="cards">{summary}</div>
<div class="legend"><span><i class="lg64"></i>x64</span><span><i class="lg86"></i>x86</span><span><i class="lgmiss"></i>없음(None·섹션 없음)</span></div>
<h2 id="rva">1. 모듈 전역 주소 (RVA)</h2>
<p class="note">막대는 각 빌드에서 값이 있는 RVA를 낮은 주소부터 높은 주소까지 눌러 그린 축입니다. 점에 마우스를 올리면 표의 같은 항목이 강조되고, 누르면 그 행으로 이동합니다. 두 빌드의 주소는 서로 다른 모듈 이미지 기준입니다.</p>
{tracks(entries, modules, group_index)}
<div class="controls"><input id="q" type="search" placeholder="항목 이름·값 검색"><select id="sort"><option value="order">선언 순서</option><option value="a64">x64 주소순</option><option value="a86">x86 주소순</option></select></div>
<div class="chips">{chips}</div>
<div class="tablewrap">{render_rva_section(entries, structs, modules, group_index)}</div>
<h2 id="records">2. 구조체 배치 (바이트 offset)</h2>
<p class="note">각 칸은 한 필드의 시작 offset부터 다음 필드가 시작하기 전까지입니다. Layout은 필드 폭을 기록하지 않으므로 칸 길이는 필드 크기가 아니라 다음 기재 offset까지의 간격입니다. 같은 offset의 필드는 한 칸에 묶고 밑줄로 표시합니다.</p>
{render_records(entries, flats, layouts, structs)}
<h2 id="consts">3. 기타 상수·크기</h2>
{render_consts(entries, structs)}
<footer>생성: <code>tools/memory-map/layout-map.py</code></footer>
</main>
"""
    page = (
        '<!doctype html><html lang="ko"><head><meta charset="utf-8">'
        '<meta name="viewport" content="width=device-width,initial-scale=1"><title>BWRAPI 메모리 지도</title>'
        f"<style>{CSS}</style></head><body>{body}<script>{JS}</script></body></html>"
    )
    stats = {"rva": rva, "record": rec, "const": const, "groups": len(groups)}
    return page, stats


def default_out() -> Path:
    return DEFAULT_OUT


def main() -> None:
    parser = argparse.ArgumentParser(description="Layout::X64·X86 비교 메모리 지도 HTML 생성")
    parser.add_argument("--source", type=Path, default=DEFAULT_SOURCE, help="Layout 소스(lib.rs)")
    parser.add_argument("--out", type=Path, default=None, help="출력 HTML 경로")
    args = parser.parse_args()
    text = args.source.read_text(encoding="utf-8")
    layouts = parse_layouts(text)
    flats = {}
    for width in WIDTHS:
        flats[width] = {}
        flatten(layouts[width], "", flats[width])
    structs = parse_structs(text)
    page, stats = render(layouts, flats, structs, args.source)
    out = args.out or default_out()
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(page, encoding="utf-8")
    sys.stdout.reconfigure(encoding="utf-8")
    for name, value in (("RVA", stats["rva"]), ("구조체 필드", stats["record"]), ("상수·크기", stats["const"])):
        total, both, only64, only86 = value
        print(f"{name}: {total}개 (양쪽 {both}, x64만 {only64}, x86만 {only86})")
    print(f"{out}")


if __name__ == "__main__":
    main()
