//! Shapes drawn into the game's own screen. The in-game server hands each
//! frame's shape list to [`set`]; the hook table's renderer entry puts a copy
//! of the renderer's vtable on the renderer whose `render` adds the latest
//! list to the game's draw queue, in the vertex and command layout the
//! game's own draw code writes, and then lets the game draw the queue. A list
//! stays until the next one replaces it, so every screen update between two
//! game frames draws it.

#![cfg_attr(all(not(windows), not(test)), allow(dead_code))]

use std::sync::{Mutex, MutexGuard};

/// One shape of a draw list (`BwrShape` of the ABI): BWAPI's shape fields,
/// the color as RGBA bytes in memory order, and for text its UTF-8 bytes in
/// the list's text and its size (0 small .. 3 huge).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DrawShape {
    pub kind: u8,
    pub coordinates: u8,
    pub solid: u8,
    pub text_size: u8,
    pub x1: i32,
    pub y1: i32,
    pub x2: i32,
    pub y2: i32,
    pub extra1: i32,
    pub extra2: i32,
    pub rgba: u32,
    pub text_offset: u32,
    pub text_len: u32,
}

/// [`DrawShape::kind`] values, BWAPI's shape type numbers.
pub mod shape_kind {
    pub const TEXT: u8 = 1;
    pub const BOX: u8 = 2;
    pub const TRIANGLE: u8 = 3;
    pub const CIRCLE: u8 = 4;
    pub const ELLIPSE: u8 = 5;
    pub const DOT: u8 = 6;
    pub const LINE: u8 = 7;
}

/// [`DrawShape::coordinates`] values, BWAPI's coordinate type numbers: game
/// screen pixels, map pixels, or pixels from the cursor.
pub mod coordinates {
    pub const SCREEN: u8 = 1;
    pub const MAP: u8 = 2;
    pub const MOUSE: u8 = 3;
}

/// Number of text color codes a list maps to RGBA.
pub const TEXT_COLORS: usize = 32;

/// A draw list this DLL owns.
struct DrawList {
    shapes: Vec<DrawShape>,
    text: Vec<u8>,
    text_colors: [u32; TEXT_COLORS],
}

impl DrawList {
    const fn new() -> Self {
        Self { shapes: Vec::new(), text: Vec::new(), text_colors: [0; TEXT_COLORS] }
    }
}

/// The list the renderer draws and the one the next [`set`] fills, which
/// keeps its capacity.
struct Lists {
    front: DrawList,
    back: DrawList,
}

static LISTS: Mutex<Lists> = Mutex::new(Lists { front: DrawList::new(), back: DrawList::new() });

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Why a draw list was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DrawError {
    /// No frame hook runs, so nothing would draw the list.
    NoFrameHook,
    /// A text shape names bytes outside the list's text.
    TextOutside,
}

impl DrawError {
    pub fn message(self) -> &'static str {
        match self {
            Self::NoFrameHook => "the frame hook is not installed",
            Self::TextOutside => "a text shape names bytes outside the text",
        }
    }
}

/// Replaces the drawn list with a copy of `shapes`, whose text shapes name
/// UTF-8 bytes of `text`, and the RGBA of each text color code; empty
/// `shapes` draw nothing. Fails, leaving the drawn list, when a text shape
/// names bytes outside `text`.
pub(crate) fn set(shapes: &[DrawShape], text: &[u8], text_colors: &[u32; TEXT_COLORS]) -> Result<(), DrawError> {
    let inside = |shape: &DrawShape| {
        shape.kind != shape_kind::TEXT
            || (shape.text_offset as usize).checked_add(shape.text_len as usize).is_some_and(|end| end <= text.len())
    };
    if !shapes.iter().all(inside) {
        return Err(DrawError::TextOutside);
    }
    let mut lists = lock(&LISTS);
    let lists = &mut *lists;
    lists.back.shapes.clear();
    lists.back.shapes.extend_from_slice(shapes);
    lists.back.text.clear();
    lists.back.text.extend_from_slice(text);
    lists.back.text_colors = *text_colors;
    std::mem::swap(&mut lists.front, &mut lists.back);
    Ok(())
}

/// One vertex of the game's colored triangle lists.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Vertex {
    x: f32,
    y: f32,
    u: f32,
    v: f32,
    rgba: u32,
}

/// What a run of quads is drawn with: the plain color texture, or a page of
/// the glyph atlas in the outline of a font.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ink {
    Color,
    Glyphs { font: usize, page: u32 },
}

/// Consecutive quads of one ink.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Run {
    ink: Ink,
    first: usize,
    quads: usize,
}

/// Where the screen is when it is drawn: the game screen size in game
/// pixels, the map pixel at its top-left corner and the cursor.
#[derive(Clone, Copy, Debug)]
struct View {
    width: f64,
    height: f64,
    map: (i32, i32),
    mouse: (i32, i32),
}

/// A cached glyph of the game's atlas: its page, the bitmap's top-left offset
/// from the pen and its advance in game pixels, its size and position in
/// atlas pixels ([`Glyphs::pixel_scale`] of them to a game pixel).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Glyph {
    page: u32,
    bearing: [f32; 2],
    size: [f32; 2],
    atlas: [f32; 2],
    advance: f32,
}

/// The game's glyphs by font (BWAPI text size).
trait Glyphs {
    fn glyph(&mut self, font: usize, character: char) -> Option<Glyph>;
    fn line_height(&mut self, font: usize) -> f32;
    fn atlas_size(&self) -> f32;
    /// Atlas pixels per game pixel: the game renders glyphs at the scale the
    /// screen is drawn at.
    fn pixel_scale(&self) -> f32;
}

/// Text control codes (BWAPI's Text enum).
mod text_code {
    pub const PREVIOUS: u32 = 1;
    pub const DEFAULT: usize = 2;
    pub const TAB: u32 = 9;
    pub const NEWLINE: u32 = 10;
    pub const INVISIBLE: u32 = 11;
    pub const ALIGN_RIGHT: u32 = 18;
    pub const ALIGN_CENTER: u32 = 19;
    pub const INVISIBLE2: u32 = 20;

