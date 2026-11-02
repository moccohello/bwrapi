//! Generates the C header, the C#/Python declarations and the C ABI reference
//! (`docs/C-ABI.md`) from the engine's table definitions and this crate's doc
//! comments, so the bindings carry no hand-written layout and every binding
//! prints the same contract text: column docs come from the `///` comments of
//! the table definitions, function and type docs from the doc comments in
//! `lib.rs` and `command.rs`.

use std::fmt::Write;

use bwrapi_engine::tables::{TableInfo, FRAME_TABLES, TABLES};
use bwrapi_engine::CellKind;

use crate::command::{COMMAND_FLAGS, COMMAND_KINDS, GAME_COMMAND_KINDS, RESULT_STATES};
use crate::{CHAT_KINDS, SHAPE_COORDINATES, SHAPE_KINDS};

/// The sources whose doc comments are the contract text.
const LIB: &str = include_str!("lib.rs");
const COMMAND: &str = include_str!("command.rs");

/// The command that rewrites `docs/C-ABI.md`, run from the repository root.
pub const REGENERATE: &str = "cargo run -p bwrapi-ffi --bin bwrapi-gen -- --doc docs/C-ABI.md";

/// Widest generated comment line.
const WIDTH: usize = 100;

const STATUSES: [(&str, u32); 7] = [
    ("OK", 0),
    ("NOT_IN_GAME", 1),
    ("TIMEOUT", 2),
    ("GONE", 3),
    ("INVALID", 4),
    ("UNSUPPORTED_BUILD", 5),
    ("NO_COMMANDS", 6),
];
const FRAME_FLAGS: [(&str, u32, &str); 2] = [
    (
        "TORN",
        2,
        "The game frame advanced while the frame was read, twice in a row; rows may mix two game frames. \
         Only a read from another process without the in-game DLL can be torn.",
    ),
    ("REPLAY", 4, "The frame is from a replay."),
];

/// `(name, C type, C# type, ctypes type)` of the frame header fields before the tables.
const HEADER: [(&str, &str, &str, &str); 10] = [
    ("size", "uint32_t", "uint", "c_uint32"),
    ("flags", "uint32_t", "BwrFrameFlags", "c_uint32"),
    ("sequence", "uint64_t", "ulong", "c_uint64"),
    ("sources", "uint64_t", "BwrSource", "c_uint64"),
    ("match_id", "uint32_t", "uint", "c_uint32"),
    ("game_frame", "uint32_t", "uint", "c_uint32"),
    ("map_width", "uint16_t", "ushort", "c_uint16"),
    ("map_height", "uint16_t", "ushort", "c_uint16"),
    ("local_player", "uint8_t", "byte", "c_uint8"),
    ("reserved", "uint8_t[3]", "fixed byte", "c_uint8 * 3"),
];

/// A `#[repr(C)]` struct of the ABI besides the frame: where its Rust
/// definition is, its Python class name, and its fields as `(name, C type,
/// C# type, ctypes type)`; an array's C and C# types end in `[n]`.
struct AbiStruct {
    rust: &'static str,
    source: &'static str,
    python: &'static str,
    fields: &'static [(&'static str, &'static str, &'static str, &'static str)],
}

const COMMAND_STRUCT: AbiStruct = AbiStruct {
    rust: "BwrCommand",
    source: COMMAND,
    python: "Command",
    fields: &[
        ("size", "uint32_t", "uint", "c_uint32"),
        ("kind", "uint32_t", "BwrCommandKind", "c_uint32"),
        ("flags", "uint32_t", "BwrCommandFlags", "c_uint32"),
        ("type_id", "uint32_t", "uint", "c_uint32"),
        ("slot", "uint32_t", "uint", "c_uint32"),
        ("x", "int32_t", "int", "c_int32"),
        ("y", "int32_t", "int", "c_int32"),
        ("target", "uint64_t", "ulong", "c_uint64"),
        ("units", "const uint64_t*", "ulong*", "POINTER(c_uint64)"),
        ("unit_count", "uint32_t", "uint", "c_uint32"),
        ("reserved", "uint32_t", "uint", "c_uint32"),
        ("text", "const char*", "byte*", "c_char_p"),
    ],
};

const RESULT_STRUCT: AbiStruct = AbiStruct {
    rust: "BwrResult",
    source: COMMAND,
    python: "Result",
    fields: &[
        ("id", "uint64_t", "ulong", "c_uint64"),
        ("state", "uint32_t", "BwrResultState", "c_uint32"),
        ("frame_hint", "uint32_t", "uint", "c_uint32"),
        ("actions", "uint32_t", "uint", "c_uint32"),
        ("reserved", "uint32_t", "uint", "c_uint32"),
    ],
};

const CHAT_STRUCT: AbiStruct = AbiStruct {
    rust: "BwrChat",
    source: LIB,
    python: "Chat",
    fields: &[
        ("kind", "uint32_t", "BwrChatKind", "c_uint32"),
        ("player", "uint32_t", "uint", "c_uint32"),
        ("text", "char[256]", "fixed byte[256]", "c_char * 256"),
    ],
};

const SHAPE_STRUCT: AbiStruct = AbiStruct {
    rust: "BwrShape",
    source: LIB,
    python: "Shape",
    fields: &[
        ("kind", "uint8_t", "BwrShapeKind", "c_uint8"),
        ("coordinates", "uint8_t", "BwrShapeCoordinates", "c_uint8"),
        ("solid", "uint8_t", "byte", "c_uint8"),
        ("text_size", "uint8_t", "byte", "c_uint8"),
        ("x1", "int32_t", "int", "c_int32"),
        ("y1", "int32_t", "int", "c_int32"),
        ("x2", "int32_t", "int", "c_int32"),
        ("y2", "int32_t", "int", "c_int32"),
        ("extra1", "int32_t", "int", "c_int32"),
        ("extra2", "int32_t", "int", "c_int32"),
        ("rgba", "uint32_t", "uint", "c_uint32"),
        ("text_offset", "uint32_t", "uint", "c_uint32"),
        ("text_len", "uint32_t", "uint", "c_uint32"),
    ],
};

const DRAW_LIST_STRUCT: AbiStruct = AbiStruct {
    rust: "BwrDrawList",
    source: LIB,
    python: "DrawList",
    fields: &[
        ("size", "uint32_t", "uint", "c_uint32"),
        ("count", "uint32_t", "uint", "c_uint32"),
        ("shapes", "const BwrShape*", "BwrShape*", "POINTER(Shape)"),
        ("text", "const char*", "byte*", "c_char_p"),
        ("text_bytes", "uint32_t", "uint", "c_uint32"),
        ("text_colors", "uint32_t[32]", "fixed uint[32]", "c_uint32 * 32"),
    ],
};

