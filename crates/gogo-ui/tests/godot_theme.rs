//! Port of the Godot gogo suite (`FantasiaGodot/addons/gogo/test/run_tests.gd`),
//! batch `theme`:
//! - `_theme_and_styles` lines 2439-2651 (32 checks)
//! - `_palette_tests` lines 2651-2833 (30 checks)
//! - `_translation_tests` lines 2833-2971 (21 checks)
//! - `_pigment_alpha_tests` lines 2971-2984 (4 checks)
//! - `_for_style_tests` lines 3760-3982 (31 checks)
//! - `_radial_tests` lines 3982-4119 (25 checks)
//!
//! Dialect: Godot sources with a bare `Button`/`Rectangle` root are wrapped
//! in `View v { }` (a Rust page has one View/Panel root); `.tres` font paths
//! become `res://fonts/x.ttf` (compile does not check presence, the build
//! does); the theme variant is set straight on `Runtime::variant`.
//!
//! ENGINE (not ported, no Rust counterpart):
//! - `tr.custom-prop` (2917-2925): host-registered custom property callbacks.
//! - `radial.gesture-claims` / `gesture-value` / `gesture-wedge` (4058-4067),
//!   `radial.real-input-claims` / `real-input-releases` (4095-4105): the
//!   fixture script `knob_behavior.gd` drives the gesture (script-backed
//!   control input, `_gui_input` / viewport `push_input`).
use gogo::draw::{Draw, draw};
use gogo::layout::{FixedAdvance, Rect, layout};
use std::sync::Arc;

use gogo::locale::{Keys, Locale, display};
use gogo::page::{NO_KEY, NodeId, PropKey, Rgba, Value};
use gogo::runtime::{Resolved, Runtime, deref, resolve};
use gogo::{Compiled, Diagnostic, Page, Severity, compile};

fn build(src: &str) -> Compiled {
    compile(src, &|_| None)
}

fn ok(src: &str) -> Compiled {
    let c = build(src);
    assert!(!gogo::has_errors(&c.diags), "{:#?}", c.diags);
    c
}

fn errors(src: &str) -> Vec<Diagnostic> {
    build(src).diags.into_iter().filter(|d| d.severity == Severity::Error).collect()
}

fn codes(src: &str) -> Vec<String> {
    build(src).diags.iter().map(|d| d.code.clone()).collect()
}

/// `#RRGGBB` opaque.
fn rgb(hex: u32) -> Rgba {
    Rgba([(hex >> 16) as u8, (hex >> 8) as u8, hex as u8, 0xFF])
}

/// `#RRGGBB` at an alpha byte.
fn rgb_a(hex: u32, a: u8) -> Rgba {
    Rgba([(hex >> 16) as u8, (hex >> 8) as u8, hex as u8, a])
}

/// Godot's `is_equal_approx` on a Color: the Rust side rounds to bytes.
fn alpha_of(fraction: f32) -> u8 {
    (fraction * 255.0).round() as u8
}

/// A compiled page, resolved and laid out in a 400x400 frame.
struct Fx {
    page: Page,
    /// This compile's own key table (`keys.names[key]` spells a key).
    keys: Keys,
    rt: Runtime,
    res: Resolved,
    rects: Vec<Rect>,
    draws: Vec<Draw>,
}

impl Fx {
    fn new(src: &str) -> Fx {
        Fx::build(src, "", None)
    }

    fn with_variant(src: &str, variant: &str) -> Fx {
        Fx::build(src, variant, None)
    }

    fn with_locale(src: &str, locale: Locale) -> Fx {
        Fx::build(src, "", Some(Arc::new(locale)))
    }

    fn build(src: &str, variant: &str, locale: Option<Arc<Locale>>) -> Fx {
        let c = ok(src);
        let page = c.page;
        let mut rt = Runtime::new(&page);
        if !variant.is_empty() {
            rt.variant = page.intern_lookup(variant);
        }
        rt.set_locale(locale);
        let res = resolve(&page, &rt);
        let rects = layout(&page, &res, Rect::new(0.0, 0.0, 400.0, 400.0), &FixedAdvance);
        let draws = draw(&page, &res, &rects);
        Fx { page, keys: c.keys, rt, res, rects, draws }
    }

    /// Switches the language on the SAME runtime and lays out again.
    fn set_locale(&mut self, locale: Option<Locale>) {
        self.rt.set_locale(locale.map(Arc::new));
        self.res = resolve(&self.page, &self.rt);
        self.rects = layout(&self.page, &self.res, Rect::new(0.0, 0.0, 400.0, 400.0), &FixedAdvance);
        self.draws = draw(&self.page, &self.res, &self.rects);
    }

    fn id(&self, name: &str) -> NodeId {
        self.page.find(name).unwrap_or_else(|| panic!("no node {name:?}"))
    }

    fn color(&self, name: &str, key: PropKey) -> Rgba {
        let id = self.id(name);
        self.res.color(&self.page, id, key).unwrap_or_else(|| panic!("{name} has no {key:?}: {:?}", self.page.node_props(id)))
    }

    fn rect(&self, name: &str) -> Rect {
        self.rects[self.id(name) as usize]
    }

    fn font(&self, name: &str) -> String {
        let id = self.id(name);
        self.res.font(&self.page, id).map(|s| self.page.str(s).to_string()).unwrap_or_default()
    }

    fn text(&self, name: &str) -> String {
        let id = self.id(name);
        match self.res.prop(&self.page, id, PropKey::Text) {
            Some(Value::VText { text, .. }) => self.page.str(text).to_string(),
            other => panic!("{name} has no text: {other:?}"),
        }
    }