    pub fn is_color(code: u32) -> bool {
        matches!(code, 2..=8 | 14..=17 | 21..=31)
    }
}

/// Most quads one command draws: its u16 indices count from its first vertex.
const MAX_COMMAND_QUADS: usize = (u16::MAX as usize + 1) / 4;

/// The quads of a draw list in game screen coordinates (0..1, y up), in
/// list order, grouped in runs of one ink.
#[derive(Default)]
struct Mesh {
    quads: Vec<[Vertex; 4]>,
    runs: Vec<Run>,
}

type Point = (f64, f64);

impl Mesh {
    fn clear(&mut self) {
        self.quads.clear();
        self.runs.clear();
    }

    fn build(&mut self, list: &DrawList, view: &View, glyphs: &mut impl Glyphs) {
        self.clear();
        for shape in &list.shapes {
            let origin = match shape.coordinates {
                coordinates::SCREEN => (0, 0),
                coordinates::MAP => (-view.map.0, -view.map.1),
                coordinates::MOUSE => view.mouse,
                _ => continue,
            };
            let mut builder = Builder { mesh: self, view, rgba: shape.rgba };
            let at = |x: i32, y: i32| (f64::from(x) + f64::from(origin.0), f64::from(y) + f64::from(origin.1));
            let (x1, y1) = at(shape.x1, shape.y1);
            let (x2, y2) = at(shape.x2, shape.y2);
            let solid = shape.solid != 0;
            match shape.kind {
                shape_kind::TEXT => {
                    let end = shape.text_offset as usize + shape.text_len as usize;
                    let text = &list.text[shape.text_offset as usize..end];
                    builder.text(usize::from(shape.text_size), (x1, y1), text, &list.text_colors, glyphs);
                }
                shape_kind::BOX => builder.boxed((x1, y1), (x2, y2), solid),
                shape_kind::TRIANGLE => builder.triangle([(x1, y1), (x2, y2), at(shape.extra1, shape.extra2)], solid),
                shape_kind::CIRCLE => builder.ellipse((x1, y1), shape.extra1, shape.extra1, solid),
                shape_kind::ELLIPSE => builder.ellipse((x1, y1), shape.extra1, shape.extra2, solid),
                shape_kind::DOT => builder.rect(x1, y1, 1.0, 1.0),
                shape_kind::LINE => builder.line((x1, y1), (x2, y2)),
                _ => {}
            }
        }
    }

    fn push(&mut self, ink: Ink, quad: [Vertex; 4]) {
        match self.runs.last_mut() {
            Some(run) if run.ink == ink && run.quads < MAX_COMMAND_QUADS => run.quads += 1,
            _ => self.runs.push(Run { ink, first: self.quads.len(), quads: 1 }),
        }
        self.quads.push(quad);
    }
}

/// Adds one shape's quads. Coordinates are game pixels; pixel (x, y) covers
/// [x, x + 1) x [y, y + 1), so a 1-pixel line runs through pixel centers.
struct Builder<'a> {
    mesh: &'a mut Mesh,
    view: &'a View,
    rgba: u32,
}