/// Exported functions: name, C declaration, C# import, ctypes result and
/// arguments. Their contracts are the doc comments in `lib.rs`.
const FUNCTIONS: [(&str, &str, &str, &str, &str); 16] = [
    (
        "bwr_abi_version",
        "uint32_t bwr_abi_version(void);",
        "[LibraryImport(Library, EntryPoint = \"bwr_abi_version\")] public static partial uint AbiVersion();",
        "c_uint32",
        "[]",
    ),
    (
        "bwr_open",
        "BwrStatus bwr_open(uint32_t pid, uint32_t flags, BwrSession** out);",
        "[LibraryImport(Library, EntryPoint = \"bwr_open\")] public static partial BwrStatus Open(uint pid, uint flags, out nint session);",
        "c_uint32",
        "[c_uint32, c_uint32, POINTER(c_void_p)]",
    ),
    (
        "bwr_open_self",
        "BwrStatus bwr_open_self(BwrSession** out);",
        "[LibraryImport(Library, EntryPoint = \"bwr_open_self\")] public static partial BwrStatus OpenSelf(out nint session);",
        "c_uint32",
        "[POINTER(c_void_p)]",
    ),
    (
        "bwr_close",
        "void bwr_close(BwrSession* session);",
        "[LibraryImport(Library, EntryPoint = \"bwr_close\")] public static partial void Close(nint session);",
        "None",
        "[c_void_p]",
    ),
    (
        "bwr_subscribe",
        "BwrStatus bwr_subscribe(BwrSession* session, uint64_t sources);",
        "[LibraryImport(Library, EntryPoint = \"bwr_subscribe\")] public static partial BwrStatus Subscribe(nint session, BwrSource sources);",
        "c_uint32",
        "[c_void_p, c_uint64]",
    ),
    (
        "bwr_step",
        "BwrStatus bwr_step(BwrSession* session, uint32_t timeout_ms);",
        "[LibraryImport(Library, EntryPoint = \"bwr_step\")] public static partial BwrStatus Step(nint session, uint timeoutMs);",
        "c_uint32",
        "[c_void_p, c_uint32]",
    ),
    (
        "bwr_acquire",
        "BwrStatus bwr_acquire(BwrSession* session, const BwrFrame** out);",
        "[LibraryImport(Library, EntryPoint = \"bwr_acquire\")] public static partial BwrStatus Acquire(nint session, out BwrFrame* frame);",
        "c_uint32",
        "[c_void_p, POINTER(POINTER(Frame))]",
    ),
    (
        "bwr_release",
        "void bwr_release(BwrSession* session, const BwrFrame* frame);",
        "[LibraryImport(Library, EntryPoint = \"bwr_release\")] public static partial void Release(nint session, BwrFrame* frame);",
        "None",
        "[c_void_p, POINTER(Frame)]",
    ),
    (
        "bwr_last_error",
        "const char* bwr_last_error(const BwrSession* session);",
        "[LibraryImport(Library, EntryPoint = \"bwr_last_error\")] public static partial nint LastError(nint session);",
        "c_char_p",
        "[c_void_p]",
    ),
    (
        "bwr_set_frame_callback",
        "BwrStatus bwr_set_frame_callback(BwrSession* session, void (*callback)(void* user), void* user);",
        "[LibraryImport(Library, EntryPoint = \"bwr_set_frame_callback\")] public static partial BwrStatus SetFrameCallback(nint session, nint callback, nint user);",
        "c_uint32",
        "[c_void_p, CFUNCTYPE(None, c_void_p), c_void_p]",
    ),
    (
        "bwr_submit",
        "BwrStatus bwr_submit(BwrSession* session, const BwrCommand* commands, uint32_t count, uint64_t* out_ids);",
        "[LibraryImport(Library, EntryPoint = \"bwr_submit\")] public static partial BwrStatus Submit(nint session, BwrCommand* commands, uint count, ulong* ids);",
        "c_uint32",
        "[c_void_p, POINTER(Command), c_uint32, POINTER(c_uint64)]",
    ),
    (
        "bwr_poll_result",
        "BwrStatus bwr_poll_result(BwrSession* session, uint64_t id, BwrResult* out);",
        "[LibraryImport(Library, EntryPoint = \"bwr_poll_result\")] public static partial BwrStatus PollResult(nint session, ulong id, out BwrResult result);",
        "c_uint32",
        "[c_void_p, c_uint64, POINTER(Result)]",
    ),
    (
        "bwr_inject",
        "BwrStatus bwr_inject(uint32_t pid, const char* module, const char* export_name);",
        "[LibraryImport(Library, EntryPoint = \"bwr_inject\", StringMarshalling = StringMarshalling.Utf8)] public static partial BwrStatus Inject(uint pid, string module, string export);",
        "c_uint32",
        "[c_uint32, c_char_p, c_char_p]",
    ),
    (
        "bwr_capture_chat",
        "BwrStatus bwr_capture_chat(BwrSession* session, uint32_t enable);",
        "[LibraryImport(Library, EntryPoint = \"bwr_capture_chat\")] public static partial BwrStatus CaptureChat(nint session, uint enable);",
        "c_uint32",
        "[c_void_p, c_uint32]",
    ),
    (
        "bwr_next_chat",
        "BwrStatus bwr_next_chat(BwrSession* session, BwrChat* out);",
        "[LibraryImport(Library, EntryPoint = \"bwr_next_chat\")] public static partial BwrStatus NextChat(nint session, out BwrChat chat);",
        "c_uint32",
        "[c_void_p, POINTER(Chat)]",
    ),
    (
        "bwr_draw",
        "BwrStatus bwr_draw(BwrSession* session, const BwrDrawList* list);",
        "[LibraryImport(Library, EntryPoint = \"bwr_draw\")] public static partial BwrStatus Draw(nint session, BwrDrawList* list);",
        "c_uint32",
        "[c_void_p, POINTER(DrawList)]",
    ),
];