    /// (shown text, key) of the drawn text that starts with `starts` — the
    /// shown text through `locale::display`, as the renderer reads it.
    fn drawn_text(&self, starts: &str) -> (String, String) {
        self.draws
            .iter()
            .find_map(|d| match d {
                Draw::Text { text, key, .. } => {
                    let shown = display(&self.page, self.rt.locale.as_deref(), *text, *key);
                    let name = if *key == NO_KEY { String::new() } else { self.keys.names[*key as usize].clone() };
                    shown.starts_with(starts).then(|| (shown.to_string(), name))
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("no drawn text starting {starts:?} in {:#?}", self.draws))
    }

    fn theme(&self, name: &str) -> Value {
        let id = self.page.intern_lookup(name);
        deref(&self.page, self.res.variant, Value::VRef { name: id, opacity: None })
    }
}

// ------------------------------------------------------------ _theme_and_styles

/// `_theme_and_styles` 2440-2451 (`theme.chain`, `theme.rgba-color`): a
/// `$ref` may point at another `$ref`, and a style nobody uses may carry a
/// word the compiler never sees (styles apply lazily). The swatch shows
/// the chained color, alpha last.
#[test]
fn theme_ref_chains_and_unused_styles_stay_quiet() {
    let fx = Fx::new("$accent = #E51400FF\n$accent2 = $accent\nstyle Big min_height=36, tap=true\nView v { Rectangle swatch color=$accent2 }\n");
    assert_eq!(fx.color("swatch", PropKey::Color), rgb(0xE51400));
}

/// `_theme_and_styles` 2453-2492 (`variant.inactive-uses-default`,
/// `variant.active-overrides`, `variant.undefined-falls-back`,
/// `theme.default-block-token`): a `theme dark { $x = }` block overrides
/// the default only while `dark` is the active variant; a variant no block
/// names falls back; the unnamed `theme { }` block IS the defaults block.
#[test]
fn variant_blocks_override_only_while_active() {
    let vsrc = "$accent = #E51400FF\ntheme dark {\n    $accent = #00FF00FF\n}\nView v { Rectangle swatch color=$accent }\n";
    assert_eq!(Fx::new(vsrc).color("swatch", PropKey::Color), rgb(0xE51400), "inactive uses the default");
    assert_eq!(Fx::with_variant(vsrc, "dark").color("swatch", PropKey::Color), rgb(0x00FF00), "active overrides");
    assert_eq!(Fx::with_variant(vsrc, "light").color("swatch", PropKey::Color), rgb(0xE51400), "an undefined variant falls back");
    let defblock = Fx::new("theme {\n    $accent = #00FF00FF\n}\nView v { Rectangle swatch color=$accent }\n");
    assert_eq!(defblock.color("swatch", PropKey::Color), rgb(0x00FF00));
}

/// `gogo.set_theme_variant` (gogo.gd:237-248, GP38): the host seam that
/// flips the active variant live — the next resolve reads the new values;
/// `""` goes back to the defaults; a variant no file defines keeps them.
#[test]
fn set_theme_variant_is_the_host_seam() {
    let vsrc = "$accent = #E51400FF\ntheme dark {\n    $accent = #00FF00FF\n}\nView v { Rectangle swatch color=$accent }\n";
    let mut fx = Fx::new(vsrc);
    fx.rt.set_theme_variant(&fx.page, "dark");
    fx.res = resolve(&fx.page, &fx.rt);
    assert_eq!(fx.color("swatch", PropKey::Color), rgb(0x00FF00), "the flip reads dark");
    fx.rt.set_theme_variant(&fx.page, "");
    fx.res = resolve(&fx.page, &fx.rt);
    assert_eq!(fx.color("swatch", PropKey::Color), rgb(0xE51400), "empty = defaults only");
    fx.rt.set_theme_variant(&fx.page, "sepia");
    fx.res = resolve(&fx.page, &fx.rt);
    assert_eq!(fx.color("swatch", PropKey::Color), rgb(0xE51400), "an undefined variant keeps the defaults");
}

/// `_theme_and_styles` 2494-2498 (`variant.bare-entry-errors`,
/// `variant.nested-set-errors`): a theme-block entry without `=` is refused
/// (Rust reads `accent #FFFFFFFF` as a node line: G3006 "not a control
/// kind"), and a `$name = value` inside a node body is refused (G0022 —
/// theme values live at the top level).
#[test]
fn theme_entry_shape_errors() {
    let bare = codes("theme dark {\n    accent #FFFFFFFF\n}\nView v { Button b text=\"x\" }\n");
    assert!(bare.contains(&"G3006".to_string()), "{bare:?}");
    let nested = codes("View v { Button b text=\"x\" {\n    $accent = #FFFFFFFF\n} }\n");
    assert!(nested.contains(&"G0022".to_string()), "{nested:?}");
}

/// `_theme_and_styles` 2500-2512 (`theme.number-font`): a numeric theme
/// value reaches `font_size=` through a `$chain` exactly like an inline
/// number (consumers build font ramps on it).
#[test]
fn numeric_theme_value_feeds_font_size_through_a_chain() {
    let fx = Fx::new("$rowFont = 14\n$rowFont2 = $rowFont\nView v { Label t text=\"x\", font_size=$rowFont2 }\n");
    let t = fx.id("t");
    assert_eq!(fx.res.num(&fx.page, t, PropKey::FontSize), Some(14.0));
}

/// `_theme_and_styles` 2500-2512 (`theme.number-length`,
/// `theme.number-minsize`): a numeric `$value` feeds a LENGTH prop
/// (`min_width=$rowFont`) like an inline number — placement bakes at
/// compile and theme entries are compile-known, so the ref resolves there
/// (GPAR88, 2026-08-31; default entries only, a variant block cannot flip
/// a baked length).
#[test]
fn numeric_theme_value_feeds_length_props() {
    let src = "$rowFont = 14\n$rowFont2 = $rowFont\nView v { Label t text=\"x\", font_size=$rowFont2, min_width=$rowFont, halign=left, valign=top }\n";
    let fx = Fx::new(src);
    assert_eq!(fx.res.placement[fx.id("t") as usize].min_width, 14.0);
    assert_eq!(fx.rect("t").w, 14.0, "{:?}", fx.rect("t"));
}

/// `_theme_and_styles` 2514-2541 (`theme-override.ok`,
/// `theme-override.resource`, `theme-override.propagates`,
/// `theme-override.nearest-wins`): a node-body `theme font=` sets the font
/// of the enclosing node and every descendant (nearest ancestor wins),
/// even when the theme line sits BELOW the label it affects.
#[test]
fn node_body_theme_font_propagates_nearest_ancestor_wins() {
    let fx = Fx::new("$labelFont = \"res://fonts/a.ttf\"\nPanel root {\n    theme font=$labelFont\n    Label outer text=\"o\"\n    Panel inner {\n        Label nested text=\"n\"\n        theme font=\"res://fonts/b.ttf\"\n    }\n}\n");
    assert_eq!(fx.font("root"), "res://fonts/a.ttf", "the enclosing node carries the font");
    assert_eq!(fx.font("outer"), "res://fonts/a.ttf", "propagates to descendants");
    assert_eq!(fx.font("nested"), "res://fonts/b.ttf", "the nearer theme line wins, even below the label");
}

/// `_theme_and_styles` 2543-2545 (`theme-override.unknown-prop`) — FLIP,
/// GPAR20 DECIDED 2026-08-30: Godot refused any node-body theme key but
/// `font`; Rust accepts any key there (it folds into the node), so
/// `theme font_size=12` in a Button body builds clean and the Button
/// reads 12.
#[test]
fn node_body_theme_takes_any_key() {
    let c = ok("View v { Button b text=\"x\" {\n    theme font_size=12\n} }\n");
    assert!(c.diags.is_empty(), "{:#?}", c.diags);
    let fx = Fx::new("View v { Button b text=\"x\" {\n    theme font_size=12\n} }\n");
    assert_eq!(fx.res.num(&fx.page, fx.id("b"), PropKey::FontSize), Some(12.0));
}

/// `_theme_and_styles` 2546-2547 (`theme-override.missing-font`): a font
/// path that names no file fails the build (gogo-design §7: assets are
/// checked at build). In Rust that check lives in `build_pages`, not in
/// `compile` — so this runs a one-page build in a scratch folder.
#[test]
fn missing_font_file_fails_the_build() {
    let dir = std::env::temp_dir().join(format!("gogo_theme_missing_font_{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    std::fs::write(dir.join("page.gogo"), "View v { Label l text=\"x\", font=\"res://fonts/missing.ttf\" }\n").expect("write page");
    let result = gogo::build_pages(&dir, &dir.join("out.rs"), gogo::BuildOptions::new(&dir));
    let _ = std::fs::remove_dir_all(&dir);
    let err = result.expect_err("a missing font must fail the build");
    assert!(err.contains("missing.ttf") && err.contains("not found"), "{err}");
}

/// `_theme_and_styles` 2548-2549 (`theme-override.top-level-errors`):
/// Godot refuses `theme font=…` written as a top-level LINE (props on the
/// `theme` word itself, no block): "inside a node". Rust reads the line,
/// ignores its props and builds clean — the font is dropped without a word.
#[test]
fn top_level_theme_line_with_props_is_refused() {
    let errs = errors("theme font=\"res://fonts/a.ttf\"\nView v { Button b text=\"x\" }\n");
    assert!(!errs.is_empty(), "a top-level theme LINE must not build");
}

/// `_theme_and_styles` 2551-2568 (`theme.doc-font`), 2585-2596
/// (`theme.body-beats-doc`): a document `theme { font = … }` lands on
/// every label; a node-body theme line is the nearer scope and wins for its
/// subtree.
#[test]
fn document_theme_font_reaches_labels_and_a_body_line_beats_it() {
    let doc = Fx::new("theme {\n    font = \"res://fonts/a.ttf\"\n}\ntheme fancy {\n    font = \"res://fonts/b.ttf\"\n}\nView top {\n    Label t text=\"x\"\n}\n");
    assert_eq!(doc.font("t"), "res://fonts/a.ttf");
    let rooted = Fx::new("theme {\n    font = \"res://fonts/a.ttf\"\n}\nPanel top {\n    theme font=\"res://fonts/b.ttf\"\n    Label t text=\"x\"\n}\n");
    assert_eq!(rooted.font("t"), "res://fonts/b.ttf");
}

/// `_theme_and_styles` 2569-2573 (`theme.variant-font`): a named block's
/// `font = …` applies only while that variant is active (GPAR88,
/// 2026-08-31 — `KindDefault::variant`, the active variant's entry wins
/// over the always-on one). A palette override in a non-`dark` variant
/// block is refused (G6006) — only `dark` has a second color column;
/// variant-scoped colors are `$values`.
#[test]
fn variant_block_font_applies_while_active() {
    let src = "theme {\n    font = \"res://fonts/a.ttf\"\n}\ntheme fancy {\n    font = \"res://fonts/b.ttf\"\n}\nView top {\n    Label t text=\"x\"\n}\n";
    assert_eq!(Fx::with_variant(src, "fancy").font("t"), "res://fonts/b.ttf");
    assert_eq!(Fx::new(src).font("t"), "res://fonts/a.ttf", "no variant active reads the default entry");
    let refused = codes("theme fancy {\n    red = #112233\n}\nView v { Rectangle r color=red }\n");
    assert!(refused.contains(&"G6006".to_string()), "a palette override outside dark is refused: {refused:?}");
}

/// GPAR88 round 2: a `theme dark {{ }}` palette override compiles uses of
/// the word (and its aliases) to a light/dark pair — the dark variant
/// repaints them, no variant reads the plain override chain.
#[test]
fn dark_block_palette_override_flips_at_use() {
    let src = "theme dark {\n    red = #112233\n}\nView v { Row w { Rectangle r color=red\n Rectangle d color=danger } }\n";
    assert_eq!(Fx::new(src).color("r", PropKey::Color), rgb(0xFF6347), "light keeps the built-in");
    let dark = Fx::with_variant(src, "dark");
    assert_eq!(dark.color("r", PropKey::Color), rgb(0x112233));
    assert_eq!(dark.color("d", PropKey::Color), rgb(0x112233), "the alias follows the dark override");
}

/// `_theme_and_styles` 2598-2626 (`export.ok`, `export.vars`,
/// `export.token-override`, `export.token-builtin`): Godot read these facts
/// from the editor's color-export tables; here the same facts are read
/// through `$ref` deref and swatches — a color `$var` resolves to its color,
/// `$alias = $var/0.5` carries alpha 0.5, a number `$var` is not a color,
/// `theme { red = }` overrides the word, a word nobody touched keeps its
/// built-in value.
#[test]
fn color_vars_and_palette_overrides_read_back() {
    let fx = Fx::new("$accentVar = #E51400\n$alias     = $accentVar/0.5\n$notColor  = 12\ntheme {\n    red = #AA0000\n    accent = purple\n}\nView v { Row w {\n    Label t text=\"x\"\n    Rectangle r color=red\n    Rectangle b color=blue\n} }\n");
    assert_eq!(fx.theme("accentVar"), Value::VColor(rgb(0xE51400)));
    assert_eq!(fx.theme("alias"), Value::VColor(rgb_a(0xE51400, alpha_of(0.5))));
    assert_eq!(fx.theme("notColor"), Value::VNum(12.0), "a number is not a color");
    assert_eq!(fx.color("r", PropKey::Color), rgb(0xAA0000), "token override");
    assert_eq!(fx.color("b", PropKey::Color), rgb(0x6495ED), "built-in token untouched");
}

/// `_theme_and_styles` 2628-2635 (`theme.doc-unknown-prop`,
/// `theme.stray-entry-errors`, `theme.top-entry-errors`): `width = 12` in
/// a theme block is refused (a layout prop is style territory — Rust: G6003
/// "not a palette color"); a bare `font = 12` inside a node body or at the
/// top level is refused (G0022 — bare assignments belong in `when` blocks).
#[test]
fn theme_block_refuses_layout_props_and_stray_entries() {
    let bad_doc = codes("theme {\n    width = 12\n}\nView v { Button b text=\"x\" }\n");
    assert!(bad_doc.contains(&"G6003".to_string()), "{bad_doc:?}");
    let stray = codes("View v { Button b text=\"x\" {\n    font = 12\n} }\n");
    assert!(stray.contains(&"G0022".to_string()), "{stray:?}");
    let stray_top = codes("font = 12\nView v { Button b text=\"x\" }\n");
    assert!(stray_top.contains(&"G0022".to_string()), "{stray_top:?}");
}

/// `_theme_and_styles` 2637-2647 (`style.ok`, `style.applied`,
/// `style.local-override`): a style's props land on the node that lists it;
/// a prop written on the node itself wins over the style's.
#[test]
fn style_props_apply_and_local_props_win() {
    let fx = Fx::new("style Tool min_height=36, text=\"styled\"\nView v { Column c {\n    Button a .Tool\n    Button b .Tool text=\"local wins\"\n} }\n");
    assert_eq!(fx.res.placement[fx.id("a") as usize].min_height, 36.0);
    assert_eq!(fx.text("a"), "styled");
    assert_eq!(fx.text("b"), "local wins");
}

// ---------------------------------------------------------------- _palette_tests

/// `_palette_tests` 2658-2687 (`palette.literal`,
/// `palette.alias-follows-literal`, `palette.snake-alias`,
/// `palette.stylebox-color`): bare color words are values in every color
/// position — a Rectangle fill, a semantic alias, a snake_case alias, a
/// Button background.
#[test]
fn palette_words_are_values_in_color_positions() {
    let fx = Fx::new("View v { Row r {\n    Rectangle a color=red\n    Rectangle b color=danger\n    Rectangle c color=text_muted\n    Button e background=silver, text=\"x\" {\n        when held background=white\n    }\n} }\n");
    assert_eq!(fx.color("a", PropKey::Color), rgb(0xFF6347));
    assert_eq!(fx.color("b", PropKey::Color), rgb(0xFF6347), "danger follows red");
    assert_eq!(fx.color("c", PropKey::Color), rgb(0x708090), "text_muted follows gray");
    assert_eq!(fx.color("e", PropKey::Background), rgb(0xDCDCDC));
}

/// `_palette_tests` 2670-2672, 2733-2765 (`palette.mode-token-light-default`,
/// `palette.dark-flips-mode-tokens`, `palette.other-variant-stays-light`,
/// `palette.dark-override-wins`): the mode tokens `surface` / `text_muted`
/// read the light column by default (#FBFAF7), flip under the well-known
/// variant `dark` (#1E1F22 / #9AA3AD), stay light under any other variant
/// name, and a `theme dark { surface = }` beats the built-in dark column.
/// GPAR111 (user 2026-08-31): compile keeps both halves (`VColorPair`),
/// `deref` picks the active one — also pinned: a plain-block override
/// replaces the pair for ALL modes (REFERENCE.md), a theme `$value`
/// carries the pair through the ref, and a mode word in `stops=` is
/// refused (G2008 — a stop bakes one color).
#[test]
fn mode_tokens_flip_under_dark() {
    let dsrc = "View v { Row r {\n    Rectangle s color=surface\n    Rectangle m color=text_muted\n} }\n";
    assert_eq!(Fx::new(dsrc).color("s", PropKey::Color), rgb(0xFBFAF7), "light default");
    let dark = Fx::with_variant(dsrc, "dark");
    assert_eq!(dark.color("s", PropKey::Color), rgb(0x1E1F22));
    assert_eq!(dark.color("m", PropKey::Color), rgb(0x9AA3AD));
    assert_eq!(Fx::with_variant(dsrc, "night").color("s", PropKey::Color), rgb(0xFBFAF7), "any other variant reads light");
    let dover = Fx::with_variant("theme dark {\n    surface = #141414\n}\nView v { Rectangle s color=surface }\n", "dark");
    assert_eq!(dover.color("s", PropKey::Color), rgb(0x141414));
    let psrc = "theme {\n    surface = #223344\n}\nView v { Rectangle s color=surface }\n";
    assert_eq!(Fx::new(psrc).color("s", PropKey::Color), rgb(0x223344));
    assert_eq!(Fx::with_variant(psrc, "dark").color("s", PropKey::Color), rgb(0x223344), "a plain override replaces the pair for all modes");
    let rsrc = "theme {\n    $ground = surface\n}\nView v { Rectangle s color=$ground }\n";
    assert_eq!(Fx::with_variant(rsrc, "dark").color("s", PropKey::Color), rgb(0x1E1F22), "a $value carries the pair through the ref");
    assert!(codes("View v { Gradient g stops=[surface@0, white@1], width=10, height=10 }").contains(&"G2008".to_string()), "a mode word in stops= is refused");
}

/// `_palette_tests` 2665, 2684-2686 (`palette.font_color`): a palette
/// word in `font_color=` colors the text — the mode token `text` is
/// #2E3338 in the light column.
#[test]
fn font_color_takes_a_palette_word() {
    let fx = Fx::new("View v { Label f text=\"y\", font_color=text }\n");
    assert_eq!(fx.color("f", PropKey::FontColor), rgb(0x2E3338));
}

/// `_palette_tests` 2689-2707 (`palette.override-wins`,
/// `palette.black-overridable`): a `theme { red = }` override (no `$`)
/// wins over the built-in word; `black` is a word like any other.
#[test]
fn theme_block_overrides_palette_words() {
    let fx = Fx::new("theme {\n    red = #112233\n    black = #101010\n}\nView v { Row r {\n    Rectangle a color=red\n    Rectangle c color=black\n} }\n");
    assert_eq!(fx.color("a", PropKey::Color), rgb(0x112233));
    assert_eq!(fx.color("c", PropKey::Color), rgb(0x101010));
}

/// `_palette_tests` 2689-2703 (`palette.alias-follows-override`) and
/// `_theme_and_styles` 2619-2623 (`export.token-remap`, the `danger` half):
/// `danger` is an alias OF `red`, so overriding `red` moves `danger` too.
/// Rust resolves every alias to bytes when the palette is built, so an
/// override of `red` leaves `danger` at the old tomato.
#[test]
fn aliases_follow_an_override_of_their_word() {
    let fx = Fx::new("theme {\n    red = #112233\n}\nView v { Rectangle b color=danger }\n");
    assert_eq!(fx.color("b", PropKey::Color), rgb(0x112233));
    let remap = Fx::new("theme {\n    red = #AA0000\n    accent = purple\n}\nView v { Row r { Rectangle ac color=accent\n Rectangle d color=danger } }\n");
    assert_eq!(remap.color("ac", PropKey::Color), rgb(0x9370DB));
    assert_eq!(remap.color("d", PropKey::Color), rgb(0xAA0000), "danger follows red's override");
}

/// `_palette_tests` 2709-2723 (`palette.alias-remap`): a semantic token
/// remapped to another palette word takes that word's color.
#[test]
fn semantic_token_remaps_to_another_word() {
    let fx = Fx::new("theme {\n    accent = purple\n}\nView v { Rectangle a color=accent }\n");
    assert_eq!(fx.color("a", PropKey::Color), rgb(0x9370DB));
}

/// `_palette_tests` 2709-2725 (`palette.alias-to-ref`): a semantic token
/// remapped to a `$token` (`info = $brand`) takes the token's color. Rust's
/// palette override only follows widget-param refs, not theme values, so
/// `$brand` is "not a color" there (G6004).
#[test]
#[ignore = "NEW: Godot `theme { info = $brand }` follows the $token vs Rust G6004 (a palette override cannot point at a theme $value)"]
fn semantic_token_remaps_to_a_theme_ref() {
    let fx = Fx::new("$brand = #00FF00FF\ntheme {\n    info = $brand\n}\nView v { Rectangle b color=info }\n");
    assert_eq!(fx.color("b", PropKey::Color), rgb(0x00FF00));
}

/// `_palette_tests` 2727-2745 (`palette.dark-literals-stay`): the `dark`
/// variant never touches a plain color word — `red` is still tomato.
#[test]
fn dark_variant_keeps_literal_words() {
    let fx = Fx::with_variant("View v { Rectangle r color=red }\n", "dark");
    assert_eq!(fx.color("r", PropKey::Color), rgb(0xFF6347));
}

/// `_palette_tests` 2767-2798 (`palette.transparent`,
/// `palette.opacity-name`, `palette.opacity-hex`, `palette.opacity-ref`,
/// `palette.opacity-in-override`, `palette.opacity-use-site-wins`):
/// `transparent` is all-zero; `/opacity` sets the alpha on a word, a hex
/// and a `$ref`; an override may carry one (`danger = red/0.5`); the use
/// site's suffix wins over the override's.
#[test]
fn transparent_and_the_opacity_suffix() {
    let fx = Fx::new("$brand = #00FF00FF\ntheme {\n    danger = red/0.5\n}\nView v { Row r {\n    Rectangle a color=transparent\n    Rectangle b color=black/0.5\n    Rectangle c color=#FF6347/0.25\n    Rectangle d color=$brand/0.5\n    Rectangle e color=danger\n    Rectangle f color=danger/0.8\n    Button g text=\"x\" {\n        when held background=white/0.2\n    }\n} }\n");
    assert_eq!(fx.color("a", PropKey::Color), Rgba([0, 0, 0, 0]));
    assert_eq!(fx.color("b", PropKey::Color), rgb_a(0x000000, alpha_of(0.5)));
    assert_eq!(fx.color("c", PropKey::Color), rgb_a(0xFF6347, alpha_of(0.25)));
    assert_eq!(fx.color("d", PropKey::Color), rgb_a(0x00FF00, alpha_of(0.5)));
    assert_eq!(fx.color("e", PropKey::Color), rgb_a(0xFF6347, alpha_of(0.5)), "override carries its suffix");
    assert_eq!(fx.color("f", PropKey::Color), rgb_a(0xFF6347, alpha_of(0.8)), "use site wins");
}

/// `_palette_tests` 2800-2802 (`palette.err-opacity-range`): `red/2` is
/// refused — opacity is 0..1.
#[test]
fn opacity_suffix_out_of_range_is_refused() {
    let errs = errors("View v { Rectangle a color=red/2 }\n");
    assert!(errs.iter().any(|d| d.message.contains("0..1")), "{errs:#?}");
}

/// `_palette_tests` 2803-2808 (`palette.err-opacity-not-color`,
/// `palette.err-opacity-align`, `palette.err-opacity-number`): a `/`
/// suffix off a color position is refused — on `fill`, on `center` (G0039)
/// and on a number (`4/2` — Rust's lexer has no `/` after a number: G0001).
#[test]
fn opacity_suffix_off_a_color_position_is_refused() {
    let opfill = codes("View v { Button b text=\"x\", width=fill/0.5 }\n");
    assert!(opfill.contains(&"G0039".to_string()), "{opfill:?}");
    let opalign = codes("View v { Button b text=\"x\", halign=center/0.5 }\n");
    assert!(opalign.contains(&"G0039".to_string()), "{opalign:?}");
    let opnum = codes("View v { Row r spacing=4/2 }\n");
    assert!(opnum.contains(&"G0001".to_string()), "{opnum:?}");
}

/// `_palette_tests` 2810-2812 (`palette.err-not-global`): color words are
/// contextual — `opacity=red` is refused ("a number"), never coerced.
#[test]
fn palette_word_in_a_number_position_is_refused() {
    let errs = errors("View v { Button b text=\"x\", opacity=red }\n");
    assert!(!errs.is_empty(), "opacity=red must not build");
}

/// `_palette_tests` 2814-2816, 2818-2822 (`palette.err-suggests`,
/// `palette.err-bad-alias`): a bare-word typo in a color position is
/// refused AND names the nearest palette word (`charcol` → charcoal,
/// `bloo` → blue; gogo-design §7 "unknown word = build error with a
/// suggestion"). Rust refuses (G2007 / G6004) but suggests nothing.
#[test]
fn color_typo_names_the_nearest_palette_word() {
    let typo = errors("View v { Rectangle a color=charcol }\n");
    assert!(typo.iter().any(|d| d.message.contains("charcoal")), "{typo:#?}");
    let badname = errors("theme {\n    accent = bloo\n}\nView v { Button b text=\"x\" }\n");
    assert!(badname.iter().any(|d| d.message.contains("blue")), "{badname:#?}");
}

/// `_palette_tests` 2823-2825 (`palette.err-bad-override-type`): a palette
/// override takes a color, a name or a `$ref` — `red = 5` is refused
/// (G6004 "needs a color").
#[test]
fn palette_override_needs_a_color() {
    let badtype = codes("theme {\n    red = 5\n}\nView v { Button b text=\"x\" }\n");
    assert!(badtype.contains(&"G6004".to_string()), "{badtype:?}");
}

/// `_palette_tests` 2827-2829 (`palette.err-circular`): `red = green`,
/// `green = red` is a circular chain and is refused. Rust resolves each
/// override to bytes at its own line, so the pair builds and `red` paints
/// the OLD green — no cycle, no diagnostic.
#[test]
fn circular_palette_override_is_refused() {
    let errs = errors("theme {\n    red = green\n    green = red\n}\nView v { Rectangle a color=red }\n");
    assert!(errs.iter().any(|d| d.message.contains("circular")), "{errs:#?}");
}

// ------------------------------------------------------------ _translation_tests

/// `_translation_tests` 2839-2845 (`tr.literal-shows`,
/// `tr.binding-recorded`): with no locale table the English literal shows,
/// and the key travels with the drawn text (Rust's binding IS the key on
/// `Draw::Text`). Also: a digit-leading key (`/4by3`) lexes as a key, not
/// as a number and a unit (2847-2856's lexing half).
#[test]
fn literal_shows_and_the_key_rides_with_the_text() {
    let fx = Fx::new("View v { Label t text=\"new stroke\"/newStroke }\n");
    assert_eq!(fx.drawn_text("new"), ("new stroke".to_string(), "newStroke".to_string()));
    let dig = Fx::new("View v { Label t text=\"4:3\"/4by3 }\n");
    assert_eq!(dig.drawn_text("4:3"), ("4:3".to_string(), "4by3".to_string()));
}

/// `_translation_tests` 2847-2887 (`tr.digit-leading-key`,
/// `tr.locale-at-build`, `tr.missing-falls-back`, `tr.flip-reapplies`,
/// `tr.translate-hit`, `tr.translate-fallback`, `tr.translate-keyback`):
/// with locale `xx` = {trGreet: bonjour, 4by3: "4:3-XX"} the keyed labels
/// show the table's text, a key the table lacks falls back to its literal,
/// clearing the locale shows the literals again, and a code-side lookup
/// follows the same fallback rule (key → literal → the key itself).
/// Rust: `Runtime::set_locale` with a `gogo::Locale`; the code-side
/// lookup is `locale::display` (key → table → literal; Godot's third step,
/// the key's own name, has no caller here).
#[test]
fn locale_table_translates_and_falls_back() {
    static XX: &[(&str, &str)] = &[("trGreet", "hello"), ("trBye", "goodbye"), ("4by3", "4:3")];
    let src = "View v { Column c {\n    Label a text=\"hello\"/trGreet\n    Label b text=\"goodbye\"/trBye\n    Label d text=\"4:3\"/4by3\n} }\n";
    let mut fx = Fx::with_locale(src, Locale::build(XX, &[(0, "bonjour"), (2, "4:3-XX")]));
    assert_eq!(fx.keys.names, ["trGreet", "trBye", "4by3"], "the ids the table above indexes");
    assert_eq!(fx.drawn_text("bonjour"), ("bonjour".to_string(), "trGreet".to_string()), "trGreet translated");
    assert_eq!(fx.drawn_text("goodbye").0, "goodbye", "a key the table lacks falls back to the literal");
    assert_eq!(fx.drawn_text("4:3-XX").0, "4:3-XX", "a digit-leading key indexes the table");
    fx.set_locale(None);
    assert_eq!(fx.drawn_text("hello").0, "hello", "clearing the locale shows the literals again");
    // The translated WIDTH is pinned in tests/locale.rs
    // (`a_keyed_label_is_measured_at_its_translated_width`); here the
    // labels fill their Column.
}

/// `_translation_tests` 2889-2900 (`tr.widget-param`): a keyed string
/// handed to a widget `$param` reaches the body's Label with its key.
#[test]
fn widget_param_carries_the_key_to_the_body() {
    let fx = Fx::new("widget Chip $label=\"?\" {\n    Label t text=$label\n}\nView v { Chip c label=\"keep changes\"/keepChanges }\n");
    assert_eq!(fx.drawn_text("keep"), ("keep changes".to_string(), "keepChanges".to_string()));
}

/// `_translation_tests` 2927-2942 (`tr.same-literal-shares`,
/// `tr.drift-error`): one key may carry the same English twice; a
/// different English for the same key is the drift bug — G8001 at build,
/// naming the first text.
#[test]
fn one_key_one_english_string() {
    let dup = ok("View v { Column c {\n    Label a text=\"save\"/trSave\n    Label b text=\"save\"/trSave\n} }\n");
    assert!(dup.diags.is_empty(), "{:#?}", dup.diags);
    let drift = errors("View v { Column c {\n    Label a text=\"save\"/trKeep\n    Label b text=\"Save it\"/trKeep\n} }\n");
    assert!(drift.iter().any(|d| d.code == "G8001" && d.message.contains("\"save\"")), "{drift:#?}");
}

/// `_translation_tests` 2944-2947, 2953-2959 (`tr.crossfile-first-ok`,
/// `tr.edit-literal-ok`): the first file to use a key builds; editing the
/// literal in place (the same file built again with new text) is legal.
#[test]
fn first_use_and_in_place_edit_of_a_key_build() {
    ok("View v { Label t text=\"one\"/trCross }\n");
    ok("View v { Label t text=\"three\"/trEdit }\n");
    ok("View v { Label t text=\"four\"/trEdit }\n");
}

/// `_translation_tests` 2948-2952 (`tr.crossfile-drift`): a SECOND file
/// keying different English under a key the first file already carries is
/// refused, naming the first file. Rust checks drift only inside one
/// compile (a page and its `uses`); two pages of one build never meet.
#[test]
fn cross_file_key_drift_is_refused() {
    let dir = std::env::temp_dir().join(format!("gogo_theme_crossfile_{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    std::fs::write(dir.join("a.gogo"), "View v { Label t text=\"one\"/trCross }\n").expect("write a");
    std::fs::write(dir.join("b.gogo"), "View v { Label t text=\"two\"/trCross }\n").expect("write b");
    let result = gogo::build_pages(&dir, &dir.join("out.rs"), gogo::BuildOptions::new(&dir));
    let _ = std::fs::remove_dir_all(&dir);
    let err = result.expect_err("the second file must fail the build");
    assert!(err.contains("G8001") && err.contains("a.gogo"), "{err}");
}

/// `_translation_tests` 2961-2963 (`tr.err-theme-value`): keys live on
/// displayed text only — `$title = "Fantasia"/appTitle` is refused
/// (G8001, GPAR89 round 6; the key used to be recorded silently).
#[test]
fn theme_value_never_carries_a_key() {
    let errs = errors("$title = \"Fantasia\"/appTitle\nView v { Label t text=$title }\n");
    assert!(!errs.is_empty(), "a keyed theme value must not build");
}

/// `_translation_tests` 2964-2967 (`tr.err-non-string`,
/// `tr.err-key-not-ident`): `12/trNum` and `"x"/"y"` are refused — the
/// key suffix follows a string only, and is a bare word (Rust: the stray
/// `/` is G0001).
#[test]
fn key_suffix_misuse_is_refused() {
    let numk = codes("View v { Label t text=\"x\", min_width=12/trNum }\n");
    assert!(numk.contains(&"G0001".to_string()), "{numk:?}");
    let badk = codes("View v { Label t text=\"x\"/\"y\" }\n");
    assert!(badk.contains(&"G0001".to_string()), "{badk:?}");
}

/// `_translation_tests` 2968-2969 (`tr.err-uses-path`): a key on a `uses`
/// path is refused ("uses needs one quoted path"). With the include
/// present, Rust refuses it too (G0023, E4).
#[test]
fn uses_path_never_carries_a_key() {
    let loader = |path: &str| if path == "styles/theme.gogo" { Some("$pad = 4\n".to_string()) } else { None };
    let c = compile("uses \"styles/theme.gogo\"/trPath\nView v { Button b text=\"x\" }\n", &loader);
    assert!(gogo::has_errors(&c.diags), "a keyed uses path must not build: {:#?}", c.diags);
}

/// `_translation_tests` 2971-2975 (`tr.opacity-suffix-intact`): the color
/// `/opacity` suffix shares the `/` with the string key suffix and is
/// untouched by it.
#[test]
fn color_opacity_suffix_is_intact_beside_the_key_suffix() {
    let fx = Fx::new("View v { Rectangle rr color=#FF0000/0.5 }\n");
    assert_eq!(fx.color("rr", PropKey::Color), rgb_a(0xFF0000, alpha_of(0.5)));
}

// ---------------------------------------------------------- _pigment_alpha_tests

/// `_pigment_alpha_tests` 2977-2980 (`pigment.alpha0-structured-refused`,
/// `pigment.alpha0-border-refused`): a static `background=` /
/// `border_color=` whose alpha is 00 but whose channels carry color is the
/// XAML `#AARRGGBB` copy trap (alpha first there, last here) — G2004.
#[test]
fn invisible_pigment_is_refused() {
    assert!(codes("View v { Panel p background=#77000000 }\n").contains(&"G2004".to_string()));
    assert!(codes("View v { Panel p border_color=#4400FF00, border_thickness=2 }\n").contains(&"G2004".to_string()));
}

/// `_pigment_alpha_tests` 2981-2984 (`pigment.canonical-transparent-ok`,
/// `pigment.gradient-stop-alpha0-ok`): the canonical transparent
/// `#00000000` and a gradient's fade-out endpoint stay legal.
#[test]
fn canonical_transparent_and_fade_stops_are_legal() {
    ok("View v { Panel p background=#00000000 { Label text=\"x\" } }\n");
    ok("View v { Gradient g stops=[#FFFFFF00, #FFFFFFFF], 20x20 }\n");
}

// -------------------------------------------------------------- _for_style_tests

/// The Godot suite's `FADER_SRC` (run_tests.gd 3385-3403), verbatim: a
/// `control` with a script, contract states, signals and automatic slots.
const FADER_SRC: &str = r#"
control Fader $value=0, $min=0, $max=100, $step=0 {
    script "res://addons/gogo/test/fixtures/fader_behavior.gd"
    state_builtin hovered, focused, disabled
    state dragging
    signal drag_started, drag_ended
    transition duration=0.12, easing=cubic_out
    anim releasePop {
        slot.thumb.translate.y from=-3, to=0, duration=0.2
    }
    Layers {
        slot track width=fill, height=24 {
            slot progress automatic=length by $value in $min..$max { Rectangle color=#4488FFFF }
            slot thumb automatic=position by $value in $min..$max { Circle 12x12, color=#FFFFFFFF }
        }
        slot label { Label text=live($value) }
    }
}
"#;

/// Godot's tails mix top-level `style` lines with root instance lines;
/// Rust wants the styles above the `View` and the instances inside it.
fn with_control(control: &str, tail: &str) -> String {
    let mut top = String::new();
    let mut body = String::new();
    let mut depth: i32 = 0;
    for line in tail.lines() {
        let top_level = depth > 0 || line.trim_start().starts_with("style ");
        depth += line.matches('{').count() as i32 - line.matches('}').count() as i32;
        let into = if top_level { &mut top } else { &mut body };
        into.push_str(line);
        into.push('\n');
    }
    format!("{control}{top}View v {{\n{body}}}\n")
}

fn with_fader(tail: &str) -> String {
    with_control(FADER_SRC, tail)
}

/// `_for_style_tests` 3761-3797 (`fstyle.ok`, `fstyle.slot-box`,
/// `fstyle.slot-size`, `fstyle.when-slot-motion`,
/// `fstyle.when-slot-motion-back`, `fstyle.instance-path`): a `for=Fader`
/// style restyles the control's slots by path (`slot.track.background`),
/// its `when dragging` targets a slot's scale through the transition and
/// comes back, and an instance line takes the same paths. Builds clean in
/// Godot; the slot facts to pin once the feature exists: track background
/// #332211, thumb min size 8, thumb scale 1.2 while dragging then 1.0,
/// instance `slot.track.corner_radius=3` reads 3.
#[test]
#[ignore = "GP40: `style X for=<Control>` + `slot.<name>.<prop>` paths not ported (the fixture's `slot.thumb.translate.y` anim lane is G0063 first — GP32 owes slot. lanes)"]
fn for_style_restyles_slots_by_path() {
    let styled = build(&with_fader("style ThinFader for=Fader, slot.track.height=4, slot.track.background=#332211FF, slot.thumb.size=8 {\n    transition duration=0.05\n    when dragging slot.thumb.scale=1.2\n    when focused slot.track.border_color=#FFFFFFFF, slot.track.border_thickness=1\n}\nFader vol .ThinFader 200x24, value=50"));
    assert!(styled.diags.is_empty(), "{:#?}", styled.diags);
    let inst = build(&with_fader("Fader f 200x24, slot.track.corner_radius=3"));
    assert!(inst.diags.is_empty(), "{:#?}", inst.diags);
}

/// `_for_style_tests` 3799-3833 (`fstyle.d16-builds`, `fstyle.d16-lint`,
/// `fstyle.d2-one-type`, `fstyle.d2-two-types-silent`): a `for=` style
/// answering a universal state the control never declared builds with a
/// lint ("doesn't declare 'hovered'"); a plain style used by ONE control
/// type suggests `for=`; used by two, it stays silent.
#[test]
#[ignore = "GP40: `for=` checked styles and their lints not ported (the Fader fixture's slot. anim lane is G0063 first — GP32 owes it)"]
fn for_style_lints() {
    let d16 = build("control Chip2 $value=0 {\n    script \"res://addons/gogo/test/fixtures/fader_behavior.gd\"\n    state marked\n    Layers {\n        slot mark height=6 { }\n    }\n}\nstyle ChipHover for=Chip2 { when hovered opacity=0.9 }\nView v { Chip2 c .ChipHover }\n");
    assert!(!gogo::has_errors(&d16.diags), "{:#?}", d16.diags);
    assert!(d16.diags.iter().any(|d| d.severity == Severity::Warning && d.message.contains("doesn't declare 'hovered'")), "{:#?}", d16.diags);
    let one = build(&with_fader("style Ghosty { when dragging opacity=0.8 }\nFader a .Ghosty 100x24"));
    assert!(one.diags.iter().any(|d| d.severity == Severity::Warning && d.message.contains("add `for=Fader`")), "{:#?}", one.diags);
    let two = build(&format!("{FADER_SRC}control Mini $value=0 {{\n    script \"res://addons/gogo/test/fixtures/fader_behavior.gd\"\n    state dragging\n    Layers {{\n        slot track height=8 {{ }}\n    }}\n}}\nstyle Ghosty {{ when dragging opacity=0.8 }}\nView v {{\n Fader a .Ghosty 100x24\n Mini b .Ghosty 100x8\n}}\n"));
    assert!(!gogo::has_errors(&two.diags), "{:#?}", two.diags);
    assert!(!two.diags.iter().any(|d| d.message.contains("add `for=")), "{:#?}", two.diags);
}

/// `_for_style_tests` 3835-3896 (`fstyle.err-d1`, `fstyle.err-d11-order`,
/// `fstyle.err-uses-reach`, `fstyle.err-widget-target`,
/// `fstyle.err-d4-typo`, `fstyle.err-d4-unopted`, `fstyle.err-slot-typo`,
/// `fstyle.err-d3-code`, `fstyle.err-static-motion`,
/// `fstyle.err-content-route`, `fstyle.err-when-vocab`, `fstyle.err-d10`):
/// the `for=` error family — each misuse is refused with the message Godot
/// gives (slot path without `for=`, `for=` not first, unknown control,
/// widget target, state typo with the suggestion, un-opted built-in,
/// slot typo, driven slot prop G3002, static motion, content route, slot
/// when-vocab, style applied to the wrong kind).
#[test]
#[ignore = "GP40: `for=` checked styles not ported — none of these twelve fences exist (Fader fixture: G0063 on its slot. anim lane, GP32)"]
fn for_style_error_family() {
    let has = |src: String, fragment: &str| {
        let errs: Vec<Diagnostic> = compile(&src, &|_| None).diags.into_iter().filter(|d| d.severity == Severity::Error).collect();
        assert!(errs.iter().any(|d| d.message.contains(fragment)), "want {fragment:?} in {errs:#?}");
    };
    has("style Bad1 slot.track.height=2\nView v { Rectangle r color=#111111FF }\n".to_string(), "needs to know which control");
    has(with_fader("style Bad2 slot.track.height=2, for=Fader\nFader f 100x24"), "`for=` comes first");
    has("style Bad3 for=Knob, slot.track.size=64\nView v { Rectangle r color=#111111FF }\n".to_string(), "`uses`");
    has("widget Card { Column { slot } }\nstyle Bad4 for=Card, slot.header.height=2\nView v { Rectangle r color=#111111FF }\n".to_string(), "is a widget");
    has(with_fader("style Bad5 for=Fader { when draging opacity=0.5 }\nFader f 100x24"), "no state 'draging' on Fader");
    has(with_fader("style Bad6 for=Fader { when held opacity=0.5 }\nFader f 100x24"), "Fader doesn't use 'held'");
    has(with_fader("style Bad7 for=Fader, slot.thmb.size=8\nFader f 100x24"), "no slot 'thmb' on Fader");
    let d3 = build(&with_fader("style Bad8 for=Fader, slot.progress.width=10\nFader f 100x24"));
    assert!(d3.diags.iter().any(|d| d.code == "G3002" && d.message.contains("moves by itself")), "{:#?}", d3.diags);
    has(with_fader("style Bad9 for=Fader, slot.thumb.scale=1.2\nFader f 100x24"), "base look declares no transform");
    has(with_fader("style Bad10 for=Fader, slot.label.font_size=12\nFader f 100x24"), "restyling content IS filling");
    has(with_fader("style Bad11 for=Fader { when dragging slot.label.text=\"x\" }\nFader f 100x24"), "isn't a slot when-response");
    has(with_fader("style Thin for=Fader, slot.track.height=2\nButton b .Thin text=\"x\""), "is for Fader — applied here to Button");
}

/// `_for_style_tests` 3898-3904, 3913-3914 (`fstyle.err-d15-style`,
/// `fstyle.err-d15-inline`): contract features (slot paths) on a NATIVE
/// built-in such as Slider are G5106, from a `for=Slider` style and from
/// an inline `slot.thumb.size=`.
#[test]
#[ignore = "GP40: slot paths / G5106 native-built-in fence not ported (slot.thumb.size= is a G2001 warning)"]
fn slot_paths_on_a_native_builtin_are_refused() {
    assert!(codes("style ThinSlider for=Slider, slot.track.height=2\nView v { Slider s .ThinSlider }\n").contains(&"G5106".to_string()));
    assert!(codes("View v { Slider s slot.thumb.size=10 }\n").contains(&"G5106".to_string()));
}

/// `_for_style_tests` 3905-3912 (`fstyle.err-d15-state`): `when held` in a
/// plain style applied to a Slider builds — `held` is native-raisable on a
/// Slider (its drag brackets it, D181).
#[test]
fn when_held_on_a_slider_builds() {
    let c = ok("style Ghost { when held opacity=0.8 }\nView v { Slider s .Ghost }\n");
    assert!(c.diags.is_empty(), "{:#?}", c.diags);
}

/// `_for_style_tests` 3916-3923 (`fstyle.err-d14-wording`): an unknown,
/// non-retired state word (`wobbling`) is refused and the message lists the
/// states that do exist (`held`, `selected` among them). Godot names the
/// kind too ("Button has no state"); Rust's G0036 lists the built-ins
/// without the kind.
#[test]
fn unknown_state_word_lists_the_real_ones() {
    let errs = errors("style Ghost { when wobbling opacity=0.8 }\nView v { Button b .Ghost text=\"x\" }\n");
    let d = errs.iter().find(|d| d.code == "G0036").unwrap_or_else(|| panic!("{errs:#?}"));
    assert!(d.message.contains("wobbling") && d.message.contains("held") && d.message.contains("selected"), "{}", d.message);
}

/// `_for_style_tests` 3924-3928 (`fstyle.err-retired-state`): the retired
/// spelling `pressed` is refused with the one-keystroke fix — `held`
/// (Godot: "use `when held`"; Rust: G0035 "`pressed` is spelled `held`").
#[test]
fn retired_state_spelling_names_the_fix() {
    let errs = errors(&with_fader("style Old { when pressed opacity=0.5 }\nButton b .Old text=\"x\""));
    assert!(errs.iter().any(|d| d.code == "G0035" && d.message.contains("held")), "{errs:#?}");
}

/// `_for_style_tests` 3930-3951 (`fstyle.extends-same-ok`,
/// `fstyle.err-extends-plain-from-for`, `fstyle.err-extends-cross`,
/// `fstyle.extends-for-from-plain-ok`): the `extends=` matrix for `for=`
/// styles — same control ok, a plain style may not inherit slot paths, a
/// `for=` style may not inherit another control's slots, a `for=` style
/// may extend a plain one.
#[test]
#[ignore = "GP40: `style X extends=` and `for=` styles not ported (Fader fixture: G0063 on its slot. anim lane, GP32)"]
fn for_style_extends_matrix() {
    let same = build(&with_fader("style BaseF for=Fader, slot.track.height=4\nstyle ThinF for=Fader, extends=BaseF, slot.thumb.size=6\nFader f .ThinF 100x24"));
    assert!(same.diags.is_empty(), "{:#?}", same.diags);
    let plain = errors(&with_fader("style BaseF for=Fader, slot.track.height=4\nstyle Derived extends=BaseF, opacity=0.9\nFader f 100x24"));
    assert!(plain.iter().any(|d| d.message.contains("inherits slot paths")), "{plain:#?}");
    let cross = errors(&with_fader("style SliderThin for=Slider, slot.track.height=2\nstyle FaderThin for=Fader, extends=SliderThin\nFader f 100x24"));
    assert!(cross.iter().any(|d| d.message.contains("another control's slots")), "{cross:#?}");
    let from_plain = build(&with_fader("style PlainBase opacity=0.95\nstyle ForF for=Fader, extends=PlainBase, slot.track.height=4\nFader f .ForF 100x24"));
    assert!(from_plain.diags.is_empty(), "{:#?}", from_plain.diags);
}

// ----------------------------------------------------------------- _radial_tests

/// The Godot suite's `KNOB_SRC` (run_tests.gd 3958-3979), verbatim.
const KNOB_SRC: &str = r#"
control Knob $value=0, $min=0, $max=1, $step=0, min_size=48x48 {
    script "res://addons/gogo/test/fixtures/knob_behavior.gd"
    state_builtin hovered, focused, disabled
    state dragging
    signal drag_started, drag_ended
    transition duration=0.15, easing=cubic_out
    Circle color=#333333FF {
        slot track width=fill, height=fill {
            slot progress automatic=arc(from=-225, sweep=270) by $value in $min..$max {
                Arc thickness=4, color=#4488FFFF
            }
            slot thumb automatic=angle(from=-225, sweep=270) by $value in $min..$max {
                Circle 8x8, color=#FFFFFFFF
            }
        }
        slot label { Label text=live($value), halign=center, valign=center }
    }
}
"#;

fn with_knob(tail: &str) -> String {
    with_control(KNOB_SRC, tail)
}

/// `_radial_tests` 3983-3991 (`radial.arc-ok`, `radial.arc-props`): `Arc`
/// is a primitive with `from`, `sweep`, `thickness` (the stroke — not
/// `width`) and `color`; it builds clean and keeps its numbers.
#[test]
fn arc_primitive_builds_with_its_props() {
    let arc = build("View v { Arc a 48x48, from=-90, sweep=180, thickness=3, color=#FF0000FF }\n");
    assert!(arc.diags.is_empty(), "{:#?}", arc.diags);
    let fx = Fx::new("View v { Arc a 48x48, from=-90, sweep=180, thickness=3, color=#FF0000FF }\n");
    assert_eq!(fx.color("a", PropKey::Color), rgb(0xFF0000));
}

/// `_radial_tests` 3993-3997 (`radial.wedge-inside`, `radial.wedge-outside`,
/// `radial.wedge-full`, `radial.wedge-empty`): one wedge formula serves
/// drawing and hits. The four Godot cases, as (point, from°, sweep°) →
/// inside: ((-1,0), -225, 135) → true; ((1,0), -225, 135) → false;
/// ((1,0), -225, 360) → true; ((-1,0), -225, 0) → false. Rust has no arc
/// wedge yet; the body pins the precondition (an `automatic=arc` slot
/// compiles) and the four cases wait for the helper's name.
#[test]
fn wedge_formula_serves_draw_and_hit() {
    let c = build(&with_knob("Knob k 48x48, value=0.5"));
    assert!(c.diags.is_empty(), "{:#?}", c.diags);
}

/// `_radial_tests` 3999-4015 (`radial.zero-ok`, `radial.zero-wedge-fraction`,
/// `radial.zero-wedge-hits-nothing`): at `value=0` the arc wedge is armed
/// with fraction 0 (the first driver write lands) and a point on the arc
/// (24, 4 in a 48 box — the -90° point) hits nothing.
#[test]
fn knob_at_zero_reveals_nothing() {
    let fx = Fx::new(&with_knob("Knob z 48x48, value=0, halign=left, valign=top"));
    let progress = fx.id("z/progress");
    assert_ne!(gogo::draw::hit(&fx.page, &fx.res, &fx.rects, 24.0, 4.0, true), Some(progress), "the empty wedge hits nothing");
}

/// `_radial_tests` 4017-4056 (`radial.knob-ok`, `radial.aperture`,
/// `radial.wedge-armed`, `radial.wedge-clips-children`, `radial.seat-mid`,
/// `radial.live-label`): at `value=0.5` the wedge runs -225° for 270° at
/// fraction 0.5 and clips its children; the `angle` slot seats the 8x8
/// thumb on the host circle at -90° (center (24, 4), box at (20, 0)); the
/// live label shows "0.5".
#[test]
fn knob_mid_value_geometry() {
    let fx = Fx::new(&with_knob("Knob k 48x48, value=0.5, halign=left, valign=top"));
    let thumb = fx.rect("k/thumb");
    assert!((thumb.x + thumb.w / 2.0 - 24.0).abs() < 1.0 && (thumb.y + thumb.h / 2.0 - 4.0).abs() < 1.0, "{thumb:?}");
    let progress = fx.rect("k/progress");
    assert!(fx.draws.contains(&Draw::ClipPush(progress)), "the wedge clips its children: {:#?}", fx.draws);
    assert_eq!(fx.drawn_text("0.5").0, "0.5");
}

/// `_radial_tests` 4073-4081 (`radial.real-input-ok`): a Knob instance
/// line with its own size (`Knob k 96x96`) builds clean.
#[test]
fn knob_instance_line_builds() {
    let c = build(&with_knob("Knob k 96x96, value=0"));
    assert!(c.diags.is_empty(), "{:#?}", c.diags);
}

/// `_radial_tests` 4082-4085 (`radial.instance-size-args`): a positional
/// `WxH` on an INSTANCE line sizes the instance (Godot once dropped it).
/// Ported on a plain widget — the size arg is the same instance-line
/// member on a `widget` and on a `control`.
#[test]
fn instance_line_size_arg_sizes_the_instance() {
    let fx = Fx::new("widget W {\n    Layers body { Rectangle r 10x10 }\n}\nView v { W k 96x96, halign=left, valign=top }\n");
    let root = fx.res.children(&fx.page, 0)[0];
    let r = fx.rects[root as usize];
    assert_eq!((r.w, r.h), (96.0, 96.0), "{r:?}");
}

/// `_radial_tests` 4107-4119 (`radial.err-aperture-off-radial`,
/// `radial.err-driven-sweep`, `radial.err-widget-aperture`): the aperture
/// config belongs to the radial kinds only ("only the radial kinds take
/// config"); a driven radial prop restyled from a `for=` style is G3002;
/// an aperture on a widget slot teaches `control`.
#[test]
fn radial_error_family() {
    let off = errors(&with_knob("Knob k 48x48").replace("automatic=arc(from=-225, sweep=270)", "automatic=length(from=-225)"));
    assert!(off.iter().any(|d| d.message.contains("radial")), "{off:#?}");
    let sweep = build(&with_knob("style Wide for=Knob, slot.progress.sweep=90\nKnob k 48x48"));
    assert!(sweep.diags.iter().any(|d| d.code == "G3002"), "{:#?}", sweep.diags);
    let wa = errors("widget W { Layers { slot p from=-90 } }\nView v { W w }\n");
    assert!(wa.iter().any(|d| d.message.contains("goes inside the kind")), "{wa:#?}");
}