impl Builder<'_> {
    /// One pixel beyond the screen on every side: the farthest a shape is
    /// kept or clipped to, so huge coordinates keep their precision.
    fn bounds(&self) -> (f64, f64, f64, f64) {
        (-1.0, -1.0, self.view.width + 1.0, self.view.height + 1.0)
    }

    fn visible(&self, points: &[Point], margin: f64) -> bool {
        let (left, top, right, bottom) = self.bounds();
        let (mut low_x, mut low_y, mut high_x, mut high_y) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for &(x, y) in points {
            low_x = low_x.min(x);
            low_y = low_y.min(y);
            high_x = high_x.max(x);
            high_y = high_y.max(y);
        }
        high_x + margin >= left && low_x - margin <= right && high_y + margin >= top && low_y - margin <= bottom
    }

    fn vertex(&self, (x, y): Point, u: f32, v: f32) -> Vertex {
        Vertex { x: (x / self.view.width) as f32, y: (1.0 - y / self.view.height) as f32, u, v, rgba: self.rgba }
    }

    fn quad(&mut self, points: [Point; 4]) {
        let quad = points.map(|point| self.vertex(point, 0.0, 0.0));
        self.mesh.push(Ink::Color, quad);
    }

    /// Fills [x, x + w) x [y, y + h), clipped to the bounds.
    fn rect(&mut self, x: f64, y: f64, w: f64, h: f64) {
        let (left, top, right, bottom) = self.bounds();
        let (x0, y0) = (x.max(left), y.max(top));
        let (x1, y1) = ((x + w).min(right), (y + h).min(bottom));
        if x0 < x1 && y0 < y1 {
            self.quad([(x0, y0), (x1, y0), (x1, y1), (x0, y1)]);
        }
    }

    /// BWAPI's box: `a` and `b` are opposite corners and the width is their
    /// distance; a hollow box is its 1-pixel border.
    fn boxed(&mut self, a: Point, b: Point, solid: bool) {
        let (left, top) = (a.0.min(b.0), a.1.min(b.1));
        let (width, height) = ((b.0 - a.0).abs(), (b.1 - a.1).abs());
        if width == 0.0 || height == 0.0 {
            return;
        }
        if solid {
            self.rect(left, top, width, height);
            return;
        }
        self.rect(left, top, 1.0, height);
        self.rect(left, top, width, 1.0);
        self.rect(left + width - 1.0, top, 1.0, height);
        self.rect(left, top + height - 1.0, width, 1.0);
    }

    /// A 1-pixel line between the pixels at `a` and `b`, both included,
    /// clipped to the bounds.
    fn line(&mut self, a: Point, b: Point) {
        let (a, b) = ((a.0 + 0.5, a.1 + 0.5), (b.0 + 0.5, b.1 + 0.5));
        if let Some((a, b)) = self.clip(a, b) {
            self.stroke(a, b);
        }
    }

    /// Liang–Barsky: the part of segment `a`-`b` inside the bounds.
    fn clip(&self, a: Point, b: Point) -> Option<(Point, Point)> {
        let (left, top, right, bottom) = self.bounds();
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let (mut t0, mut t1) = (0.0f64, 1.0f64);
        for (p, q) in [(-dx, a.0 - left), (dx, right - a.0), (-dy, a.1 - top), (dy, bottom - a.1)] {
            if p == 0.0 {
                if q < 0.0 {
                    return None;
                }
                continue;
            }
            let r = q / p;
            if p < 0.0 {
                t0 = t0.max(r);
            } else {
                t1 = t1.min(r);
            }
            if t0 > t1 {
                return None;
            }
        }
        Some(((a.0 + t0 * dx, a.1 + t0 * dy), (a.0 + t1 * dx, a.1 + t1 * dy)))
    }

    /// A 1-pixel wide band from pixel center `a` to `b`, half a pixel past
    /// both, so the end pixels are covered.
    fn stroke(&mut self, a: Point, b: Point) {
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let length = (dx * dx + dy * dy).sqrt();
        if length < 1e-9 {
            self.rect(a.0 - 0.5, a.1 - 0.5, 1.0, 1.0);
            return;
        }
        let (ex, ey) = (dx / length * 0.5, dy / length * 0.5);
        let (nx, ny) = (-ey, ex);
        self.quad([
            (a.0 - ex + nx, a.1 - ey + ny),
            (b.0 + ex + nx, b.1 + ey + ny),
            (b.0 + ex - nx, b.1 + ey - ny),
            (a.0 - ex - nx, a.1 - ey - ny),
        ]);
    }

    fn triangle(&mut self, corners: [Point; 3], solid: bool) {
        if solid {
            let [a, b, c] = corners.map(|(x, y)| (x + 0.5, y + 0.5));
            if self.visible(&[a, b, c], 0.0) {
                self.quad([a, b, c, c]);
            }
            return;
        }
        let [a, b, c] = corners;
        self.line(a, b);
        self.line(a, c);
        self.line(b, c);
    }

    /// An ellipse of radii `a` and `b` around the pixel at `center`; a circle
    /// has `a == b`. A zero radius is a 1-pixel line.
    fn ellipse(&mut self, center: Point, a: i32, b: i32, solid: bool) {
        if a < 0 || b < 0 {
            return;
        }
        let (ra, rb) = (f64::from(a), f64::from(b));
        if !self.visible(&[center], ra.max(rb) + 1.0) {
            return;
        }
        if a == 0 || b == 0 {
            self.rect(center.0 - ra, center.1 - rb, 2.0 * ra + 1.0, 2.0 * rb + 1.0);
            return;
        }
        let c = (center.0 + 0.5, center.1 + 0.5);
        let segments = segments(a.max(b));
        // A filled ellipse reaches the outer edge of its outline pixels.
        let (ra, rb) = if solid { (ra + 0.5, rb + 0.5) } else { (ra, rb) };
        let point = |index: usize| {
            let angle = std::f64::consts::TAU * index as f64 / segments as f64;
            (c.0 + ra * angle.cos(), c.1 + rb * angle.sin())
        };
        let mut previous = point(0);
        for index in 1..=segments {
            let next = point(index);
            if solid {
                self.quad([c, previous, next, next]);
            } else {
                self.stroke(previous, next);
            }
            previous = next;
        }
    }

    /// BWAPI text: `origin` is the top-left of the first line. Control codes
    /// pick colors (from `colors`), the previous color, invisible text, right
    /// or center alignment across the screen, tab and newline.
    fn text(&mut self, font: usize, origin: Point, text: &[u8], colors: &[u32; TEXT_COLORS], glyphs: &mut impl Glyphs) {
        if font > 3 {
            return;
        }
        let text = String::from_utf8_lossy(text);
        let line_height = f64::from(glyphs.line_height(font));
        let space = glyphs.glyph(font, ' ').map_or(0.0, |glyph| f64::from(glyph.advance));
        let atlas = glyphs.atlas_size();
        let scale = f64::from(glyphs.pixel_scale());
        let mut color = Some(text_code::DEFAULT);
        let mut previous = color;
        let (mut pen_x, mut pen_y) = origin;
        for character in text.chars() {
            let code = character as u32;
            match code {
                text_code::PREVIOUS => color = previous,
                text_code::TAB => pen_x += space,
                text_code::NEWLINE => {
                    pen_x = origin.0;
                    pen_y += line_height;
                }
                text_code::INVISIBLE | text_code::INVISIBLE2 => color = None,
                text_code::ALIGN_RIGHT => pen_x += self.view.width - widest_line(&text, font, space, glyphs) - origin.0,
                text_code::ALIGN_CENTER => {
                    pen_x += (self.view.width - widest_line(&text, font, space, glyphs)) / 2.0 - origin.0;
                }
                code if text_code::is_color(code) => {
                    previous = color;
                    color = Some(code as usize);
                }
                0..=31 => {}
                _ => {
                    let Some(glyph) = glyphs.glyph(font, character) else { continue };
                    if let Some(color) = color {
                        self.rgba = colors[color];
                        self.glyph(font, &glyph, (pen_x, pen_y), atlas, scale);
                    }
                    pen_x += f64::from(glyph.advance);
                }
            }
        }
    }

    fn glyph(&mut self, font: usize, glyph: &Glyph, pen: Point, atlas: f32, scale: f64) {
        let (w, h) = (f64::from(glyph.size[0]) / scale, f64::from(glyph.size[1]) / scale);
        if w <= 0.0 || h <= 0.0 {
            return;
        }
        let x0 = pen.0 + f64::from(glyph.bearing[0]);
        let y0 = pen.1 + f64::from(glyph.bearing[1]);
        let (x1, y1) = (x0 + w, y0 + h);
        if !self.visible(&[(x0, y0), (x1, y1)], 0.0) {
            return;
        }
        let (u0, v0) = (glyph.atlas[0] / atlas, glyph.atlas[1] / atlas);
        let (u1, v1) = ((glyph.atlas[0] + glyph.size[0]) / atlas, (glyph.atlas[1] + glyph.size[1]) / atlas);
        let quad = [
            self.vertex((x0, y0), u0, v0),
            self.vertex((x1, y0), u1, v0),
            self.vertex((x1, y1), u1, v1),
            self.vertex((x0, y1), u0, v1),
        ];
        self.mesh.push(Ink::Glyphs { font, page: glyph.page }, quad);
    }
}

