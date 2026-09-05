//! `text` — shaping and glyph rasterization for gogo, pure Rust
//! (`cosmic-text` + `swash`): no GPU, no OS. Needed as its own crate so
//! `gogo` stays text-engine-free (it measures through `TextMeasure`) and
//! the backend only uploads what this crate rasterizes. Limit: one 8-bit
//! coverage atlas, shelf-packed; when it fills it doubles and every glyph
//! is rasterized again on demand. All sizes here are DEVICE pixels — the
//! caller converts epx.

use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use cosmic_text::fontdb;
use cosmic_text::{Attrs, Buffer, CacheKey, Family, FontSystem, Metrics, Shaping, SwashCache, SwashContent, Weight, Wrap};
use gogo::layout::TextMeasure;
use gogo::ImageKey;

/// Godot's default `Label` line_spacing (px), added between lines.
pub const LINE_SPACING_PX: f32 = 3.0;

/// Entries a measure / shape cache holds before it is emptied. Needed
/// because a label that changes every few frames (the debug overlay's
/// stats line, a zoom readout) is a new string each time — an unbounded
/// cache would grow for as long as the app runs. Emptying costs one
/// re-shape per visible string on the next frame, far below a frame.
pub const CACHE_CAP: usize = 4096;

/// A face's vertical metrics in font units, read from the file at load.
#[derive(Clone, Copy, Debug)]
pub struct FaceMetrics {
    pub ascent: f32,
    pub descent: f32,
    pub units_per_em: f32,
}

impl FaceMetrics {
    /// Roboto's hhea metrics (1900 / 500 over 2048): the fallback when a
    /// face carries none.
    pub const ROBOTO: FaceMetrics = FaceMetrics { ascent: 1900.0, descent: 500.0, units_per_em: 2048.0 };

    /// One line's own height in px at `font_size` px (no spacing) — a
    /// single-line label's box, the way Godot measures it: FreeType rounds
    /// the scaled ascender up and the descender down to whole pixels
    /// (`FT_Size_Metrics`), so 24 px Roboto is 23 + 6 = 29, not
    /// round(28.13) = 28 — one pixel per list row otherwise.
    pub fn font_height(&self, font_size: f32) -> f32 {
        let scale = font_size / self.units_per_em;
        (self.ascent * scale).ceil() + (self.descent * scale).ceil()
    }

    /// The pitch between lines: the face height plus the line spacing.
    pub fn line_height(&self, font_size: f32) -> f32 {
        self.font_height(font_size) + LINE_SPACING_PX
    }

    /// The box height of `lines` lines: spacing sits only BETWEEN lines
    /// (Godot: lines × font height + (lines − 1) × line_spacing).
    pub fn block_height(&self, font_size: f32, lines: f32) -> f32 {
        let n = lines.max(1.0);
        n * self.font_height(font_size) + (n - 1.0) * LINE_SPACING_PX
    }
}

/// Where a glyph sits in the atlas plus its placement relative to the pen.
#[derive(Clone, Copy, Debug)]
struct Slot {
    u0: f32,
    v0: f32,
    u1: f32,
    v1: f32,
    left: i32,
    top: i32,
    width: u32,
    height: u32,
}

/// One glyph to draw: a rect in device pixels and its atlas uv.
#[derive(Clone, Copy, Debug)]
pub struct GlyphQuad {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub uv: [f32; 4],
}

/// A loaded font file: which family/weight `Attrs` asks for, and its
/// vertical metrics.
#[derive(Clone, Debug)]
struct Face {
    family: String,
    weight: Weight,
    metrics: FaceMetrics,
}

