//! `gogo` — the UI language, pure Rust: no wgpu, no winit, no OS (the one
//! exception is a host's choice: the `hot-reload` feature reads the
//! host's page folder and starts a thread to watch it).
//! In: `.gogo` text (at build time through `build_pages`, or at runtime
//! for hot reload, `reload`), touches, the current theme/arrangement. Out: a draw
//! list. The host program supplies what gogo cannot know (`CompileOptions`,
//! `BuildOptions`). Design: `docs/gogo-design.md`.

// A deprecated item is a build error in this crate. The arena's
// index-range accessors were fenced this way first and then removed
// (todo-bugs.md B37: a range test stood in for a subtree walk four
// times; `Node::subtree_end` is a newtype only page.rs can read, the
// walks are `Page::descendants` and `Resolved::tree`). The fence stays for the
// next item that has to go: a new use fails to build, never warns.
#![deny(deprecated)]

pub mod anim;
pub mod ast;
pub mod atlas;
pub mod color;
pub mod compile;
pub mod diag;
pub mod draw;
pub mod font_style;
pub mod frame;
pub mod emit;
pub mod input;
pub mod layout;
pub mod lexer;
pub mod locale;
pub mod page;
pub mod parser;
mod placement;
pub mod reload;
pub mod runtime;
pub mod tree;

pub use compile::{CompileOptions, Compiled, compile, compile_with};
pub use diag::{Diagnostic, Severity, has_errors};
pub use draw::{Draw, Fit, GradientKind, Sampling, TextAlign, draw, hit};
pub use frame::{Frame, KeptFrame};
pub use input::{Behavior, Claim, Cx, Event, EventKind, Input, Key, KeyAction, MOUSE_ID, Mods, Phase, Touch};
pub use layout::{FixedAdvance, LayoutBuffers, Rect, TextMeasure, layout, layout_into};
pub use locale::{KeyName, Keys, Locale};
pub use runtime::{AnimName, ArrangementName, ThemeName};
pub use page::{ArrangementId, BASE, BEHAVIOR_SLOTS, BUILTIN_BEHAVIORS, BehaviorSpec, ImageKey, ImageSlice, Images, Kind, KeyId, KEYS_ARROWS, KEYS_ENTER, KEYS_HOME_END, KEYS_SPACE, NO_IMAGE, NO_KEY, NO_NODE, NO_STR, NodeId, Orientation, Page, Payload, PropKey, Color_Straight_Srgb_4xU8_RGBA, ScreenClass, StaticPage, StrId, Value};
pub use anim::{Easing, Finish, Motion};
pub use runtime::{Resolved, Runtime, resolve, resolve_animated};

use std::path::{Path, PathBuf};

/// Compiles one `.gogo` file; `uses` paths resolve relative to it.
pub fn compile_file(path: &Path) -> Result<Compiled, String> {
    let source = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let dir: PathBuf = path.parent().map(Path::to_path_buf).unwrap_or_default();
    let loader = move |rel: &str| std::fs::read_to_string(dir.join(rel)).ok();
    Ok(compile(&source, &loader))
}

/// A file is a page when a `View`/`Panel` root opens a line at column 0;
/// fragment files (themes, widgets) and test fixtures with another root
/// are skipped with a note.
/// Every file a `.gogo` under `dir` (any depth) points at with
/// `uses "…"`, canonicalized. Needed to aim the not-a-page warning: a
/// rootless file someone `uses` is a widget/style library compiled
/// INTO its users — fine, and silent; a rootless file NOBODY uses is
/// the case worth a shout (a dead file, or a new page whose root line
/// is wrong — the build would otherwise drop that page with a green
/// result).
fn used_files(dir: &Path) -> Vec<PathBuf> {
    let mut used = Vec::new();
    let mut dirs = vec![dir.to_path_buf()];
    while let Some(d) = dirs.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                dirs.push(p);
            } else if p.extension().is_some_and(|x| x == "gogo") {
                let Ok(text) = std::fs::read_to_string(&p) else { continue };
                for line in text.lines() {
                    if let Some(rel) = line.trim().strip_prefix("uses ").and_then(|r| r.trim().strip_prefix('"')).and_then(|r| r.split('"').next()) {
                        if let Ok(c) = d.join(rel).canonicalize() {
                            used.push(c);
                        }
                    }
                }
            }
        }
    }
    used
}