/// How the ABI fits together: the overview of the header and the reference.
const OVERVIEW: [(&str, &str); 8] = [
    (
        "Sessions",
        "`bwr_open(pid)` opens a running game from any process; `bwr_open_self` opens the calling process from \
         code running inside the game. A session is used by one thread at a time. A call that can fail returns a \
         `BwrStatus`; `bwr_last_error` gives the message of a failure.",
    ),
    (
        "Sources and frames",
        "`bwr_subscribe` selects sources (`BWR_SOURCE_*`); each is one column table. `bwr_acquire` reads them \
         into a frame: a header and one table pointer per source, NULL when the frame does not carry it. A \
         table is `rows` plus one array per column; cell `i` of every column belongs to row `i`. The frame \
         stays valid and unchanged until `bwr_release`. TERRAIN, WALKABILITY, REGIONS and REGION_NEIGHBORS \
         are read once per match and shared by its frames.",
    ),
    (
        "Values",
        "Positions are map pixels unless a column says build tiles (32 pixels) or walk tiles (8 pixels); unit \
         x/y are the unit's center. Unit types, orders, techs, upgrades and bullet types are the game's ids, \
         which are BWAPI's numbering. Unit ids are `(slot + 1) | uniqueness << 16`, the same for a unit's whole \
         life; 0 is no unit. A value the game does not have reads its type's sentinel: unsigned columns the \
         type maximum (`UINT8_MAX`, `UINT16_MAX`, `UINT32_MAX`, `UINT64_MAX`), signed columns the minimum \
         (`INT16_MIN`, `INT32_MIN`), text empty. Flag columns are `uint8_t` 0 or 1, 255 when unknown.",
    ),
    (
        "Step loop",
        "From another process: `bwr_step` waits for the next game frame, `bwr_acquire` reads it, \
         `bwr_release` returns it. Inside the game, `bwr_set_frame_callback` runs a callback once per game \
         frame on the game thread while the game waits, and the callback makes the same calls.",
    ),
    (
        "Commands",
        "They need a game running the in-game DLL (`bwrapi.dll` inside the game, for example loaded with \
         `bwr_inject`). `bwr_submit` queues `BwrCommand`s and returns their ids at once; they run at the next \
         frame boundary, or right after the frame callback that queued them. `bwr_poll_result` reads each \
         id's `BwrResult`. `BWR_RESULT_OK` means the game's command functions ran; whether the order changes \
         the game shows in later frames.",
    ),
    (
        "Chat",
        "`bwr_capture_chat` keeps the lines the local player types (instead of sending them) and the lines \
         the game receives; `bwr_next_chat` takes them. Needs a `bwr_open_self` session in a game running the \
         frame hook.",
    ),
    (
        "Drawing",
        "`bwr_draw` draws a list of BWAPI shapes into every screen update until the next call. Needs a \
         `bwr_open_self` session in a game running the frame hook.",
    ),
    (
        "In the game or outside",
        "Reading works from another process without injecting anything. When the game runs the in-game DLL, \
         a `bwr_open` session copies the frames its game thread publishes, with the same contents. Commands, \
         frame callbacks, chat and drawing need the in-game DLL.",
    ),
];

fn c_type(kind: CellKind) -> &'static str {
    match kind {
        CellKind::U8 => "uint8_t",
        CellKind::U16 => "uint16_t",
        CellKind::U32 => "uint32_t",
        CellKind::U64 => "uint64_t",
        CellKind::I16 => "int16_t",
        CellKind::I32 => "int32_t",
        CellKind::Text32 => "BwrText32",
        CellKind::Text256 => "BwrText256",
    }
}

fn cs_type(kind: CellKind) -> &'static str {
    match kind {
        CellKind::U8 => "byte",
        CellKind::U16 => "ushort",
        CellKind::U32 => "uint",
        CellKind::U64 => "ulong",
        CellKind::I16 => "short",
        CellKind::I32 => "int",
        CellKind::Text32 => "BwrText32",
        CellKind::Text256 => "BwrText256",
    }
}

fn py_type(kind: CellKind) -> &'static str {
    match kind {
        CellKind::U8 => "c_uint8",
        CellKind::U16 => "c_uint16",
        CellKind::U32 => "c_uint32",
        CellKind::U64 => "c_uint64",
        CellKind::I16 => "c_int16",
        CellKind::I32 => "c_int32",
        CellKind::Text32 => "Text32",
        CellKind::Text256 => "Text256",
    }
}

fn upper(name: &str) -> String {
    name.to_ascii_uppercase()
}

fn sources(mut line: impl FnMut(&TableInfo, usize)) {
    for (index, table) in TABLES.iter().enumerate() {
        line(table, index);
    }
}

// --- Doc comments of the Rust sources ---------------------------------------

/// The `///` lines right above line `at`, attributes skipped.
fn doc_above(lines: &[&str], at: usize) -> String {
    let mut doc = Vec::new();
    for line in lines[..at].iter().rev() {
        let line = line.trim();
        if let Some(text) = line.strip_prefix("///") {
            doc.push(text.strip_prefix(' ').unwrap_or(text));
        } else if !line.starts_with("#[") {
            break;
        }
    }
    doc.reverse();
    doc.join("\n")
}

/// Doc comment of the item of `source` whose line starts with `item`.
fn item_doc(source: &str, item: &str) -> String {
    let lines: Vec<&str> = source.lines().collect();
    let at = lines.iter().position(|line| line.trim_start().starts_with(item)).unwrap_or_else(|| panic!("no `{item}`"));
    doc_above(&lines, at)
}

/// Doc comment of the field (or variant) starting with `field` inside the
/// struct (or enum) whose line starts with `scope`.
fn member_doc(source: &str, scope: &str, field: &str) -> String {
    let lines: Vec<&str> = source.lines().collect();
    let start = lines.iter().position(|line| line.trim_start().starts_with(scope)).unwrap_or_else(|| panic!("no `{scope}`"));
    let at = lines[start..]
        .iter()
        .take_while(|line| line.trim() != "}")
        .position(|line| line.trim_start().starts_with(field))
        .unwrap_or_else(|| panic!("no `{field}` in `{scope}`"));
    doc_above(&lines, start + at)
}

/// Contract of exported function `name`.
fn function_doc(name: &str) -> String {
    let lines: Vec<&str> = LIB.lines().collect();
    let item = format!(" fn {name}(");
    let at = lines.iter().position(|line| line.contains(&item)).unwrap_or_else(|| panic!("no `{name}`"));
    doc_above(&lines, at)
}

fn field_doc(abi: &AbiStruct, field: &str) -> String {
    member_doc(abi.source, &format!("pub struct {} ", abi.rust), &format!("pub {field}:"))
}

fn struct_doc(abi: &AbiStruct) -> String {
    item_doc(abi.source, &format!("pub struct {} ", abi.rust))
}

fn status_doc(name: &str) -> String {
    member_doc(LIB, "pub enum BwrStatus", &format!("{} =", pascal(name)))
}