pub struct TextEngine {
    fs: FontSystem,
    cache: SwashCache,
    /// path key (as written in the .gogo) → face.
    faces: HashMap<String, Face>,
    default_face: Option<Face>,
    atlas: Vec<u8>,
    atlas_size: u32,
    shelf_x: u32,
    shelf_y: u32,
    shelf_h: u32,
    slots: HashMap<CacheKey, Option<Slot>>,
    /// The atlas changed since the last `take_dirty`.
    dirty: bool,
    /// Set when the atlas grew: every uv handed out before is stale.
    pub generation: u32,
    /// Keyed by a hash of (text, size, width, font) — no String is built
    /// to look a measure up (a page lays out every animated frame). The
    /// engine serves every page, and their `StrId`s overlap, so the text
    /// itself is hashed, not its id. A 64-bit collision would return a
    /// wrong size for one string in ~10^19; accepted.
    measure_cache: HashMap<u64, (f32, f32)>,
    /// Shaped quads at the origin, keyed by a hash of (text, size, width,
    /// font, box_w, halign), with the atlas generation they were shaped
    /// against.
    shape_cache: HashMap<u64, (u32, Vec<GlyphQuad>)>,
}

/// The measure / shape cache key: a hash over the request, no allocation.
fn cache_key(text: &str, font_size: f32, max_width: Option<f32>, font: Option<&str>, box_w: f32, halign: u8) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    font_size.to_bits().hash(&mut h);
    max_width.unwrap_or(-1.0).to_bits().hash(&mut h);
    font.unwrap_or("").hash(&mut h);
    box_w.to_bits().hash(&mut h);
    halign.hash(&mut h);
    h.finish()
}

impl TextEngine {
    /// `fonts`: (key as the .gogo names it, file bytes). The first is the
    /// default face.
    pub fn new(fonts: &[(&str, Vec<u8>)]) -> TextEngine {
        let mut db = cosmic_text::fontdb::Database::new();
        let mut loaded: Vec<(String, cosmic_text::fontdb::ID, String, Weight)> = Vec::new();
        for (key, bytes) in fonts {
            let before: Vec<_> = db.faces().map(|f| f.id).collect();
            db.load_font_data(bytes.clone());
            if let Some(face) = db.faces().find(|f| !before.contains(&f.id)) {
                let family = face.families.first().map(|(n, _)| n.clone()).unwrap_or_default();
                loaded.push((key.to_string(), face.id, family, face.weight));
            }
        }
        // Only our fonts: no system-font scan (hundreds of ms on Android).
        let mut fs = FontSystem::new_with_locale_and_db("en-US".to_string(), db);
        let mut faces = HashMap::new();
        let mut default_face = None;
        for (key, id, family, weight) in loaded {
            // The file's own ascent/descent: the line height comes from
            // them, not from a constant fitted to one size.
            let metrics = fs
                .get_font(id, weight)
                .map(|font| {
                    let m = font.as_swash().metrics(&[]);
                    FaceMetrics { ascent: m.ascent, descent: m.descent, units_per_em: m.units_per_em as f32 }
                })
                .filter(|m| m.units_per_em > 0.0 && m.ascent > 0.0)
                .unwrap_or(FaceMetrics::ROBOTO);
            let f = Face { family, weight, metrics };
            if default_face.is_none() {
                default_face = Some(f.clone());
            }
            faces.insert(key, f);
        }
        let atlas_size = 1024;
        TextEngine {
            fs,
            cache: SwashCache::new(),
            faces,
            default_face,
            atlas: vec![0; (atlas_size * atlas_size) as usize],
            atlas_size,
            shelf_x: 0,
            shelf_y: 0,
            shelf_h: 0,
            slots: HashMap::new(),
            dirty: true,
            generation: 0,
            measure_cache: HashMap::new(),
            shape_cache: HashMap::new(),
        }
    }

    pub fn atlas(&self) -> (&[u8], u32) {
        (&self.atlas, self.atlas_size)
    }