/// Outline points of an ellipse whose larger radius is `radius`: more for
/// larger ones, at most a pixel off the curve.
fn segments(radius: i32) -> usize {
    (8 + radius.max(0) as usize / 2).min(64)
}

/// Width of the widest line of `text` in game pixels, which alignment places
/// across the screen.
fn widest_line(text: &str, font: usize, space: f64, glyphs: &mut impl Glyphs) -> f64 {
    let (mut widest, mut line) = (0.0f64, 0.0f64);
    for character in text.chars() {
        match character as u32 {
            text_code::TAB => line += space,
            text_code::NEWLINE => line = 0.0,
            0..=31 => {}
            _ => line += glyphs.glyph(font, character).map_or(0.0, |glyph| f64::from(glyph.advance)),
        }
        widest = widest.max(line);
    }
    widest
}

#[cfg(windows)]
mod game {
    use std::cell::UnsafeCell;
    use std::panic::{self, AssertUnwindSafe};
    use std::sync::atomic::{AtomicUsize, Ordering::Acquire, Ordering::Release};
    use std::sync::Mutex;

    use bwrapi_layout::{read_pointer, FontLayout, Layout, RenderLayout};
    use bwrapi_memory::{InProcessMemory, ProcessMemory};

    use super::{lock, Glyph, Glyphs, Ink, Mesh, Vertex, View, LISTS};
    use crate::hook;

    /// Most vtable entries the copy holds.
    const VTABLE_CAPACITY: usize = 64;

    /// The renderer's vtable copy: the type information pointer, then the
    /// entries, with `render` replaced. Written only while the slot does not
    /// point at it yet.
    struct VtableCopy(UnsafeCell<[u64; VTABLE_CAPACITY]>);
    unsafe impl Sync for VtableCopy {}

    static VTABLE: VtableCopy = VtableCopy(UnsafeCell::new([0; VTABLE_CAPACITY]));
    /// The game vtable the copy was made from, and its `render`.
    static SOURCE: AtomicUsize = AtomicUsize::new(0);
    static RENDER: AtomicUsize = AtomicUsize::new(0);
    /// The renderer the color texture was made for, and the texture.
    static WHITE: Mutex<(usize, usize)> = Mutex::new((0, 0));
    /// The quads of the last screen update; reused, so a list costs no
    /// allocation once the buffers fit it.
    static MESH: Mutex<Option<Mesh>> = Mutex::new(None);

    unsafe fn read(address: usize, width: usize) -> usize {
        match width {
            4 => std::ptr::read_unaligned(address as *const u32) as usize,
            _ => std::ptr::read_unaligned(address as *const u64) as usize,
        }
    }

    unsafe fn write(address: usize, value: usize, width: usize) {
        match width {
            4 => std::ptr::write_unaligned(address as *mut u32, value as u32),
            _ => std::ptr::write_unaligned(address as *mut u64, value as u64),
        }
    }

    unsafe fn read_u32(address: usize) -> u32 {
        std::ptr::read_unaligned(address as *const u32)
    }

    unsafe fn read_f32(address: usize) -> f32 {
        std::ptr::read_unaligned(address as *const f32)
    }

    /// The renderer's vtable pointer: the first field of the renderer
    /// object. Skipped while the game has no renderer.
    pub(crate) fn renderer_slot(memory: &InProcessMemory, layout: &Layout) -> Option<usize> {
        let render = layout.render.as_ref()?;
        let width = layout.pointer_bytes;
        let at = memory.main_module().range_address(render.renderer, width)?;
        let mut raw = [0; 8];
        memory.read_into(at, &mut raw[..width]).ok()?;
        read_pointer(&raw, 0, width).filter(|renderer| *renderer != 0)
    }

    /// The vtable pointer the renderer must hold: the copy, made again from
    /// the game vtable `current` points at when it is not the one the copy
    /// was made from (another renderer class).
    pub(crate) fn renderer_vtable(current: usize) -> usize {
        let Some((memory, layout)) = hook::game_state() else { return current };
        let Some(render) = layout.render.as_ref() else { return current };
        let width = layout.pointer_bytes;
        let copy = VTABLE.0.get() as usize;
        if current == copy + width {
            return current;
        }
        if current != SOURCE.load(Acquire) {
            let entries = render.vtable_entries.min(VTABLE_CAPACITY - 1);
            let mut raw = [0u8; VTABLE_CAPACITY * 8];
            let bytes = (entries + 1) * width;
            let Some(start) = current.checked_sub(width) else { return current };
            if memory.read_into(start, &mut raw[..bytes]).is_err() {
                return current;
            }
            // The slot does not point at the copy, so nothing reads it while
            // it is written.
            unsafe { std::ptr::copy_nonoverlapping(raw.as_ptr(), copy as *mut u8, bytes) };
            let entry = copy + (render.render_entry + 1) * width;
            RENDER.store(unsafe { read(entry, width) }, Release);
            unsafe { write(entry, render_hook as *const () as usize, width) };
            SOURCE.store(current, Release);
        }
        copy + width
    }

    member_fn! {
        /// The renderer's `render` on the copy: adds the drawn list to the queue
        /// the game is about to draw, then draws it with the game's `render`.
        unsafe fn render_hook(renderer: usize, queue: usize, width: u32, height: u32) -> usize {
            if let Some((memory, layout)) = hook::game_state() {
                let start = hook::qpc();
                let _ = panic::catch_unwind(AssertUnwindSafe(|| draw(memory, layout, renderer, queue, width, height)));
                hook::record_render(hook::qpc().wrapping_sub(start));
            }
            let original: member_fn!(type (usize, usize, u32, u32) -> usize) =
                std::mem::transmute(RENDER.load(Acquire));
            original(renderer, queue, width, height)
        }
    }