fn frame_field_doc(name: &str) -> String {
    member_doc(LIB, "pub struct BwrFrame ", &format!("pub {name}:"))
}

/// The text before the `- ` list of a constant table's doc comment.
fn list_intro(doc: &str) -> String {
    doc.lines().take_while(|line| !line.starts_with("- ")).collect::<Vec<_>>().join("\n")
}

/// The `` - `NAME`: text `` entry of a constant table's doc comment, with its
/// indented continuation lines.
fn list_entry(doc: &str, name: &str) -> String {
    let head = format!("- `{name}`:");
    let mut lines = doc.lines().skip_while(|line| !line.starts_with(&head));
    let Some(first) = lines.next() else { return String::new() };
    let mut text = first[head.len()..].trim().to_owned();
    for line in lines.take_while(|line| line.starts_with("  ")) {
        text.push(' ');
        text.push_str(line.trim());
    }
    text
}

/// Doc text as paragraphs of one line each, without rustdoc link syntax;
/// a `# Safety` heading starts a "Safety:" paragraph.
fn paragraphs(doc: &str) -> Vec<String> {
    let doc = doc.replace("[`", "`").replace("`]", "`");
    let mut out = Vec::new();
    let mut current = String::new();
    for line in doc.lines().map(str::trim) {
        if line.is_empty() || line == "# Safety" {
            if !current.is_empty() {
                out.push(std::mem::take(&mut current));
            }
            if line == "# Safety" {
                current.push_str("Safety:");
            }
            continue;
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(line);
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() && line.len() + 1 + word.len() > width {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

/// Wrapped lines of `paragraphs`, an empty line between two paragraphs.
fn comment_lines(paragraphs: &[String], width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    for (index, paragraph) in paragraphs.iter().enumerate() {
        if index > 0 {
            lines.push(String::new());
        }
        lines.extend(wrap(paragraph, width));
    }
    lines
}

// --- Comment writers -----------------------------------------------------------

fn c_comment(o: &mut String, indent: &str, doc: &[String]) {
    let doc: Vec<String> = doc.iter().map(|text| text.replace("*/", "* /")).collect();
    let lines = comment_lines(&doc, WIDTH - indent.len() - 3);
    for (index, line) in lines.iter().enumerate() {
        let lead = if index == 0 { "/* " } else { "   " };
        let tail = if index + 1 == lines.len() { " */" } else { "" };
        writeln!(o, "{}", format!("{indent}{lead}{line}{tail}").trim_end()).unwrap();
    }
}

/// A declaration with its doc: on the same line when it fits, else above.
fn c_member(o: &mut String, indent: &str, declaration: &str, doc: &[String]) {
    match doc {
        [] => writeln!(o, "{indent}{declaration}").unwrap(),
        [text] if indent.len() + declaration.len() + text.len() + 7 <= WIDTH => {
            writeln!(o, "{indent}{declaration} /* {} */", text.replace("*/", "* /")).unwrap()
        }
        _ => {
            c_comment(o, indent, doc);
            writeln!(o, "{indent}{declaration}").unwrap();
        }
    }
}

fn xml(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn cs_doc(o: &mut String, indent: &str, doc: &[String]) {
    let doc: Vec<String> = doc.iter().map(|text| xml(text)).collect();
    match doc.as_slice() {
        [] => {}
        [text] if indent.len() + text.len() + 24 <= WIDTH => writeln!(o, "{indent}/// <summary>{text}</summary>").unwrap(),
        _ => {
            writeln!(o, "{indent}/// <summary>").unwrap();
            for line in comment_lines(&doc, WIDTH - indent.len() - 4) {
                writeln!(o, "{}", format!("{indent}/// {line}").trim_end()).unwrap();
            }
            writeln!(o, "{indent}/// </summary>").unwrap();
        }
    }
}

fn py_doc(o: &mut String, indent: &str, doc: &[String]) {
    for line in comment_lines(doc, WIDTH - indent.len() - 2) {
        writeln!(o, "{}", format!("{indent}# {line}").trim_end()).unwrap();
    }
}

/// One-line Markdown table cell.
fn md_cell(doc: &[String]) -> String {
    doc.join(" ").replace('|', "\\|")
}

fn one(text: &str) -> Vec<String> {
    paragraphs(text)
}

// --- Constant groups -----------------------------------------------------------

/// A group of named constants with their docs: `(name, value, doc)`.
type Constants = Vec<(&'static str, u64, Vec<String>)>;

fn listed<T: Copy + Into<u64>>(source: &str, item: &str, values: &[(&'static str, T)]) -> (Vec<String>, Constants) {
    let doc = item_doc(source, item);
    let constants = values.iter().map(|(name, value)| (*name, (*value).into(), one(&list_entry(&doc, name)))).collect();
    (paragraphs(&list_intro(&doc)), constants)
}

fn statuses() -> Constants {
    STATUSES.iter().map(|(name, value)| (*name, u64::from(*value), paragraphs(&status_doc(name)))).collect()
}

fn frame_flags() -> Constants {
    FRAME_FLAGS.iter().map(|(name, value, doc)| (*name, u64::from(*value), one(doc))).collect()
}

fn command_flags() -> (Vec<String>, Constants) {
    listed(COMMAND, "pub const COMMAND_FLAGS", &COMMAND_FLAGS)
}

fn unit_kinds() -> (Vec<String>, Constants) {
    listed(COMMAND, "pub const COMMAND_KINDS", &COMMAND_KINDS)
}

fn game_kinds() -> (Vec<String>, Constants) {
    listed(COMMAND, "pub const GAME_COMMAND_KINDS", &GAME_COMMAND_KINDS)
}

fn result_states() -> (Vec<String>, Constants) {
    listed(COMMAND, "pub const RESULT_STATES", &RESULT_STATES)
}

fn chat_kinds() -> (Vec<String>, Constants) {
    listed(LIB, "pub const CHAT_KINDS", &CHAT_KINDS)
}

fn shape_kinds() -> (Vec<String>, Constants) {
    listed(LIB, "pub const SHAPE_KINDS", &SHAPE_KINDS)
}

fn shape_coordinates() -> (Vec<String>, Constants) {
    listed(LIB, "pub const SHAPE_COORDINATES", &SHAPE_COORDINATES)
}

/// What a table's rows are, from its definition.
fn table_doc(table: &TableInfo) -> Vec<String> {
    paragraphs(table.doc)
}

fn source_doc(index: usize) -> String {
    let table = &TABLES[index];
    let shared = if index >= FRAME_TABLES { " Read once per match and shared by its frames." } else { "" };
    format!("`Bwr{}` table at `frame->{}`.{shared}", table.type_name, table.name)
}

// --- C ---------------------------------------------------------------------------

fn c_defines(o: &mut String, prefix: &str, constants: &Constants) {
    for (name, value, doc) in constants {
        c_member(o, "", &format!("#define {prefix}{name} {value}u"), doc);
    }
}

fn c_struct(o: &mut String, abi: &AbiStruct) {
    c_comment(o, "", &paragraphs(&struct_doc(abi)));
    writeln!(o, "typedef struct {} {{", abi.rust).unwrap();
    for (name, c, _, _) in abi.fields {
        let declaration = match c.split_once('[') {
            Some((ty, count)) => format!("{ty} {name}[{count};"),
            None => format!("{c} {name};"),
        };
        c_member(o, "    ", &declaration, &paragraphs(&field_doc(abi, name)));
    }
    writeln!(o, "}} {};\n", abi.rust).unwrap();
}

pub fn c_header() -> String {
    let mut out = String::new();
    let o = &mut out;
    writeln!(o, "/* Generated from the bwrapi engine tables and the bwrapi crate's doc comments. Do not edit. */").unwrap();
    writeln!(o, "/* bwrapi.h: C ABI {}.{} of bwrapi.dll. Reference: docs/C-ABI.md. */\n", super::ABI_MAJOR, super::ABI_MINOR).unwrap();
    for (title, text) in OVERVIEW {
        c_comment(o, "", &one(&format!("{title}: {text}")));
    }
    writeln!(o, "\n#ifndef BWRAPI_H\n#define BWRAPI_H\n\n#include <stddef.h>\n#include <stdint.h>\n").unwrap();
    writeln!(o, "#ifdef __cplusplus\nextern \"C\" {{\n#endif\n").unwrap();
    writeln!(o, "/* The ABI version this header declares; see bwr_abi_version. */").unwrap();
    writeln!(o, "#define BWR_ABI_MAJOR {}\n#define BWR_ABI_MINOR {}\n", super::ABI_MAJOR, super::ABI_MINOR).unwrap();
    c_comment(o, "", &paragraphs(&item_doc(LIB, "pub enum BwrStatus")));
    writeln!(o, "typedef enum BwrStatus {{").unwrap();
    for (name, value, doc) in statuses() {
        c_member(o, "    ", &format!("BWR_{name} = {value},"), &doc);
    }
    writeln!(o, "}} BwrStatus;\n").unwrap();
    c_comment(o, "", &one("BwrFrame.flags bits."));
    c_defines(o, "BWR_FRAME_", &frame_flags());
    writeln!(o).unwrap();
    c_comment(o, "", &one("Sources for bwr_subscribe and BwrFrame.sources: source bit `i` is table `i` of BwrFrame."));
    sources(|table, index| {
        c_member(o, "", &format!("#define BWR_SOURCE_{} (1ull << {index})", upper(table.name)), &one(&source_doc(index)))
    });
    writeln!(o, "\n/* NUL-padded text, UTF-8 except for names the game keeps in its legacy code page. */\ntypedef struct BwrText32 {{ char text[32]; }} BwrText32;\ntypedef struct BwrText256 {{ char text[256]; }} BwrText256;\n").unwrap();
    for table in TABLES {
        c_comment(o, "", &table_doc(table));
        writeln!(o, "typedef struct Bwr{} {{\n    size_t rows;", table.type_name).unwrap();
        for column in table.columns {
            c_member(o, "    ", &format!("const {}* {};", c_type(column.kind), column.name), &paragraphs(column.doc));
        }
        writeln!(o, "}} Bwr{};\n", table.type_name).unwrap();
    }
    c_comment(o, "", &paragraphs(&item_doc(LIB, "pub struct BwrFrame ")));
    writeln!(o, "typedef struct BwrFrame {{").unwrap();
    for (name, c, _, _) in HEADER {
        let declaration = match c.split_once('[') {
            Some((ty, count)) => format!("{ty} {name}[{count};"),
            None => format!("{c} {name};"),
        };
        c_member(o, "    ", &declaration, &paragraphs(&frame_field_doc(name)));
    }
    writeln!(o, "    /* NULL when the frame does not carry the source. */").unwrap();
    for table in TABLES {
        writeln!(o, "    const Bwr{}* {};", table.type_name, table.name).unwrap();
    }
    writeln!(o, "}} BwrFrame;\n").unwrap();
    writeln!(o, "/* A session from bwr_open or bwr_open_self; used by one thread at a time. */").unwrap();
    writeln!(o, "typedef struct BwrSession BwrSession;\n").unwrap();
    let (intro, flags) = command_flags();
    c_comment(o, "", &intro);
    c_defines(o, "BWR_COMMAND_FLAG_", &flags);
    writeln!(o).unwrap();
    let (intro, kinds) = unit_kinds();
    c_comment(o, "", &intro);
    c_defines(o, "BWR_COMMAND_", &kinds);
    writeln!(o).unwrap();
    let (intro, kinds) = game_kinds();
    c_comment(o, "", &intro);
    c_defines(o, "BWR_COMMAND_", &kinds);
    writeln!(o).unwrap();
    let (intro, states) = result_states();
    c_comment(o, "", &intro);
    c_defines(o, "BWR_RESULT_", &states);
    writeln!(o).unwrap();
    c_struct(o, &COMMAND_STRUCT);
    c_struct(o, &RESULT_STRUCT);
    let (intro, kinds) = chat_kinds();
    c_comment(o, "", &intro);
    c_defines(o, "BWR_CHAT_", &kinds);
    writeln!(o).unwrap();
    c_struct(o, &CHAT_STRUCT);
    let (intro, kinds) = shape_kinds();
    c_comment(o, "", &intro);
    c_defines(o, "BWR_SHAPE_", &kinds);
    let (intro, coordinates) = shape_coordinates();
    c_comment(o, "", &intro);
    c_defines(o, "BWR_COORDINATES_", &coordinates);
    writeln!(o).unwrap();
    c_struct(o, &SHAPE_STRUCT);
    c_struct(o, &DRAW_LIST_STRUCT);
    for (name, declaration, _, _, _) in FUNCTIONS {
        c_comment(o, "", &paragraphs(&function_doc(name)));
        writeln!(o, "{declaration}\n").unwrap();
    }
    writeln!(o, "#ifdef __cplusplus\n}}\n#endif\n\n#endif").unwrap();
    out
}

// --- C# --------------------------------------------------------------------------

fn cs_enum(o: &mut String, constants: &Constants) {
    for (name, value, doc) in constants {
        cs_doc(o, "    ", doc);
        writeln!(o, "    {} = {value},", pascal(name)).unwrap();
    }
}

fn cs_struct(o: &mut String, abi: &AbiStruct) {
    cs_doc(o, "", &paragraphs(&struct_doc(abi)));
    let unsafe_ = abi.fields.iter().any(|(_, _, cs, _)| cs.contains('*') || cs.starts_with("fixed"));
    writeln!(o, "[StructLayout(LayoutKind.Sequential)]\npublic {}struct {}\n{{", if unsafe_ { "unsafe " } else { "" }, abi.rust).unwrap();
    for (name, _, cs, _) in abi.fields {
        cs_doc(o, "    ", &paragraphs(&field_doc(abi, name)));
        match cs.split_once('[') {
            Some((ty, count)) => writeln!(o, "    public {ty} {}[{count};", pascal(name)).unwrap(),
            None => writeln!(o, "    public {cs} {};", pascal(name)).unwrap(),
        }
    }
    writeln!(o, "}}\n").unwrap();
}

pub fn csharp() -> String {
    let mut out = String::new();
    let o = &mut out;
    writeln!(o, "// Generated from the bwrapi engine tables and the bwrapi crate's doc comments. Do not edit.\n// Reference: docs/C-ABI.md.\n#nullable enable\nusing System;\nusing System.Runtime.InteropServices;\n").unwrap();
    writeln!(o, "namespace BwrApi.Native;\n").unwrap();
    cs_doc(o, "", &paragraphs(&item_doc(LIB, "pub enum BwrStatus")));
    writeln!(o, "public enum BwrStatus : uint\n{{").unwrap();
    cs_enum(o, &statuses());
    let (intro, flags) = command_flags();
    writeln!(o, "}}\n").unwrap();
    cs_doc(o, "", &intro);
    writeln!(o, "[Flags]\npublic enum BwrCommandFlags : uint\n{{\n    None = 0,").unwrap();
    cs_enum(o, &flags);
    writeln!(o, "}}\n\n/// <summary>Command kinds: the unit kinds, then the game kinds.</summary>\npublic enum BwrCommandKind : uint\n{{").unwrap();
    cs_enum(o, &unit_kinds().1);
    cs_enum(o, &game_kinds().1);
    let (intro, states) = result_states();
    writeln!(o, "}}\n").unwrap();
    cs_doc(o, "", &intro);
    writeln!(o, "public enum BwrResultState : uint\n{{").unwrap();
    cs_enum(o, &states);
    writeln!(o, "}}\n").unwrap();
    cs_struct(o, &COMMAND_STRUCT);
    cs_struct(o, &RESULT_STRUCT);
    let (intro, kinds) = chat_kinds();
    cs_doc(o, "", &intro);
    writeln!(o, "public enum BwrChatKind : uint\n{{\n    None = 0,").unwrap();
    cs_enum(o, &kinds);
    writeln!(o, "}}\n").unwrap();
    cs_struct(o, &CHAT_STRUCT);
    let (intro, kinds) = shape_kinds();
    cs_doc(o, "", &intro);
    writeln!(o, "public enum BwrShapeKind : byte\n{{").unwrap();
    cs_enum(o, &kinds);
    let (intro, coordinates) = shape_coordinates();
    writeln!(o, "}}\n").unwrap();
    cs_doc(o, "", &intro);
    writeln!(o, "public enum BwrShapeCoordinates : byte\n{{").unwrap();
    cs_enum(o, &coordinates);
    writeln!(o, "}}\n").unwrap();
    cs_struct(o, &SHAPE_STRUCT);
    cs_struct(o, &DRAW_LIST_STRUCT);
    writeln!(o, "/// <summary>Sources for Subscribe and BwrFrame.Sources: bit i is table i of BwrFrame.</summary>\n[Flags]\npublic enum BwrSource : ulong\n{{\n    None = 0,").unwrap();
    sources(|table, index| {
        cs_doc(o, "    ", &one(&source_doc(index)));
        writeln!(o, "    {} = 1UL << {index},", pascal(table.name)).unwrap()
    });
    writeln!(o, "}}\n\n[Flags]\npublic enum BwrFrameFlags : uint\n{{\n    None = 0,").unwrap();
    cs_enum(o, &frame_flags());
    writeln!(o, "}}\n").unwrap();
    for size in [32, 256] {
        writeln!(o, "/// <summary>NUL-padded text, UTF-8 except for names the game keeps in its legacy code page.</summary>\n[System.Runtime.CompilerServices.InlineArray({size})]\npublic struct BwrText{size}\n{{\n    private byte _first;\n\n    public override readonly string ToString()\n    {{\n        ReadOnlySpan<byte> bytes = this;\n        int end = bytes.IndexOf((byte)0);\n        return System.Text.Encoding.UTF8.GetString(end < 0 ? bytes : bytes[..end]);\n    }}\n}}\n").unwrap();
    }
    // Columns are spans over native memory, valid until the frame is released.
    for table in TABLES {
        cs_doc(o, "", &table_doc(table));
        writeln!(o, "[StructLayout(LayoutKind.Sequential)]\npublic readonly unsafe struct Bwr{}\n{{\n    public readonly nuint Rows;", table.type_name).unwrap();
        for column in table.columns {
            writeln!(o, "    private readonly {}* _{};", cs_type(column.kind), column.name).unwrap();
        }
        for column in table.columns {
            cs_doc(o, "    ", &paragraphs(column.doc));
            writeln!(o, "    public ReadOnlySpan<{ty}> {} => new(_{}, checked((int)Rows));", pascal(column.name), column.name, ty = cs_type(column.kind)).unwrap();
        }
        writeln!(o, "}}\n").unwrap();
    }
    cs_doc(o, "", &paragraphs(&item_doc(LIB, "pub struct BwrFrame ")));
    writeln!(o, "[StructLayout(LayoutKind.Sequential)]\npublic readonly unsafe struct BwrFrame\n{{").unwrap();
    for (name, _, cs, _) in HEADER {
        match cs {
            "fixed byte" => writeln!(o, "    private readonly byte _{name}0, _{name}1, _{name}2;").unwrap(),
            _ => {
                cs_doc(o, "    ", &paragraphs(&frame_field_doc(name)));
                writeln!(o, "    public readonly {cs} {};", pascal(name)).unwrap()
            }
        }
    }
    for table in TABLES {
        writeln!(o, "    private readonly Bwr{}* _{};", table.type_name, table.name).unwrap();
    }
    for table in TABLES {
        writeln!(o, "    /// <summary>Null when the frame does not carry the source.</summary>\n    public Bwr{ty}? {} => _{} == null ? null : *_{};", pascal(table.name), table.name, table.name, ty = table.type_name).unwrap();
    }
    writeln!(o, "}}\n").unwrap();
    writeln!(o, "public static unsafe partial class BwrNative\n{{\n    public const string Library = \"bwrapi\";\n    public const uint AbiMajor = {};", super::ABI_MAJOR).unwrap();
    for (name, _, import, _, _) in FUNCTIONS {
        cs_doc(o, "    ", &paragraphs(&function_doc(name)));
        writeln!(o, "    {import}").unwrap();
    }
    writeln!(o, "}}").unwrap();
    out
}

// --- Python ----------------------------------------------------------------------

fn py_constants(o: &mut String, prefix: &str, constants: &Constants) {
    for (name, value, doc) in constants {
        py_doc(o, "", doc);
        writeln!(o, "{prefix}{name} = {value}").unwrap();
    }
}

fn py_struct(o: &mut String, abi: &AbiStruct) {
    writeln!(o, "\n").unwrap();
    py_doc(o, "", &paragraphs(&struct_doc(abi)));
    writeln!(o, "class {}(Structure):\n    _fields_ = [", abi.python).unwrap();
    for (name, _, _, py) in abi.fields {
        py_doc(o, "        ", &paragraphs(&field_doc(abi, name)));
        writeln!(o, "        (\"{name}\", {py}),").unwrap();
    }
    writeln!(o, "    ]").unwrap();
}

pub fn python() -> String {
    let mut out = String::new();
    let o = &mut out;
    writeln!(o, "\"\"\"Generated from the bwrapi engine tables and the bwrapi crate's doc comments. Do not edit.\n\nReference: docs/C-ABI.md.\n\"\"\"\n").unwrap();
    writeln!(o, "from ctypes import CFUNCTYPE, POINTER, Structure, c_char, c_char_p, c_size_t, c_uint8, c_uint16, c_uint32, c_uint64, c_int16, c_int32, c_void_p\n").unwrap();
    writeln!(o, "ABI_MAJOR = {}\n", super::ABI_MAJOR).unwrap();
    writeln!(o, "Text32 = c_char * 32  # NUL-padded text, UTF-8 except for names in the game's legacy code page\nText256 = c_char * 256\n").unwrap();
    py_constants(o, "STATUS_", &statuses());
    py_constants(o, "FRAME_", &frame_flags());
    sources(|table, index| {
        py_doc(o, "", &one(&source_doc(index)));
        writeln!(o, "SOURCE_{} = 1 << {index}", upper(table.name)).unwrap()
    });
    py_constants(o, "COMMAND_FLAG_", &command_flags().1);
    py_constants(o, "COMMAND_", &unit_kinds().1);
    py_constants(o, "COMMAND_", &game_kinds().1);
    py_constants(o, "RESULT_", &result_states().1);
    py_constants(o, "CHAT_", &chat_kinds().1);
    py_constants(o, "SHAPE_", &shape_kinds().1);
    py_constants(o, "COORDINATES_", &shape_coordinates().1);
    py_struct(o, &COMMAND_STRUCT);
    py_struct(o, &RESULT_STRUCT);
    py_struct(o, &CHAT_STRUCT);
    py_struct(o, &SHAPE_STRUCT);
    py_struct(o, &DRAW_LIST_STRUCT);
    for table in TABLES {
        writeln!(o, "\n").unwrap();
        py_doc(o, "", &table_doc(table));
        writeln!(o, "class {}(Structure):\n    _fields_ = [\n        (\"rows\", c_size_t),", table.type_name).unwrap();
        for column in table.columns {
            py_doc(o, "        ", &paragraphs(column.doc));
            writeln!(o, "        (\"{}\", POINTER({})),", column.name, py_type(column.kind)).unwrap();
        }
        writeln!(o, "    ]").unwrap();
    }
    writeln!(o, "\n").unwrap();
    py_doc(o, "", &paragraphs(&item_doc(LIB, "pub struct BwrFrame ")));
    writeln!(o, "class Frame(Structure):\n    _fields_ = [").unwrap();
    for (name, _, _, py) in HEADER {
        py_doc(o, "        ", &paragraphs(&frame_field_doc(name)));
        writeln!(o, "        (\"{name}\", {py}),").unwrap();
    }
    writeln!(o, "        # NULL when the frame does not carry the source.").unwrap();
    for table in TABLES {
        writeln!(o, "        (\"{}\", POINTER({})),", table.name, table.type_name).unwrap();
    }
    writeln!(o, "    ]\n\n\nSIGNATURES = {{").unwrap();
    for (name, _, _, result, args) in FUNCTIONS {
        py_doc(o, "    ", &paragraphs(&function_doc(name)));
        writeln!(o, "    \"{name}\": ({result}, {args}),").unwrap();
    }
    writeln!(o, "}}").unwrap();
    out
}

// --- Markdown reference ----------------------------------------------------------

fn md_constants(o: &mut String, prefix: &str, constants: &Constants) {
    writeln!(o, "| Name | Value | Meaning |\n|---|---|---|").unwrap();
    for (name, value, doc) in constants {
        writeln!(o, "| `{prefix}{name}` | {value} | {} |", md_cell(doc)).unwrap();
    }
    writeln!(o).unwrap();
}

fn md_paragraphs(o: &mut String, doc: &[String]) {
    for paragraph in doc {
        writeln!(o, "{paragraph}\n").unwrap();
    }
}

fn md_struct(o: &mut String, abi: &AbiStruct) {
    writeln!(o, "### `{}`\n", abi.rust).unwrap();
    md_paragraphs(o, &paragraphs(&struct_doc(abi)));
    writeln!(o, "| Field | Type | Meaning |\n|---|---|---|").unwrap();
    for (name, c, _, _) in abi.fields {
        writeln!(o, "| `{name}` | `{c}` | {} |", md_cell(&paragraphs(&field_doc(abi, name)))).unwrap();
    }
    writeln!(o).unwrap();
}

/// The C ABI reference, `docs/C-ABI.md`.
pub fn markdown() -> String {
    let mut out = String::new();
    let o = &mut out;
    writeln!(o, "<!-- Generated from crates/ffi and the engine table definitions. Do not edit.\n     Regenerate from the repository root: {REGENERATE} -->\n").unwrap();
    writeln!(o, "# BWRAPI C ABI {}.{} reference\n", super::ABI_MAJOR, super::ABI_MINOR).unwrap();
    writeln!(o, "`bwrapi.h` declares the C ABI of `bwrapi.dll`, one header for x64 and x86 games. The same definitions \
                 generate this page, the header, the Python declarations (`bwrapi/_native.py`) and the .NET declarations \
                 (`Native.g.cs`), so their comments match it.\n").unwrap();
    writeln!(o, "Contents: [Overview](#overview) · [Functions](#functions) · [Status codes](#status-codes) · \
                 [Frames](#frames) · [Sources](#sources) · [Commands](#commands) · [Chat](#chat) · [Drawing](#drawing)\n").unwrap();

    writeln!(o, "## Overview\n").unwrap();
    for (title, text) in OVERVIEW {
        writeln!(o, "**{title}.** {}\n", one(text).join(" ")).unwrap();
    }

    writeln!(o, "## Functions\n").unwrap();
    for (name, declaration, _, _, _) in FUNCTIONS {
        writeln!(o, "### `{name}`\n\n```c\n{declaration}\n```\n").unwrap();
        md_paragraphs(o, &paragraphs(&function_doc(name)));
    }

    writeln!(o, "## Status codes\n").unwrap();
    md_paragraphs(o, &paragraphs(&item_doc(LIB, "pub enum BwrStatus")));
    md_constants(o, "BWR_", &statuses());

    writeln!(o, "## Frames\n\n### `BwrFrame`\n").unwrap();
    md_paragraphs(o, &paragraphs(&item_doc(LIB, "pub struct BwrFrame ")));
    writeln!(o, "| Field | Type | Meaning |\n|---|---|---|").unwrap();
    for (name, c, _, _) in HEADER {
        writeln!(o, "| `{name}` | `{c}` | {} |", md_cell(&paragraphs(&frame_field_doc(name)))).unwrap();
    }
    writeln!(o, "| `players` .. `region_neighbors` | `const Bwr*Table*` | One table pointer per source, in source bit order; NULL when the frame does not carry the source. |\n").unwrap();
    writeln!(o, "### Frame flags\n").unwrap();
    md_constants(o, "BWR_FRAME_", &frame_flags());
    writeln!(o, "### Text\n\n`BwrText32` and `BwrText256` are NUL-padded text of 32 and 256 bytes, in their `text` member (.NET: `ToString()`; Python: a `c_char` array, read `.value`).\n").unwrap();
    writeln!(o, "## In-game modules\n").unwrap();
    writeln!(o, "`bwr_open_self`, `bwr_set_frame_callback`, `bwr_draw` and `bwr_capture_chat` work only in a DLL running inside the game. Load one with `bwrapi-inject <pid> <module>.dll <export>` or `bwr_inject`; the `bwrapi.dll` beside it is injected first when the game does not run one yet.\n").unwrap();
    writeln!(o, "`<export>` is a `WH_CALLWNDPROC` hook procedure, called on the game thread. It pins its own DLL so the module stays loaded, opens a session with `bwr_open_self`, subscribes, sets the frame callback once, and passes the message on with `CallNextHookEx`. A complete module: [frame_module.rs](../crates/ffi/examples/frame_module.rs).\n").unwrap();

    writeln!(o, "## Sources\n").unwrap();
    writeln!(o, "`bwr_subscribe` takes the `BWR_SOURCE_*` bits; source bit `i` is table `i` of `BwrFrame`.\n").unwrap();
    writeln!(o, "| Bit | Source | Table | Rows |\n|---|---|---|---|").unwrap();
    sources(|table, index| {
        writeln!(o, "| {index} | [`BWR_SOURCE_{}`](#bwr_source_{}) | `Bwr{}` at `frame->{}` | {} |", upper(table.name), table.name, table.type_name, table.name, md_cell(&table_doc(table))).unwrap()
    });
    writeln!(o).unwrap();
    sources(|table, index| {
        writeln!(o, "### BWR_SOURCE_{}\n", upper(table.name)).unwrap();
        md_paragraphs(o, &table_doc(table));
        md_paragraphs(o, &one(&source_doc(index)));
        writeln!(o, "| Column | Type | Meaning |\n|---|---|---|").unwrap();
        for column in table.columns {
            writeln!(o, "| `{}` | `{}` | {} |", column.name, c_type(column.kind), md_cell(&paragraphs(column.doc))).unwrap();
        }
        writeln!(o).unwrap();
    });

    writeln!(o, "## Commands\n").unwrap();
    md_struct(o, &COMMAND_STRUCT);
    let (intro, flags) = command_flags();
    writeln!(o, "### Command flags\n").unwrap();
    md_paragraphs(o, &intro);
    md_constants(o, "BWR_COMMAND_FLAG_", &flags);
    let (intro, kinds) = unit_kinds();
    writeln!(o, "### Unit commands\n").unwrap();
    md_paragraphs(o, &intro);
    md_constants(o, "BWR_COMMAND_", &kinds);
    let (intro, kinds) = game_kinds();
    writeln!(o, "### Game commands\n").unwrap();
    md_paragraphs(o, &intro);
    md_constants(o, "BWR_COMMAND_", &kinds);
    md_struct(o, &RESULT_STRUCT);
    let (intro, states) = result_states();
    writeln!(o, "### Result states\n").unwrap();
    md_paragraphs(o, &intro);
    md_constants(o, "BWR_RESULT_", &states);

    writeln!(o, "## Chat\n").unwrap();
    md_struct(o, &CHAT_STRUCT);
    let (intro, kinds) = chat_kinds();
    writeln!(o, "### Chat kinds\n").unwrap();
    md_paragraphs(o, &intro);
    md_constants(o, "BWR_CHAT_", &kinds);

    writeln!(o, "## Drawing\n").unwrap();
    md_struct(o, &SHAPE_STRUCT);
    let (intro, kinds) = shape_kinds();
    writeln!(o, "### Shape kinds\n").unwrap();
    md_paragraphs(o, &intro);
    md_constants(o, "BWR_SHAPE_", &kinds);
    let (intro, coordinates) = shape_coordinates();
    writeln!(o, "### Coordinates\n").unwrap();
    md_paragraphs(o, &intro);
    md_constants(o, "BWR_COORDINATES_", &coordinates);
    md_struct(o, &DRAW_LIST_STRUCT);
    while out.ends_with("\n\n") {
        out.pop();
    }
    out
}

fn pascal(name: &str) -> String {
    name.split('_')
        .map(|part| {
            let lower = part.to_ascii_lowercase();
            let mut chars = lower.chars();
            chars.next().map_or(String::new(), |first| first.to_ascii_uppercase().to_string() + chars.as_str())
        })
        .collect()
}