    /// The language text is shaped for (a BCP-47 tag: `de`, `bn-IN`). Line
    /// breaking and script fallback follow it, so this moves with the
    /// string table (`gogo::Locale`) — translated strings alone would break
    /// mid-glyph in a script the old tag never expected. Rebuilds the
    /// font system on the SAME font database (no file re-read) and drops
    /// the measure/shape caches; the atlas and its glyphs stay. A load-time
    /// call, not per frame.
    pub fn set_locale(&mut self, bcp47: &str) {
        if self.fs.locale() == bcp47 {
            return;
        }
        let placeholder = FontSystem::new_with_locale_and_db(String::new(), fontdb::Database::new());
        let (_, db) = std::mem::replace(&mut self.fs, placeholder).into_locale_and_db();
        self.fs = FontSystem::new_with_locale_and_db(bcp47.to_string(), db);
        self.measure_cache.clear();
        self.shape_cache.clear();
    }

    pub fn take_dirty(&mut self) -> bool {
        std::mem::replace(&mut self.dirty, false)
    }

    fn face(&self, font: Option<&str>) -> Option<&Face> {
        font.and_then(|f| self.faces.get(f)).or(self.default_face.as_ref())
    }

    fn attrs<'a>(&'a self, font: Option<&str>) -> Attrs<'a> {
        match self.face(font) {
            Some(f) => Attrs::new().family(Family::Name(&f.family)).weight(f.weight),
            None => Attrs::new(),
        }
    }

    /// The metrics of the face `font` names (the default face, else Roboto's).
    pub fn metrics(&self, font: Option<&str>) -> FaceMetrics {
        self.face(font).map_or(FaceMetrics::ROBOTO, |f| f.metrics)
    }

    fn buffer(&mut self, text: &str, font_size: f32, max_width: Option<f32>, font: Option<&str>) -> Buffer {
        let line_height = self.metrics(font).line_height(font_size);
        let mut buffer = Buffer::new(&mut self.fs, Metrics::new(font_size, line_height));
        buffer.set_wrap(if max_width.is_some() { Wrap::WordOrGlyph } else { Wrap::None });
        buffer.set_size(max_width, None);
        let attrs = self.attrs(font);
        buffer.set_text(text, &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut self.fs, false);
        buffer
    }

    /// Width and height in device pixels.
    pub fn measure(&mut self, text: &str, font_size: f32, max_width: Option<f32>, font: Option<&str>) -> (f32, f32) {
        let key = cache_key(text, font_size, max_width, font, 0.0, 0);
        if let Some(v) = self.measure_cache.get(&key) {
            return *v;
        }
        let buffer = self.buffer(text, font_size, max_width, font);
        let mut w: f32 = 0.0;
        let mut lines: f32 = 0.0;
        for run in buffer.layout_runs() {
            w = w.max(run.line_w);
            lines += 1.0;
        }
        let h = self.metrics(font).block_height(font_size, lines);
        let v = (w.ceil(), h);
        if self.measure_cache.len() >= CACHE_CAP {
            self.measure_cache.clear();
        }
        self.measure_cache.insert(key, v);
        v
    }

    /// Shapes `text` into glyph quads with the pen at (x, y) = the top-left
    /// of the text box; rasterizes new glyphs into the atlas. `halign`:
    /// 0 start, 1 center, 2 end within `box_w` (when wrapping).
    pub fn shape(&mut self, text: &str, font_size: f32, max_width: Option<f32>, font: Option<&str>, x: f32, y: f32, box_w: f32, halign: u8) -> Vec<GlyphQuad> {
        let key = cache_key(text, font_size, max_width, font, box_w, halign);
        if let Some((generation, quads)) = self.shape_cache.get(&key) {
            if *generation == self.generation {
                let (dx, dy) = (x.round(), y.round());
                return quads.iter().map(|q| GlyphQuad { x: q.x + dx, y: q.y + dy, ..*q }).collect();
            }
        }
        let quads = self.shape_uncached(text, font_size, max_width, font, box_w, halign);
        if self.shape_cache.len() >= CACHE_CAP {
            self.shape_cache.clear();
        }
        self.shape_cache.insert(key, (self.generation, quads.clone()));
        let (dx, dy) = (x.round(), y.round());
        quads.into_iter().map(|q| GlyphQuad { x: q.x + dx, y: q.y + dy, ..q }).collect()
    }

    /// Quads with the pen at the origin.
    fn shape_uncached(&mut self, text: &str, font_size: f32, max_width: Option<f32>, font: Option<&str>, box_w: f32, halign: u8) -> Vec<GlyphQuad> {
        let (x, y) = (0.0f32, 0.0f32);
        let buffer = self.buffer(text, font_size, max_width, font);
        let mut out = Vec::new();
        let runs: Vec<(f32, f32, Vec<cosmic_text::PhysicalGlyph>)> = buffer.layout_runs().map(|run| (run.line_w, run.line_y, run.glyphs.iter().map(|g| g.physical((0.0, 0.0), 1.0)).collect())).collect();
        for (line_w, line_y, glyphs) in runs {
            let shift = match halign {
                1 => ((box_w - line_w) / 2.0).max(0.0),
                2 => (box_w - line_w).max(0.0),
                _ => 0.0,
            };
            for g in glyphs {
                let Some(slot) = self.slot(g.cache_key) else { continue };
                if slot.width == 0 || slot.height == 0 {
                    continue;
                }
                out.push(GlyphQuad {
                    x: (x + shift).round() + g.x as f32 + slot.left as f32,
                    // cosmic splits the line spacing half above each line;
                    // Godot puts none above the first, so the rows sit
                    // half a spacing higher.
                    y: y.round() + (line_y - LINE_SPACING_PX / 2.0).round() + g.y as f32 - slot.top as f32,
                    w: slot.width as f32,
                    h: slot.height as f32,
                    uv: [slot.u0, slot.v0, slot.u1, slot.v1],
                });
            }
        }
        out
    }

    fn slot(&mut self, key: CacheKey) -> Option<Slot> {
        if let Some(s) = self.slots.get(&key) {
            return *s;
        }
        let generation = self.generation;
        let slot = self.rasterize(key);
        if slot.is_none() && self.generation != generation {
            // The atlas grew under this glyph: pack it again into the new one.
            let again = self.rasterize(key);
            self.slots.insert(key, again);
            return again;
        }
        self.slots.insert(key, slot);
        slot
    }

    fn rasterize(&mut self, key: CacheKey) -> Option<Slot> {
        let image = self.cache.get_image(&mut self.fs, key).clone();
        let slot = image.and_then(|img| {
            if img.content != SwashContent::Mask {
                return None;
            }
            let (w, h) = (img.placement.width, img.placement.height);
            if w == 0 || h == 0 {
                return Some(Slot { u0: 0.0, v0: 0.0, u1: 0.0, v1: 0.0, left: 0, top: 0, width: 0, height: 0 });
            }
            let (px, py) = self.pack(w + 1, h + 1)?;
            for row in 0..h {
                let src = &img.data[(row * w) as usize..((row + 1) * w) as usize];
                let dst = ((py + row) * self.atlas_size + px) as usize;
                self.atlas[dst..dst + w as usize].copy_from_slice(src);
            }
            self.dirty = true;
            let s = self.atlas_size as f32;
            Some(Slot { u0: px as f32 / s, v0: py as f32 / s, u1: (px + w) as f32 / s, v1: (py + h) as f32 / s, left: img.placement.left, top: img.placement.top, width: w, height: h })
        });
        slot
    }

    /// Shelf packing; `None` after a grow (the caller retries via `slot`).
    fn pack(&mut self, w: u32, h: u32) -> Option<(u32, u32)> {
        if self.shelf_x + w > self.atlas_size {
            self.shelf_y += self.shelf_h;
            self.shelf_x = 0;
            self.shelf_h = 0;
        }
        if self.shelf_y + h > self.atlas_size {
            self.grow();
            return None;
        }
        let pos = (self.shelf_x, self.shelf_y);
        self.shelf_x += w;
        self.shelf_h = self.shelf_h.max(h);
        Some(pos)
    }

    fn grow(&mut self) {
        self.shape_cache.clear();
        self.atlas_size *= 2;
        self.atlas = vec![0; (self.atlas_size * self.atlas_size) as usize];
        self.shelf_x = 0;
        self.shelf_y = 0;
        self.shelf_h = 0;
        self.slots.clear();
        self.generation += 1;
        self.dirty = true;
    }

    /// Rasterizes every glyph of `chars` at `font_size` so the first frame
    /// that shows them does no work (the boot warm).
    pub fn warm(&mut self, chars: &str, font_size: f32, font: Option<&str>) {
        let _ = self.shape(chars, font_size, None, font, 0.0, 0.0, 0.0, 0);
    }
}