fn is_page(source: &str) -> bool {
    source.lines().any(|l| l.starts_with("View ") || l.starts_with("View{") || l == "View" || l.starts_with("Panel ") || l.starts_with("Panel{") || l == "Panel")
}

/// What `build_pages` needs from the host beyond the folders.
#[derive(Clone, Copy)]
pub struct BuildOptions<'a> {
    /// The folder `res://` resolves against: `source="res://x/y.png"` =
    /// `res_root/x/y.png` (the scheme is gogo's, required on every asset
    /// path — G0071). A missing file fails the build.
    pub res_root: &'a Path,
    /// The face text falls back to: the first named font whose key contains
    /// this text goes first in `FONTS`. `None` = the first font in key
    /// order.
    pub default_font: Option<&'a str>,
    /// A marker file name: a folder holding one (or any folder above it,
    /// under `res_root`) is fenced off — a page naming an asset inside it
    /// is a build ERROR, so unshipped art is never embedded silently.
    /// `None` = no fence. Default `Some(".ignore")`.
    pub ignore_marker: Option<&'a str>,
    /// Passed to every page's compile.
    pub compile: CompileOptions<'a>,
    /// Keys CODE looks up at runtime that no page may declare — (name,
    /// English), joined to `KEYS` AFTER every page compiled: a page's
    /// English for the same key wins without a word (the page literal is
    /// what the shipped screen showed; this list is the translators'
    /// reference text). Needed for captions that come from data tables (a
    /// filter's property and preset names), which are translatable only as
    /// keys.
    pub runtime_keys: &'a [(&'a str, &'a str)],
    /// `res://` images CODE shows (`gogo_image!("res://…")` sites the
    /// host's build.rs scanned): they join the ONE image table — the same
    /// `ImageKey` space and embed as a page's `source=` — and the
    /// generated `code_image(path)` hands a site its key. One registry:
    /// a PNG a page also names is embedded and decoded once.
    pub code_images: &'a [&'a str],
}

impl<'a> BuildOptions<'a> {
    /// The defaults with `res_root` set: `.ignore` fence, no default
    /// font, no host options.
    pub fn new(res_root: &'a Path) -> Self {
        BuildOptions { res_root, default_font: None, ignore_marker: Some(".ignore"), compile: CompileOptions::default(), runtime_keys: &[], code_images: &[] }
    }
}

/// `compile_file` with the host's options.
pub fn compile_file_with(path: &Path, options: CompileOptions<'_>) -> Result<Compiled, String> {
    let source = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let dir: PathBuf = path.parent().map(Path::to_path_buf).unwrap_or_default();
    let loader = move |rel: &str| std::fs::read_to_string(dir.join(rel)).ok();
    let name: String = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    Ok(compile_with(&source, &loader, CompileOptions { source_name: &name, ..options }))
}

/// For `build.rs`: compiles every `*.gogo` in `dir` into one Rust file at
/// `out` (one `pub mod <file stem>` each), plus `PAGES` (name → page), the
/// translation keys (`KEYS`, `enum Key` — one table for every page, see
/// `emit::emit_keys`) and the asset tables: every image/font path a page
/// names, embedded from `options.res_root`. Errors are returned as one
/// string for `panic!` — a bad page fails the build with file:line.
/// The built-in edit bar's source (`edit_bar.gogo`, embedded) —
/// `build_pages` compiles it into every app as module `gogo_edit_bar`,
/// so a host composes the bar as a page (see the file's header).
pub const EDIT_BAR_GOGO: &str = include_str!("edit_bar.gogo");
/// The module/page name `build_pages` gives the built-in edit bar.
pub const EDIT_BAR_MODULE: &str = "gogo_edit_bar";