    fn draw(memory: &InProcessMemory, layout: &Layout, renderer: usize, queue: usize, target_width: u32, target_height: u32) {
        let Some(render) = layout.render.as_ref() else { return };
        let module = memory.main_module();
        let Some(view) = view(memory, layout, render) else { return };
        let mut mesh = lock(&MESH);
        let mesh = mesh.get_or_insert_with(Mesh::default);
        {
            let lists = lock(&LISTS);
            if lists.front.shapes.is_empty() {
                return;
            }
            let scale = target_height as f32 / view.height as f32;
            let mut glyphs =
                GameGlyphs { module: module.base_address, layout, fonts: layout.fonts.as_ref(), heights: [None; 4], scale };
            mesh.build(&lists.front, &view, &mut glyphs);
        }
        if mesh.quads.is_empty() {
            return;
        }
        let target = [1.0 / target_width.max(1) as f32, 1.0 / target_height.max(1) as f32];
        unsafe { append(mesh, module.base_address, layout, render, renderer, queue, target) };
    }

    /// The game screen as the game draws it now.
    fn view(memory: &InProcessMemory, layout: &Layout, render: &RenderLayout) -> Option<View> {
        let module = memory.main_module();
        let u32_at = |address: usize| -> Option<u32> {
            let mut raw = [0; 4];
            memory.read_into(address, &mut raw).ok()?;
            Some(u32::from_le_bytes(raw))
        };
        let width = u32_at(module.range_address(layout.scalars.game_screen_offset, 4)?)?;
        let screen = module.range_address(layout.scalars.screen_position_offset, 8)?;
        let map = (u32_at(screen)? as i32, u32_at(screen + 4)? as i32);
        let pointer_at = |address: usize| -> Option<usize> {
            let mut raw = [0; 8];
            memory.read_into(address, &mut raw[..layout.pointer_bytes]).ok()?;
            read_pointer(&raw, 0, layout.pointer_bytes).filter(|pointer| *pointer != 0)
        };
        let mouse = layout
            .mouse
            .as_ref()
            .and_then(|mouse| {
                let base = pointer_at(pointer_at(module.range_address(mouse.pointer, layout.pointer_bytes)?)?)?;
                Some((u32_at(base + mouse.offset)? as i32, u32_at(base + mouse.offset + 4)? as i32))
            })
            .unwrap_or((0, 0));
        (width != 0).then_some(View { width: f64::from(width), height: f64::from(render.screen_height), map, mouse })
    }