/// `TextMeasure` for gogo's layout: epx in, epx out, device pixels inside.
pub struct EpxMeasure<'a> {
    pub engine: &'a RefCell<TextEngine>,
    pub epx: f32,
    /// The page's default font key (the `theme { font = … }` line).
    pub font: Option<String>,
    /// (width, height) px by `ImageKey` for `aspect=natural`; `None` = no
    /// readable header (a host picture, or a bad file).
    pub images: &'a [Option<(u32, u32)>],
}

impl TextMeasure for EpxMeasure<'_> {
    fn measure(&self, text: &str, font_size: f32, max_width: Option<f32>, font: Option<&str>) -> (f32, f32) {
        let mut e = self.engine.borrow_mut();
        // Godot sizes fonts as `int(epx * size)` (truncated): same here, or
        // every line measures a hair wider and wraps a word early.
        let (w, h) = e.measure(text, (font_size * self.epx).floor().max(1.0), max_width.map(|w| w * self.epx), font.or(self.font.as_deref()));
        (w / self.epx, h / self.epx)
    }

    fn image_size(&self, key: ImageKey) -> Option<(f32, f32)> {
        self.images.get(key.index()).copied().flatten().map(|(w, h)| (w as f32, h as f32))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine() -> TextEngine {
        let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/Fonts/Roboto Regular.ttf")).expect("font");
        TextEngine::new(&[("assets/Fonts/Roboto Regular.ttf", bytes)])
    }

    /// The Roboto faces' metrics as the files carry them (the layout
    /// contract reads them, never a fitted constant).
    #[test]
    fn roboto_faces_carry_their_metrics() {
        for name in ["Roboto Regular", "Roboto Light", "Roboto Medium", "Roboto Bold"] {
            let path = format!("{}/../../assets/Fonts/{name}.ttf", env!("CARGO_MANIFEST_DIR"));
            let bytes = std::fs::read(&path).expect("font");
            let e = TextEngine::new(&[(name, bytes)]);
            let m = e.metrics(None);
            println!("{name}: ascent {} descent {} upem {} -> 24px {} 28px {} 36px {} 48px {}", m.ascent, m.descent, m.units_per_em, m.font_height(24.0), m.font_height(28.0), m.font_height(36.0), m.font_height(48.0));
            assert!(m.units_per_em > 0.0);
        }
    }

    #[test]
    fn measures_wraps_and_rasterizes() {
        let mut e = engine();
        let (w, h) = e.measure("Hello", 20.0, None, None);
        assert!(w > 30.0 && w < 70.0, "{w}");
        assert_eq!(h, 24.0);
        // PR4: Godot's per-size whole-pixel ascent + descent, not a ratio.
        let m = e.metrics(None);
        assert_eq!(m.font_height(24.0), 29.0);
        assert_eq!(m.font_height(28.0), 33.0);
        assert_eq!(m.font_height(20.0), 24.0);
        let (_, h2) = e.measure("one two three four five six seven", 20.0, Some(100.0), None);
        assert!(h2 >= 48.0, "{h2}");
        let quads = e.shape("Hi", 20.0, None, None, 10.0, 5.0, 0.0, 0);
        assert_eq!(quads.len(), 2);
        assert!(quads[0].x >= 10.0 && quads[1].x > quads[0].x);
        let (atlas, size) = e.atlas();
        assert!(atlas.iter().any(|b| *b > 128), "atlas has ink");
        assert_eq!(size, 1024);
        assert!(e.take_dirty());
        assert!(!e.take_dirty());
    }
}