pub fn build_pages(dir: &Path, out: &Path, options: BuildOptions<'_>) -> Result<(), String> {
    let res_root: &Path = options.res_root;
    // One key table for the whole build (`options.compile.keys` from the
    // host is honored; else this one).
    let own_keys = std::cell::RefCell::new(locale::Keys::default());
    // One image table too: `source=` paths are one `ImageKey` space.
    let own_images = std::cell::RefCell::new(page::Images::default());
    let mut options = options;
    if options.compile.keys.is_none() {
        options.compile.keys = Some(&own_keys);
    }
    if options.compile.images.is_none() {
        options.compile.images = Some(&own_images);
    }
    // What an `atlas` line reads (its picture's header, a packer's JSON):
    // a `res://` file under the resource root, refused under an `.ignore`
    // fence like every embedded asset.
    let read_asset = |key: &str| -> Result<Vec<u8>, String> {
        let rel = key.strip_prefix("res://").ok_or_else(|| format!("`{key}` is not a `res://` path"))?;
        let file = res_root.join(rel);
        if let Some(marker) = options.ignore_marker {
            if let Some(ignore) = ignore_file_above(&file, res_root, marker) {
                return Err(format!("it sits under {} — delete the marker to ship that folder", ignore.display()));
            }
        }
        std::fs::read(&file).map_err(|e| format!("{}: {e}", file.display()))
    };
    let compile_options: CompileOptions<'_> = CompileOptions { assets: options.compile.assets.or(Some(&read_asset)), ..options.compile };
    let used: Vec<PathBuf> = used_files(dir);
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().map(|x| x == "gogo").unwrap_or(false)).collect();
    entries.sort();
    let mut source = String::from("// Generated by gogo_ui::build_pages — do not edit.\n");
    let mut errors = String::new();
    let mut modules: Vec<String> = Vec::new();
    let mut fonts: Vec<String> = Vec::new();
    // Every compiled page, for the build-wide `<Widget>Names` structs.
    let mut compiled_pages: Vec<(String, Page)> = Vec::new();
    // Every page with its file, the files it `uses` and what they all say:
    // the `BUILD` table a hot reload compares the files with.
    let mut built: Vec<emit::BuiltPageRow> = Vec::new();
    for path in &entries {
        let module = path.file_stem().and_then(|s| s.to_str()).unwrap_or("page").replace('-', "_");
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        if !is_page(&text) {
            // A widget/style library (something `uses` it): compiled
            // into its users, no standalone page to emit — quiet.
            let canon = path.canonicalize().unwrap_or_else(|_| path.clone());
            if !used.contains(&canon) {
                println!("cargo:warning={}: file is unused and not a View/Panel page — point a page at it with `uses \"{}\"`, give it a View/Panel root to make it a page, or delete it", path.display(), path.file_name().map(|n| n.to_string_lossy()).unwrap_or_default());
            }
            continue;
        }
        let compiled = compile_file_with(path, compile_options)?;
        {
            let mut stamp = reload::TextStamp::of(&text);
            for used in &compiled.uses {
                stamp.used(used, std::fs::read_to_string(dir.join(used)).ok().as_deref());
            }
            let file: String = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            built.push(emit::BuiltPageRow { module: module.clone(), file, uses: compiled.uses.clone(), text: stamp.finish() });
        }
        // Fonts: every `res://` string naming one (G0071 makes the scheme
        // mandatory, so a label whose TEXT ends in ".ttf" never reaches
        // the embed table). Images come from the shared `Images` table —
        // the compiler interns every `source=` there.
        for s in &compiled.page.strings {
            if !s.starts_with("res://") {
                continue;
            }
            let lower = s.to_ascii_lowercase();
            if compile::FONT_EXTS.iter().any(|e| lower.ends_with(e)) && !fonts.contains(s) {
                fonts.push(s.clone());
            }
        }
        modules.push(module.clone());
        for d in &compiled.diags {
            let line = format!("{}:{}: {} {}: {}\n", path.display(), d.line, if d.severity == Severity::Error { "error" } else { "warning" }, d.code, d.message);
            if d.severity == Severity::Error {
                errors.push_str(&line);
            } else {
                println!("cargo:warning={}", line.trim_end());
            }
        }
        println!("cargo:rerun-if-changed={}", path.display());
        match emit::emit_rust(&compiled.page, &module) {
            Ok(code) => source.push_str(&code),
            Err(e) => errors.push_str(&format!("{}: {e}", path.display())),
        }
        source.push('\n');
        compiled_pages.push((module, compiled.page));
    }
    // The built-in edit bar page, compiled with the app's key table so
    // its labels join the app's translation keys (`cut`, `copy`,
    // `paste`, `selectAll`) — an app page never has to author it.
    {
        let compiled = compile_with(EDIT_BAR_GOGO, &|_: &str| None::<String>, CompileOptions { source_name: "edit_bar.gogo (built-in)", ..compile_options });
        for d in &compiled.diags {
            let line = format!("edit_bar.gogo (built-in):{}: {} {}: {}\n", d.line, if d.severity == Severity::Error { "error" } else { "warning" }, d.code, d.message);
            if d.severity == Severity::Error {
                errors.push_str(&line);
            } else {
                println!("cargo:warning={}", line.trim_end());
            }
        }
        modules.push(EDIT_BAR_MODULE.to_string());
        // The edit bar's text is inside gogo: it has no file to follow.
        built.push(emit::BuiltPageRow { module: EDIT_BAR_MODULE.to_string(), file: String::new(), uses: Vec::new(), text: 0 });
        match emit::emit_rust(&compiled.page, EDIT_BAR_MODULE) {
            Ok(code) => source.push_str(&code),
            Err(e) => errors.push_str(&e),
        }
        source.push('\n');
        compiled_pages.push((EDIT_BAR_MODULE.to_string(), compiled.page));
    }
    // One `<Widget>Names` struct per widget the build instances.
    {
        let pages: Vec<(&str, &Page)> = compiled_pages.iter().map(|(m, p)| (m.as_str(), p)).collect();
        match emit::emit_widget_names(&pages) {
            Ok(code) => source.push_str(&code),
            Err(e) => errors.push_str(&e),
        }
    }
    // Every page by file stem, for a `--page <stem>` pick at runtime.
    source.push_str("/// (file stem, page) for every page in `ui/`.\npub static PAGES: &[(&str, &gogo_ui::page::StaticPage)] = &[\n");
    for m in &modules {
        source.push_str(&format!("    ({m:?}, &{m}::PAGE),\n"));
    }
    source.push_str("];\n\n");
    // The translation keys every page named, as code — after the runtime
    // keys join (a page's English stands where both name a key).
    {
        for (name, english) in options.runtime_keys {
            let _ = options.compile.keys.expect("set above").borrow_mut().intern(name, english, "runtime");
        }
        let keys = options.compile.keys.expect("set above").borrow();
        match emit::emit_keys(&keys) {
            Ok(code) => source.push_str(&code),
            Err(e) => errors.push_str(&e),
        }
    }
    // An asset's file: the `res://` path resolved against `res_root` and
    // checked, as the absolute path `include_bytes!` takes.
    let asset_file = |key: &str| -> Result<String, String> {
        let rel = key.strip_prefix("res://").unwrap_or(key);
        let file = res_root.join(rel);
        if !file.is_file() {
            return Err(format!("asset `{key}` not found at {}\n", file.display()));
        }
        // The fence (`BuildOptions::ignore_marker`): only named assets
        // embed anyway, so a fenced asset is an ERROR on the page that
        // names it — the marker says "not yet"; a silent embed would ship
        // art nobody decided on.
        if let Some(marker) = options.ignore_marker {
            if let Some(ignore) = ignore_file_above(&file, res_root, marker) {
                return Err(format!("asset `{key}` sits under {} — delete the marker to ship that folder\n", ignore.display()));
            }
        }
        Ok(file.canonicalize().unwrap_or(file).display().to_string().trim_start_matches("\\\\?\\").to_string())
    };
    // The font table: the key is the path as the file wrote it; the
    // default face first (the first entry is what text falls back to).
    fonts.sort();
    if let Some(name) = options.default_font {
        fonts.sort_by_key(|f| !f.contains(name));
    }
    source.push_str("/// (key, TTF bytes) for every font a page names; the first is the default face.\npub const FONTS: &[(&str, &[u8])] = &[\n");
    for key in &fonts {
        match asset_file(key) {
            Ok(abs) => source.push_str(&format!("    ({key:?}, include_bytes!({abs:?})),\n")),
            Err(e) => errors.push_str(&e),
        }
    }
    source.push_str("];\n\n");
    // Code images (`gogo_image!` sites) join the table after the pages: a
    // path a page named keeps its id, a new one takes the next.
    for path in options.code_images {
        if !path.starts_with("res://") {
            errors.push_str(&format!("gogo_image!({path:?}): a code image is a `res://` asset path\n"));
            continue;
        }
        let mut images = options.compile.images.expect("set above").borrow_mut();
        let key = images.intern(path);
        images.release_lines.push((key, false)); // a code site never says `release=unused`
    }
    // The image table, dense by `ImageKey`: every path a `source=` named,
    // with the file's bytes for a `res://` asset and `None` for a picture
    // the host shows at runtime (`preview://brush`).
    {
        let images = options.compile.images.expect("set above").borrow();
        source.push_str("/// (path, encoded bytes) for every image a page names, by `ImageKey`; `None` = the host shows it.\npub const IMAGES: &[(&str, Option<&[u8]>)] = &[\n");
        for key in &images.names {
            if !key.starts_with("res://") {
                source.push_str(&format!("    ({key:?}, None),\n"));
                continue;
            }
            match asset_file(key) {
                Ok(abs) => source.push_str(&format!("    ({key:?}, Some(include_bytes!({abs:?}))),\n")),
                Err(e) => errors.push_str(&e),
            }
        }
        source.push_str("];\n\n");
        // `code_image(path)`: the key for a `gogo_image!` site — a match
        // over exactly the scanned literals, so an unscanned path is a
        // compile error at the macro, never a runtime miss.
        source.push_str("/// The image key for a `gogo_image!(\"res://…\")` site (generated over the build's scan).\npub fn code_image(path: &str) -> gogo_ui::ImageKey {\n    match path {\n");
        for path in options.code_images {
            if let Some(key) = images.get(path) {
                source.push_str(&format!("        {path:?} => gogo_ui::ImageKey::from_index({}),\n", key.index()));
            }
        }
        source.push_str("        other => panic!(\"gogo_image!({other:?}) is not in the build's image table — the build scans src/**/*.rs for the literal; a path built at runtime cannot be embedded\"),\n    }\n}\n\n");
        // The pictures the host may free while no page names them.
        source.push_str("/// The pictures every declaring line marks `release=unused`: the host frees them while no live page names them.\npub static RELEASE_UNUSED: &[gogo_ui::ImageKey] = &[");
        for key in images.release_unused() {
            source.push_str(&format!("gogo_ui::ImageKey::new({}), ", key.index()));
        }
        source.push_str("];\n\n");
        // The slice table every page points at, and the atlases as code.
        source.push_str(&emit::emit_slices(&images.slices));
        match emit::emit_atlases(&images.atlases) {
            Ok(code) => source.push_str(&code),
            Err(e) => errors.push_str(&e),
        }
        source.push_str(&emit::emit_build(&built, &images.atlases, options.compile.code_controls, options.compile.behaviors, options.compile.require_keys));
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    std::fs::write(out, source).map_err(|e| format!("{}: {e}", out.display()))
}

/// The nearest `marker` file in `file`'s folder or its parents, stopping
/// at `root` (`None` = not fenced).
fn ignore_file_above(file: &Path, root: &Path, marker: &str) -> Option<PathBuf> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let mut dir = file.canonicalize().ok()?;
    dir.pop();
    loop {
        let candidate = dir.join(marker);
        if candidate.is_file() {
            // Without the `\\?\` verbatim prefix canonicalize adds on Windows.
            let shown = candidate.display().to_string();
            return Some(PathBuf::from(shown.trim_start_matches("\\\\?\\")));
        }
        if dir == root || !dir.pop() {
            return None;
        }
    }
}