    /// The game's glyph cache; a character it lacks is rendered into the
    /// atlas by the game's own function.
    struct GameGlyphs<'a> {
        module: usize,
        layout: &'a Layout,
        fonts: Option<&'a FontLayout>,
        heights: [Option<f32>; 4],
        /// Render target pixels per game pixel.
        scale: f32,
    }

    impl GameGlyphs<'_> {
        fn font(&self, fonts: &FontLayout, font: usize) -> Option<usize> {
            if font >= fonts.count {
                return None;
            }
            let at = self.module + fonts.table + font * self.layout.pointer_bytes;
            Some(unsafe { read(at, self.layout.pointer_bytes) }).filter(|font| *font != 0)
        }

        unsafe fn find(&self, fonts: &FontLayout, set: usize, character: u32) -> Option<usize> {
            let width = self.layout.pointer_bytes;
            let mut node = read(set + fonts.glyph_tree, width);
            // A balanced tree of every Unicode character is shallower.
            for _ in 0..64 {
                if node == 0 {
                    return None;
                }
                let key = read_u32(node + fonts.node_key);
                if key == character {
                    return Some(node + fonts.node_glyph);
                }
                node = read(node + if character < key { fonts.node_left } else { fonts.node_right }, width);
            }
            None
        }
    }

    impl Glyphs for GameGlyphs<'_> {
        fn glyph(&mut self, font: usize, character: char) -> Option<Glyph> {
            let fonts = self.fonts?;
            let set = unsafe { read(self.font(fonts, font)? + fonts.glyph_set, self.layout.pointer_bytes) };
            if set == 0 {
                return None;
            }
            let code = character as u32;
            let record = match unsafe { self.find(fonts, set, code) } {
                Some(record) => record,
                None => {
                    let cache: member_fn!(type (usize, u32) -> u8) =
                        unsafe { std::mem::transmute(self.module + fonts.cache_glyph) };
                    unsafe { cache(set, code) };
                    unsafe { self.find(fonts, set, code) }?
                }
            };
            let pair = |offset: usize| unsafe { [read_f32(record + offset), read_f32(record + offset + 4)] };
            Some(Glyph {
                page: unsafe { read_u32(record + fonts.glyph_page) },
                bearing: pair(fonts.glyph_bearing),
                size: pair(fonts.glyph_size),
                atlas: pair(fonts.glyph_atlas),
                advance: unsafe { read_f32(record + fonts.glyph_advance) },
            })
        }

        fn line_height(&mut self, font: usize) -> f32 {
            if let Some(height) = self.heights.get(font).copied().flatten() {
                return height;
            }
            let height = self
                .fonts
                .and_then(|fonts| {
                    let glyphs = unsafe { read(self.font(fonts, font)? + fonts.glyphs, self.layout.pointer_bytes) };
                    (glyphs != 0).then(|| f32::from(unsafe { *((glyphs + fonts.height_byte) as *const u8) }))
                })
                .unwrap_or(0.0);
            if let Some(slot) = self.heights.get_mut(font) {
                *slot = Some(height);
            }
            height
        }

        fn atlas_size(&self) -> f32 {
            self.fonts.map_or(1.0, |fonts| fonts.atlas_size as f32)
        }
        fn pixel_scale(&self) -> f32 {
            self.scale
        }
    }

    /// The 1x1 white texture color quads multiply their vertex color with,
    /// made once per renderer through its vtable.
    unsafe fn white_texture(render: &RenderLayout, renderer: usize, width: usize) -> usize {
        let mut white = lock(&WHITE);
        if white.0 == renderer && white.1 != 0 {
            return white.1;
        }
        let entry = read(SOURCE.load(Acquire) + render.create_texture_entry * width, width);
        let create: member_fn!(type (usize, u32, *const u8, usize, u32, u32, u8, u32) -> usize) =
            std::mem::transmute(entry);
        let pixel = [0xffu8; 4];
        let texture = create(renderer, 0, pixel.as_ptr(), pixel.len(), 1, 1, 0, 0);
        *white = (renderer, texture);
        texture
    }

    /// Appends `mesh` to the draw queue the way the game's own draw code
    /// does: vertices and indices at the end of the game's buffers, one
    /// command per run on the sort layer, in list order. What no longer fits
    /// the buffers or the queue is left out.
    unsafe fn append(
        mesh: &Mesh,
        module: usize,
        layout: &Layout,
        render: &RenderLayout,
        renderer: usize,
        queue: usize,
        target: [f32; 2],
    ) {
        let width = layout.pointer_bytes;
        let fields = &render.command;
        let count_at = queue + render.queue_count;
        let mut count = std::ptr::read_unaligned(count_at as *const u16) as usize;
        let sort = queue + render.queue_sort;
        let (sort_data, mut sort_len, sort_cap) = (read(sort, width), read(sort + width, width), read(sort + 2 * width, width));
        let vertices = module + render.vertices;
        let vertex_capacity = read(vertices, width) * 4;
        let mut vertex_used = read(vertices + width, width);
        let vertex_data = read(vertices + 2 * width, width);
        let indices = vertices + render.index_buffer;
        let index_capacity = read(indices, width) * 2;
        let mut index_used = read(indices + width, width);
        let index_data = read(indices + 2 * width, width);
        let mut padding = 0;
        if sort_data == 0 || vertex_data == 0 || index_data == 0 {
            return;
        }
        // The game's own commands of this update tell the HD state.
        let is_hd = match count {
            0 => 0,
            last => read_u32(queue + (last - 1) * render.command_bytes + fields.is_hd),
        };
        let stride = std::mem::size_of::<Vertex>();
        for run in &mesh.runs {
            let start = vertex_used.div_ceil(stride) * stride;
            let vertex_end = start + run.quads * 4 * stride;
            let index_end = index_used + run.quads * 12;
            if count >= render.queue_capacity
                || sort_len >= sort_cap
                || vertex_end > vertex_capacity
                || index_end > index_capacity
            {
                break;
            }
            let (texture, second, shader, constants) = match run.ink {
                Ink::Color => (white_texture(render, renderer, width), 0, render.color_shader, color_constants(target)),
                Ink::Glyphs { font, page } => {
                    let Some(fonts) = layout.fonts.as_ref() else { break };
                    let page_texture: member_fn!(type (usize, u32) -> usize) =
                        std::mem::transmute(module + fonts.page_texture);
                    let holder = page_texture(module + fonts.pages, page);
                    let texture = if holder == 0 { 0 } else { read(holder + fonts.holder_texture, width) };
                    (texture, read(module + render.blank_texture, width), render.text_shader, glyph_constants(module, layout, fonts, font))
                }
            };
            if texture == 0 {
                continue;
            }
            std::ptr::copy_nonoverlapping(
                mesh.quads[run.first..].as_ptr().cast::<u8>(),
                (vertex_data + start) as *mut u8,
                run.quads * 4 * stride,
            );
            let index = (index_data + index_used) as *mut u16;
            for quad in 0..run.quads {
                let base = (quad * 4) as u16;
                for (offset, corner) in [0u16, 1, 2, 0, 2, 3].into_iter().enumerate() {
                    index.add(quad * 6 + offset).write_unaligned(base + corner);
                }
            }
            let command = queue + count * render.command_bytes;
            std::ptr::write_bytes(command as *mut u8, 0, render.command_bytes);
            let put_u32 = |offset: usize, value: u32| std::ptr::write_unaligned((command + offset) as *mut u32, value);
            put_u32(fields.render_target, 1);
            put_u32(fields.is_hd, is_hd);
            write(command + fields.textures, texture, width);
            write(command + fields.textures + width, second, width);
            put_u32(fields.draw_mode, render.triangle_mode);
            put_u32(fields.shader, shader);
            write(command + fields.vertex_offset, start, width);
            write(command + fields.index_offset, index_used, width);
            put_u32(fields.vertex_count, (run.quads * 4) as u32);
            put_u32(fields.used_count, (run.quads * 4) as u32);
            std::ptr::write_unaligned((command + fields.order_mark) as *mut u16, 0xffff);
            for (index, value) in constants.into_iter().enumerate() {
                std::ptr::write_unaligned((command + fields.constants + index * 4) as *mut f32, value);
            }
            let entry = sort_data + sort_len * render.sort_entry_bytes;
            std::ptr::write_bytes(entry as *mut u8, 0, render.sort_entry_bytes);
            std::ptr::write_unaligned(entry as *mut u16, render.layer);
            std::ptr::write_unaligned((entry + 2) as *mut u16, sort_len as u16);
            write(entry + render.sort_entry_bytes - width, command, width);
            padding += start - vertex_used;
            vertex_used = vertex_end;
            index_used = index_end;
            sort_len += 1;
            count += 1;
        }
        write(vertices + width, vertex_used, width);
        write(vertices + render.vertex_padding, read(vertices + render.vertex_padding, width) + padding, width);
        write(indices + width, index_used, width);
        write(sort + width, sort_len, width);
        std::ptr::write_unaligned(count_at as *mut u16, count as u16);
    }

    /// Shader constants of a color run: the color factor 1, then the game's
    /// constant pattern and the reciprocal render target size.
    fn color_constants(target: [f32; 2]) -> [f32; 20] {
        let mut constants = [0.0; 20];
        constants[..4].fill(1.0);
        constants[8..12].fill(1.0);
        constants[14] = target[0];
        constants[15] = target[1];
        constants
    }

    /// Shader constants of a glyph run, as the game's text commands carry
    /// them: the reciprocal atlas size and the font's outline.
    unsafe fn glyph_constants(module: usize, layout: &Layout, fonts: &FontLayout, font: usize) -> [f32; 20] {
        let mut constants = color_constants([1.0 / fonts.atlas_size as f32; 2]);
        let object = read(module + fonts.table + font * layout.pointer_bytes, layout.pointer_bytes);
        if object != 0 && *((object + fonts.outline) as *const u8) != 0 {
            let outline = read_f32(object + fonts.outline + 4);
            constants[16] = outline;
            constants[17] = outline;
            constants[18] = fonts.outline_edge;
        }
        constants
    }
}

#[cfg(windows)]
pub(crate) use game::{renderer_slot, renderer_vtable};

#[cfg(test)]
mod tests {
    use super::*;

    const WHITE: u32 = 0xffff_ffff;
    const VIEW: View = View { width: 640.0, height: 480.0, map: (100, 200), mouse: (10, 20) };

    /// Glyphs of a fixed width font: every printable character 8 pixels
    /// wide, on page 0 except Hangul on page 1.
    struct Fixed;

    impl Glyphs for Fixed {
        fn glyph(&mut self, _font: usize, character: char) -> Option<Glyph> {
            let blank = character == ' ';
            Some(Glyph {
                page: u32::from(('\u{ac00}'..='\u{d7a3}').contains(&character)),
                bearing: [-1.0, -2.0],
                size: if blank { [0.0, 0.0] } else { [20.0, 24.0] },
                atlas: [100.0, 200.0],
                advance: 8.0,
            })
        }
        fn line_height(&mut self, _font: usize) -> f32 {
            10.0
        }
        fn atlas_size(&self) -> f32 {
            1000.0
        }
        fn pixel_scale(&self) -> f32 {
            2.0
        }
    }

    fn shape(kind: u8, coordinates: u8, points: [i32; 6], solid: bool) -> DrawShape {
        let [x1, y1, x2, y2, extra1, extra2] = points;
        DrawShape { kind, coordinates, solid: u8::from(solid), x1, y1, x2, y2, extra1, extra2, rgba: WHITE, ..DrawShape::default() }
    }

    fn mesh(shapes: Vec<DrawShape>, text: &[u8]) -> Mesh {
        let mut colors = [0; TEXT_COLORS];
        for (code, color) in colors.iter_mut().enumerate() {
            *color = code as u32;
        }
        let list = DrawList { shapes, text: text.to_vec(), text_colors: colors };
        let mut mesh = Mesh::default();
        mesh.build(&list, &VIEW, &mut Fixed);
        mesh
    }

    /// A vertex back in game pixels.
    fn pixel(vertex: &Vertex) -> (f32, f32) {
        (vertex.x * 640.0, (1.0 - vertex.y) * 480.0)
    }

    fn corners(quad: &[Vertex; 4]) -> [(f32, f32); 4] {
        quad.each_ref().map(|vertex| {
            let (x, y) = pixel(vertex);
            ((x * 100.0).round() / 100.0, (y * 100.0).round() / 100.0)
        })
    }

    #[test]
    fn the_shape_is_the_abi_record() {
        assert_eq!(std::mem::size_of::<DrawShape>(), 40);
        assert_eq!(std::mem::offset_of!(DrawShape, x1), 4);
        assert_eq!(std::mem::offset_of!(DrawShape, rgba), 28);
        assert_eq!(std::mem::offset_of!(DrawShape, text_len), 36);
        assert_eq!(std::mem::size_of::<Vertex>(), 20);
    }

    #[test]
    fn boxes_cover_bwapi_pixels_in_every_coordinate_type() {
        let mesh = mesh(
            vec![
                shape(shape_kind::BOX, coordinates::SCREEN, [30, 40, 10, 20, 0, 0], true),
                shape(shape_kind::BOX, coordinates::MAP, [110, 220, 130, 240, 0, 0], true),
                shape(shape_kind::BOX, coordinates::MOUSE, [0, 0, 20, 20, 0, 0], true),
            ],
            b"",
        );
        let expected = [(10.0, 20.0), (30.0, 20.0), (30.0, 40.0), (10.0, 40.0)];
        assert_eq!(mesh.quads.len(), 3);
        for quad in &mesh.quads {
            assert_eq!(corners(quad), expected);
        }
        assert_eq!(mesh.runs, vec![Run { ink: Ink::Color, first: 0, quads: 3 }]);
    }

    #[test]
    fn a_hollow_box_is_its_border_and_a_flat_box_nothing() {
        let mesh = mesh(
            vec![
                shape(shape_kind::BOX, coordinates::SCREEN, [10, 20, 30, 40, 0, 0], false),
                shape(shape_kind::BOX, coordinates::SCREEN, [10, 20, 30, 20, 0, 0], true),
            ],
            b"",
        );
        let quads: Vec<_> = mesh.quads.iter().map(corners).collect();
        assert_eq!(
            quads,
            vec![
                [(10.0, 20.0), (11.0, 20.0), (11.0, 40.0), (10.0, 40.0)],
                [(10.0, 20.0), (30.0, 20.0), (30.0, 21.0), (10.0, 21.0)],
                [(29.0, 20.0), (30.0, 20.0), (30.0, 40.0), (29.0, 40.0)],
                [(10.0, 39.0), (30.0, 39.0), (30.0, 40.0), (10.0, 40.0)],
            ]
        );
    }

    #[test]
    fn lines_cover_both_end_pixels_and_clip_to_the_screen() {
        let mesh = mesh(
            vec![
                shape(shape_kind::LINE, coordinates::SCREEN, [10, 20, 30, 20, 0, 0], false),
                shape(shape_kind::LINE, coordinates::SCREEN, [-100_000, 50, 100_000, 50, 0, 0], false),
                shape(shape_kind::LINE, coordinates::SCREEN, [-50, -50, -10, -10, 0, 0], false),
            ],
            b"",
        );
        let quads: Vec<_> = mesh.quads.iter().map(corners).collect();
        assert_eq!(
            quads,
            vec![
                [(10.0, 21.0), (31.0, 21.0), (31.0, 20.0), (10.0, 20.0)],
                [(-1.5, 51.0), (641.5, 51.0), (641.5, 50.0), (-1.5, 50.0)],
            ]
        );
    }

    #[test]
    fn dots_triangles_and_ellipses() {
        let mesh = mesh(
            vec![
                shape(shape_kind::DOT, coordinates::SCREEN, [5, 6, 0, 0, 0, 0], false),
                shape(shape_kind::TRIANGLE, coordinates::SCREEN, [0, 0, 10, 0, 0, 10], true),
                shape(shape_kind::TRIANGLE, coordinates::SCREEN, [0, 0, 10, 0, 0, 10], false),
                shape(shape_kind::CIRCLE, coordinates::SCREEN, [100, 100, 0, 0, 16, 0], true),
                shape(shape_kind::ELLIPSE, coordinates::SCREEN, [100, 100, 0, 0, 30, 0], false),
                shape(shape_kind::CIRCLE, coordinates::SCREEN, [-500, 100, 0, 0, 16, 0], false),
            ],
            b"",
        );
        let quads: Vec<_> = mesh.quads.iter().map(corners).collect();
        assert_eq!(quads[0], [(5.0, 6.0), (6.0, 6.0), (6.0, 7.0), (5.0, 7.0)]);
        assert_eq!(quads[1], [(0.5, 0.5), (10.5, 0.5), (0.5, 10.5), (0.5, 10.5)]);
        // The hollow triangle is three lines; the circle's 16 fan triangles
        // reach the outer pixel edge; the flat ellipse is one row; the
        // circle off the screen is left out.
        assert_eq!(quads.len(), 1 + 1 + 3 + segments(16) + 1);
        let fan = &quads[5..5 + segments(16)];
        assert!(fan.iter().all(|quad| quad[0] == (100.5, 100.5)));
        assert_eq!(fan[0][1], (117.0, 100.5));
        assert_eq!(quads.last(), Some(&[(70.0, 100.0), (131.0, 100.0), (131.0, 101.0), (70.0, 101.0)]));
    }

    #[test]
    fn text_follows_its_control_codes() {
        // "a" in the default color, "b" red, "c" in the previous color, a
        // new line in teal, an invisible "e", then the previous color for a
        // Hangul character on its own page.
        let text = "a\x06b\x01c\n\x0fd\x0be\x01\u{d55c}";
        let mut text_shape = shape(shape_kind::TEXT, coordinates::SCREEN, [100, 50, 0, 0, 0, 0], false);
        text_shape.text_len = text.len() as u32;
        let mesh = mesh(vec![text_shape], text.as_bytes());
        let glyphs: Vec<_> = mesh.quads.iter().map(|quad| (corners(quad)[0], quad[0].rgba, quad[0].u, quad[2].v)).collect();
        assert_eq!(
            glyphs,
            vec![
                ((99.0, 48.0), 2, 0.1, 0.224),
                ((107.0, 48.0), 6, 0.1, 0.224),
                ((115.0, 48.0), 2, 0.1, 0.224),
                ((99.0, 58.0), 15, 0.1, 0.224),
                ((115.0, 58.0), 2, 0.1, 0.224),
            ]
        );
        assert_eq!(
            mesh.runs,
            vec![
                Run { ink: Ink::Glyphs { font: 0, page: 0 }, first: 0, quads: 4 },
                Run { ink: Ink::Glyphs { font: 0, page: 1 }, first: 4, quads: 1 },
            ]
        );
    }

    #[test]
    fn alignment_places_the_widest_line_across_the_screen() {
        let text = "\x12ab\nabcd";
        let mut right = shape(shape_kind::TEXT, coordinates::SCREEN, [0, 0, 0, 0, 0, 0], false);
        right.text_len = text.len() as u32;
        let center_text = "\x13ab";
        let mut center = right;
        center.text_offset = text.len() as u32;
        center.text_len = center_text.len() as u32;
        let mesh = mesh(vec![right, center], format!("{text}{center_text}").as_bytes());
        let x: Vec<f32> = mesh.quads.iter().map(|quad| corners(quad)[0].0).collect();
        // The widest line is 32 pixels: right alignment starts the first
        // line at 640 - 32, then the second line at the origin.
        assert_eq!(x, vec![607.0, 615.0, -1.0, 7.0, 15.0, 23.0, 311.0, 319.0]);
    }

    #[test]
    fn shapes_keep_their_order_across_inks() {
        let text = "x";
        let mut label = shape(shape_kind::TEXT, coordinates::SCREEN, [0, 0, 0, 0, 0, 0], false);
        label.text_len = 1;
        let dot = shape(shape_kind::DOT, coordinates::SCREEN, [1, 1, 0, 0, 0, 0], false);
        let mesh = mesh(vec![dot, label, dot, dot], text.as_bytes());
        let inks: Vec<_> = mesh.runs.iter().map(|run| (run.ink, run.quads)).collect();
        assert_eq!(inks, vec![(Ink::Color, 1), (Ink::Glyphs { font: 0, page: 0 }, 1), (Ink::Color, 2)]);
    }

    #[test]
    fn a_text_shape_outside_the_text_is_refused() {
        let mut text_shape = shape(shape_kind::TEXT, coordinates::SCREEN, [0; 6], false);
        text_shape.text_offset = 2;
        text_shape.text_len = 2;
        assert!(set(&[text_shape], b"abc", &[0; TEXT_COLORS]).is_err());
        text_shape.text_offset = 1;
        assert!(set(&[text_shape], b"abc", &[0; TEXT_COLORS]).is_ok());
        assert_eq!(lock(&LISTS).front.shapes, vec![text_shape]);
        assert!(set(&[], b"", &[0; TEXT_COLORS]).is_ok());
        assert!(lock(&LISTS).front.shapes.is_empty());
    }
}
