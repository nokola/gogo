//! Emits a compiled page as Rust source for `build.rs`: the pools as
//! `static` slices (`Debug` output of the `Copy` page types is valid Rust
//! with the variant globs imported), a `Handler` enum from the file's
//! `on:` names and auto-wired Buttons, `enum Arrangement` and `enum Anim`
//! (typed handles for `Runtime::go` / `play`), and a `Names` struct of
//! node ids — a widget instance is a nested `<Widget>Names` entry
//! (`NAMES.slider_size.track`). Build-wide: `emit_keys` (the translation
//! keys as `KEYS` + `enum Key`) and `emit_widget_names` (one
//! `<Widget>Names` struct per widget the build's pages instance).

use crate::locale::Keys;
use crate::page::*;

/// Rust's keywords: a node named `box` or `type` becomes the field
/// `r#box` / `r#type` — the name the file wrote, kept legal.
const KEYWORDS: &[&str] = &["as", "break", "const", "continue", "else", "enum", "extern", "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return", "static", "struct", "trait", "true", "type", "unsafe", "use", "where", "while", "async", "await", "dyn", "abstract", "become", "box", "do", "final", "macro", "override", "priv", "typeof", "unsized", "virtual", "yield", "try", "gen"];

/// `snake_case`, escaped when the result is a Rust keyword (`r#box`).
/// `crate`, `self`, `Self` and `super` cannot be escaped and stay as they
/// are — the emitted struct then fails to compile with rustc's own message,
/// naming the field.
pub fn field_name(name: &str) -> String {
    let s = snake_case(name);
    if KEYWORDS.contains(&s.as_str()) { format!("r#{s}") } else { s }
}

/// `save_button` from `saveButton` / `SaveButton` / `save_button`.
pub fn snake_case(name: &str) -> String {
    let mut out = String::new();
    let mut prev_lower = false;
    for c in name.chars() {
        if c.is_ascii_uppercase() {
            if prev_lower {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
            prev_lower = false;
        } else {
            out.push(c);
            prev_lower = c.is_ascii_lowercase() || c.is_ascii_digit();
        }
    }
    out
}

/// `SaveButton` from `saveButton` / `save_button`.
pub fn pascal_case(name: &str) -> String {
    let mut out = String::new();
    let mut upper = true;
    for c in name.chars() {
        if c == '_' {
            upper = true;
            continue;
        }
        if upper {
            out.push(c.to_ascii_uppercase());
            upper = false;
        } else {
            out.push(c);
        }
    }
    out
}

/// Handler names: every `Action::Named`, plus every named Button/Check/
/// Toggle/Radio without an `on:activate` and every named Slider/Input
/// without an `on:change` (the auto-wiring convention: the node's own
/// name is its handler, for `activate` and for `change`/`edit` alike).
pub fn handler_names(page: &Page) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for h in &page.handlers {
        if let Action::Named(id) | Action::Emit(id) = h.action {
            let n = page.str(id).to_string();
            if !names.contains(&n) {
                names.push(n);
            }
        }
    }
    // A named `choice` reports its pick to a handler of its own name.
    for c in &page.choices {
        if let Some(Action::Named(id) | Action::Emit(id)) = c.action {
            let n = page.str(id).to_string();
            if !names.contains(&n) {
                names.push(n);
            }
        }
    }
    let activate = page.intern_lookup("activate");
    let change = page.intern_lookup("change");
    for node in &page.nodes {
        let value_control = matches!(node.kind, Kind::Slider | Kind::Input);
        if (node.kind.is_press_surface() || value_control) && node.name != NO_STR && !page.str(node.name).contains('/') {
            let wired = if value_control { change } else { activate };
            let has_activate = page.handlers[node.handlers.0 as usize..(node.handlers.0 + node.handlers.1) as usize].iter().any(|h| h.event == wired);
            if !has_activate {
                let n = page.str(node.name).to_string();
                if !names.contains(&n) {
                    names.push(n);
                }
            }
        }
    }
    names
}

fn slice<T: std::fmt::Debug>(items: &[T]) -> String {
    let mut s = String::from("&[");
    for (i, it) in items.iter().enumerate() {
        if i > 0 {
            s.push_str(", ");
        }
        s.push_str(&format!("{it:?}"));
    }
    s.push(']');
    s
}

/// The page's arrangements as code: `enum Arrangement` (one variant per
/// declared name, `#[repr(u32)]` = the index `when <name>` poses match),
/// plus `Base` (the rest pose) — `Runtime::go(Arrangement::Wide)` cannot
/// name a pose the page lacks (GPAR27).
fn emit_arrangements(page: &Page, out: &mut String) {
    if page.arrangements.is_empty() {
        return;
    }
    out.push_str("    /// The page's declared arrangements; `Base` is the rest pose.\n");
    out.push_str("    #[allow(clippy::enum_variant_names, non_camel_case_types, dead_code)]\n    #[derive(Clone, Copy, Debug, PartialEq, Eq)]\n    #[repr(u32)]\n    pub enum Arrangement {\n");
    for (i, a) in page.arrangements.iter().enumerate() {
        out.push_str(&format!("        {} = {i},\n", key_variant(page.str(*a))));
    }
    out.push_str(&format!("        Base = {},\n", u32::MAX));
    out.push_str("    }\n\n");
    out.push_str("    impl gogo::ArrangementName for Arrangement {\n        fn id(self) -> gogo::ArrangementId {\n            gogo::ArrangementId::new(self as u32)\n        }\n    }\n\n");
}

/// The page's own anims as code: `enum Anim` (one variant per `anim` the
/// file declares at page level, `#[repr(u32)]` = the name's string id)
/// plus the `AnimName` impl — `Runtime::play(page, Anim::Appear, …)`
/// cannot misspell, and nothing interns the name per frame. A widget's
/// anims (`inst/anim`) belong to the widget's own `on:` wiring and are not
/// listed. `Err` when two names spell one variant (`fadeIn` / `fade_in`).
fn emit_anims(page: &Page, out: &mut String) -> Result<(), String> {
    let mut names: Vec<(String, StrId)> = Vec::new();
    for h in &page.anim_heads {
        let name = page.str(h.anim);
        if name.contains('/') || names.iter().any(|(n, _)| n == name) {
            continue;
        }
        names.push((name.to_string(), h.anim));
    }
    if names.is_empty() {
        return Ok(());
    }
    let mut variants: Vec<String> = Vec::with_capacity(names.len());
    for (name, _) in &names {
        let v = key_variant(name);
        if let Some(j) = variants.iter().position(|x| *x == v) {
            return Err(format!("anims `{}` and `{name}` both become `Anim::{v}` — rename one\n", names[j].0));
        }
        variants.push(v);
    }
    out.push_str("    /// The page's declared anims — `Runtime::play` / `stop` / `is_playing` take one.\n");
    out.push_str("    #[allow(clippy::enum_variant_names, non_camel_case_types, dead_code)]\n    #[derive(Clone, Copy, Debug, PartialEq, Eq)]\n    #[repr(u32)]\n    pub enum Anim {\n");
    for ((_, id), v) in names.iter().zip(&variants) {
        out.push_str(&format!("        {v} = {},\n", id.index()));
    }
    out.push_str("    }\n\n");
    out.push_str("    impl gogo::AnimName for Anim {\n        fn id(self) -> gogo::StrId {\n            gogo::StrId::new(self as u32)\n        }\n    }\n\n");
    Ok(())
}

/// One entry of a `Names` struct: a plain named node, or a widget
/// instance (its own struct, `<Widget>Names`).
#[derive(Clone, Debug, PartialEq)]
pub enum NameEntry {
    Node(NodeId),
    Instance { root: NodeId, widget: String, body: Vec<(String, NameEntry)> },
}

/// The Rust struct name for widget `SliderPhone`: `SliderPhoneNames`.
pub fn widget_struct(widget: &str) -> String {
    format!("{}Names", pascal_case(widget))
}

/// The `Names` entries of the nodes directly under `prefix` (`""` = the
/// page level; `"inst"` = the body of widget instance `inst`): every named
/// node whose name is `prefix/x` with no further `/`, snake_cased; a
/// widget instance root becomes an `Instance` entry with its body
/// resolved the same way. Unnamed instances (`#1/…`) and repeat rows
/// (`list/item/…`, no widget root) have no entry — nothing in code can
/// address them stably.
fn name_entries(page: &Page, prefix: &str) -> Result<Vec<(String, NameEntry)>, String> {
    let mut out: Vec<(String, NameEntry)> = Vec::new();
    for (i, n) in page.nodes.iter().enumerate() {
        if n.name == NO_STR {
            continue;
        }
        let full = page.str(n.name);
        let local = if prefix.is_empty() {
            full
        } else {
            match full.strip_prefix(prefix).and_then(|r| r.strip_prefix('/')) {
                Some(r) => r,
                None => continue,
            }
        };
        if local.contains('/') || local.starts_with('#') || local.is_empty() {
            continue;
        }
        let field = field_name(local);
        let entry = if n.widget != NO_STR {
            let body = name_entries(page, full)?;
            if body.iter().any(|(f, _)| f == "root") {
                return Err(format!("widget `{}`: a body node named `root` clashes with the instance's `root` field — rename it\n", page.str(n.widget)));
            }
            NameEntry::Instance { root: NodeId::from_index(i), widget: page.str(n.widget).to_string(), body }
        } else {
            NameEntry::Node(NodeId::from_index(i))
        };
        if let Some((f, other)) = out.iter().find(|(f, _)| *f == field) {
            if *other != entry {
                return Err(format!("nodes `{full}` and another under `{prefix}` both become `NAMES.{f}` — rename one\n"));
            }
            continue;
        }
        out.push((field, entry));
    }
    Ok(out)
}

/// The type a `Names` field has.
fn entry_type(e: &NameEntry) -> String {
    match e {
        NameEntry::Node(_) => "NodeId".to_string(),
        NameEntry::Instance { widget, .. } => format!("super::{}", widget_struct(widget)),
    }
}

/// The value a `Names` field has (`indent` = the literal's nesting).
fn entry_value(e: &NameEntry, indent: usize) -> String {
    match e {
        NameEntry::Node(id) => format!("{id:?}"),
        NameEntry::Instance { root, widget, body } => {
            let pad = " ".repeat(indent);
            let mut s = format!("super::{} {{\n{pad}    root: {root:?},\n", widget_struct(widget));
            for (f, e) in body {
                s.push_str(&format!("{pad}    {f}: {},\n", entry_value(e, indent + 4)));
            }
            s.push_str(&format!("{pad}}}"));
            s
        }
    }
}

/// The widget structs a page's instances need: widget → (field, type) rows,
/// every instance of one widget agreeing on the rows (they stamp one body).
fn collect_widget_shapes(entries: &[(String, NameEntry)], shapes: &mut Vec<(String, Vec<(String, String)>)>, where_: &str) -> Result<(), String> {
    for (_, e) in entries {
        if let NameEntry::Instance { widget, body, .. } = e {
            let mut rows: Vec<(String, String)> = vec![("root".to_string(), "NodeId".to_string())];
            for (f, be) in body {
                rows.push((f.clone(), entry_type(be)));
            }
            match shapes.iter().find(|(w, _)| w == widget) {
                Some((_, have)) if *have != rows => {
                    return Err(format!("widget `{widget}`: its instances do not stamp one body (see {where_}) — `{}Names` cannot be one struct\n", pascal_case(widget)));
                }
                Some(_) => {}
                None => shapes.push((widget.clone(), rows)),
            }
            collect_widget_shapes(body, shapes, where_)?;
        }
    }
    Ok(())
}

/// The build-wide `<Widget>Names` structs: one per widget any page in
/// `pages` instances, at the generated module's top level (`(module, page)`
/// pairs). One struct per widget means a widget's code-behind takes
/// `&SliderPhoneNames` whichever page the instance sits on.
pub fn emit_widget_names(pages: &[(&str, &Page)]) -> Result<String, String> {
    let mut shapes: Vec<(String, Vec<(String, String)>)> = Vec::new();
    for (module, page) in pages {
        let entries = name_entries(page, "").map_err(|e| format!("{module}: {e}"))?;
        collect_widget_shapes(&entries, &mut shapes, module)?;
    }
    let mut out = String::new();
    for (widget, rows) in &shapes {
        out.push_str(&format!("/// The body of one `{widget}` instance, by node id (`root` = the instance itself).\n"));
        out.push_str(&format!("#[allow(dead_code)]\n#[derive(Clone, Copy, Debug)]\npub struct {} {{\n", widget_struct(widget)));
        for (f, t) in rows {
            let t = t.strip_prefix("super::").unwrap_or(t);
            out.push_str(&format!("    pub {f}: {t},\n"));
        }
        out.push_str("}\n\n");
    }
    if !shapes.is_empty() {
        out = format!("use gogo::NodeId;\n\n{out}");
    }
    Ok(out)
}

/// The Rust source of `pub mod <module>`. `Err` for a name clash the
/// generated code could not carry (two anims spelling one variant, two
/// nodes spelling one field, a body node named `root`).
pub fn emit_rust(page: &Page, module: &str) -> Result<String, String> {
    let mut out = String::new();
    out.push_str(&format!("#[allow(unused_imports, clippy::all)]\npub mod {module} {{\n"));
    out.push_str("    use gogo::page::*;\n    use gogo::page::{Kind::*, Len::*, Align::*, PropKey::*, Value::*, State::*, Action::*, Auto::*};\n\n");
    emit_arrangements(page, &mut out);
    emit_anims(page, &mut out).map_err(|e| format!("{module}: {e}"))?;
    out.push_str("    pub static PAGE: StaticPage = StaticPage {\n");
    out.push_str(&format!("        nodes: {},\n", slice(&page.nodes)));
    out.push_str(&format!("        strings: {},\n", slice(&page.strings.iter().map(String::as_str).collect::<Vec<_>>())));
    out.push_str(&format!("        props: {},\n", slice(&page.props)));
    out.push_str(&format!("        values: {},\n", slice(&page.values)));
    out.push_str(&format!("        whens: {},\n", slice(&page.whens)));
    out.push_str(&format!("        handlers: {},\n", slice(&page.handlers)));
    out.push_str(&format!("        theme: {},\n", slice(&page.theme)));
    out.push_str(&format!("        kind_defaults: {},\n", slice(&page.kind_defaults)));
    out.push_str(&format!("        arrangements: {},\n", slice(&page.arrangements)));
    out.push_str(&format!("        custom_states: {},\n", slice(&page.custom_states)));
    out.push_str(&format!("        key_claims: {},\n", slice(&page.key_claims)));
    out.push_str(&format!("        behaviors: {},\n", slice(&page.behaviors)));
    out.push_str(&format!("        transitions: {},\n", slice(&page.transitions)));
    out.push_str(&format!("        anims: {},\n", slice(&page.anims)));
    out.push_str(&format!("        anim_heads: {},\n", slice(&page.anim_heads)));
    out.push_str(&format!("        choices: {},\n", slice(&page.choices)));
    out.push_str(&format!("        repeats: {},\n", slice(&page.repeats)));
    out.push_str(&format!("        bind_sites: {},\n", slice(&page.bind_sites)));
    out.push_str(&format!("        drivers: {},\n", slice(&page.drivers)));
    out.push_str(&format!("        live_texts: {},\n", slice(&page.live_texts)));
    out.push_str("    };\n\n");

    // Handler enum.
    let names = handler_names(page);
    out.push_str("    /// The file's handler names; the app matches on these.\n");
    out.push_str("    #[derive(Clone, Copy, Debug, PartialEq, Eq)]\n    pub enum Handler {\n");
    for n in &names {
        out.push_str(&format!("        {},\n", pascal_case(n)));
    }
    out.push_str("    }\n\n    impl Handler {\n        pub fn from_name(name: &str) -> Option<Handler> {\n            match name {\n");
    for n in &names {
        out.push_str(&format!("                {n:?} => Some(Handler::{}),\n", pascal_case(n)));
    }
    out.push_str("                _ => None,\n            }\n        }\n    }\n\n");

    // Names struct: plain nodes by id; a widget instance as its
    // `<Widget>Names` (root + body), nested for a widget inside a widget.
    let entries = name_entries(page, "").map_err(|e| format!("{module}: {e}"))?;
    out.push_str("    /// Every named node, by id; a widget instance by its `<Widget>Names`.\n    #[allow(dead_code)]\n    #[derive(Clone, Copy, Debug)]\n    pub struct Names {\n");
    for (f, e) in &entries {
        out.push_str(&format!("        pub {f}: {},\n", entry_type(e)));
    }
    out.push_str("    }\n\n    pub const NAMES: Names = Names {\n");
    for (f, e) in &entries {
        out.push_str(&format!("        {f}: {},\n", entry_value(e, 8)));
    }
    out.push_str("    };\n}\n");
    Ok(out)
}

/// The Rust variant for translation key `name`: `pickColorHint` →
/// `PickColorHint`; a key that starts with a digit gets `N` in front
/// (`16by9` → `N16by9`), since a variant cannot.
pub fn key_variant(name: &str) -> String {
    let p = pascal_case(name);
    if p.chars().next().is_some_and(|c| c.is_ascii_digit()) { format!("N{p}") } else { p }
}

/// The build's translation keys as code, at the `ui` module's top level:
/// `KEYS: &[(name, English)]` by `KeyId`, `enum Key` (one variant per key,
/// `key_variant` spelling, `#[repr(u32)]` = the id), `Key::from_name` for
/// loading a translation file, and the `gogo::KeyName` impl that lets
/// `Runtime::set_key` take a `Key` and nothing else. `Err` when two names
/// spell the same variant (`fxColor` and `fx_color`) — a build error, not a
/// silent merge.
pub fn emit_keys(keys: &Keys) -> Result<String, String> {
    let mut out = String::new();
    let mut variants: Vec<String> = Vec::with_capacity(keys.names.len());
    for name in &keys.names {
        let v = key_variant(name);
        if let Some(j) = variants.iter().position(|x| *x == v) {
            return Err(format!("translation keys `{}` and `{name}` both become `Key::{v}` — rename one\n", keys.names[j]));
        }
        variants.push(v);
    }
    out.push_str("/// Every translation key a page names, by `KeyId`: (key as written, its English).\n");
    out.push_str("pub static KEYS: &[(&str, &str)] = &[\n");
    for (name, english) in keys.names.iter().zip(&keys.english) {
        out.push_str(&format!("    ({name:?}, {english:?}),\n"));
    }
    out.push_str("];\n\n");
    out.push_str("/// The translation keys as code: `Key::Ok.id()` indexes `KEYS`; a key no page declares does not compile.\n");
    out.push_str("#[allow(clippy::enum_variant_names, non_camel_case_types)]\n#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]\n#[repr(u32)]\npub enum Key {\n");
    for (i, v) in variants.iter().enumerate() {
        out.push_str(&format!("    {v} = {i},\n"));
    }
    out.push_str("}\n\n");
    out.push_str("impl Key {\n    /// The key spelled as the `.gogo` files (and translation files) write it.\n    pub fn from_name(name: &str) -> Option<Key> {\n        match name {\n");
    for (name, v) in keys.names.iter().zip(&variants) {
        out.push_str(&format!("            {name:?} => Some(Key::{v}),\n"));
    }
    out.push_str("            _ => None,\n        }\n    }\n\n    pub const fn id(self) -> gogo::KeyId {\n        gogo::KeyId::new(self as u32)\n    }\n}\n\n");
    out.push_str("impl gogo::KeyName for Key {\n    fn id(self) -> gogo::KeyId {\n        gogo::KeyId::new(self as u32)\n    }\n}\n\n");
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::compile;

    #[test]
    fn keys_are_code() {
        let mut k = Keys::default();
        k.intern("ok", "OK", "").unwrap();
        k.intern("16by9", "16:9", "").unwrap();
        k.intern("pickColorHint", "Pick a color", "").unwrap();
        let src = emit_keys(&k).unwrap();
        assert!(src.contains("pub static KEYS: &[(&str, &str)] = &[\n    (\"ok\", \"OK\"),\n    (\"16by9\", \"16:9\"),\n    (\"pickColorHint\", \"Pick a color\"),\n];"), "{src}");
        assert!(src.contains("pub enum Key {\n    Ok = 0,\n    N16by9 = 1,\n    PickColorHint = 2,\n}"), "{src}");
        assert!(src.contains("\"16by9\" => Some(Key::N16by9),"), "{src}");
        assert!(src.contains("impl gogo::KeyName for Key"), "{src}");
    }

    /// GPAR27: a page with arrangements emits their typed handle —
    /// `enum Arrangement { Wide = 0, Base }` + the `ArrangementName`
    /// impl — so `Runtime::go(Arrangement::Wide)` cannot misspell.
    #[test]
    fn arrangements_emit_a_typed_enum() {
        let c = compile("arrangement wide\nView v { Panel p width=10 }\nwhen wide { p.width = 20 }\n", &|_| None);
        let src = emit_rust(&c.page, "t").unwrap();
        assert!(src.contains("pub enum Arrangement"), "{src}");
        assert!(src.contains("Wide = 0"), "{src}");
        assert!(src.contains("impl gogo::ArrangementName for Arrangement"), "{src}");
        let c = compile("View v { Panel p width=10 }", &|_| None);
        assert!(!emit_rust(&c.page, "t").unwrap().contains("enum Arrangement"), "no arrangements, no enum");
        let mut clash = Keys::default();
        clash.intern("fxColor", "a", "").unwrap();
        clash.intern("fx_color", "b", "").unwrap();
        let err = emit_keys(&clash).unwrap_err();
        assert!(err.contains("`fxColor` and `fx_color` both become `Key::FxColor`"), "{err}");
    }

    #[test]
    fn names_and_handlers_are_code() {
        let c = compile("View v { Button saveButton text=\"Save\"; Button b text=\"x\", on:activate=clear_all }", &|_| None);
        let src = emit_rust(&c.page, "main").unwrap();
        assert!(src.contains("pub enum Handler {\n        ClearAll,\n        SaveButton,\n    }"), "{src}");
        assert!(src.contains("pub save_button: NodeId"));
        assert!(src.contains("kind: Button"));
        assert!(src.contains("strings: &[\"\", \"dark\", \"View\", \"v\""), "the well-known variant name is interned on every page (GPAR111)");
    }

    /// A page's anims are a typed handle too: `enum Anim { Appear = <str id> }`
    /// + `AnimName`, so `rt.play(page, Anim::Appear, …)` cannot misspell;
    /// a widget's own anims (`inst/x`) stay out.
    #[test]
    fn anims_emit_a_typed_enum() {
        let c = compile("View v { Panel p width=10, opacity=1 }\nanim appear { p.opacity from=0, to=1, duration=0.2 }\nanim slideOut { p.translate.x to=10, duration=0.1 }\n", &|_| None);
        assert!(!crate::has_errors(&c.diags), "{:#?}", c.diags);
        let src = emit_rust(&c.page, "t").unwrap();
        let appear = c.page.intern_lookup("appear");
        assert!(src.contains(&format!("pub enum Anim {{\n        Appear = {},\n        SlideOut = {},\n    }}", appear.index(), c.page.intern_lookup("slideOut").index())), "{src}");
        assert!(src.contains("impl gogo::AnimName for Anim"), "{src}");
        assert!(!emit_rust(&compile("View v { Panel p width=10 }", &|_| None).page, "t").unwrap().contains("enum Anim"), "no anims, no enum");
    }

    /// A widget instance is a nested names struct: `NAMES.vol.track` is the
    /// instance's body node, `NAMES.vol.root` the instance; one
    /// `<Widget>Names` struct per widget for the whole build, so a
    /// code-behind takes the struct whichever page the instance sits on.
    #[test]
    fn widget_instances_emit_nested_names() {
        let src_a = "widget Bar { Column { Label caption text=\"c\"; Rectangle track width=fill, height=4 } }\nView v { Bar vol; Bar tone; Button ok text=\"k\" }";
        let src_b = "widget Bar { Column { Label caption text=\"c\"; Rectangle track width=fill, height=4 } }\nView w { Bar depth }";
        let a = compile(src_a, &|_| None);
        let b = compile(src_b, &|_| None);
        assert!(!crate::has_errors(&a.diags), "{:#?}", a.diags);
        let src = emit_rust(&a.page, "a").unwrap();
        assert!(src.contains("pub vol: super::BarNames,"), "{src}");
        assert!(src.contains("pub tone: super::BarNames,"), "{src}");
        assert!(src.contains("pub ok: NodeId,"), "{src}");
        let vol = a.page.find("vol").unwrap();
        let track = a.page.find("vol/track").unwrap();
        assert!(src.contains(&format!("vol: super::BarNames {{\n            root: {vol:?},\n            caption: {:?},\n            track: {track:?},\n        }}", a.page.find("vol/caption").unwrap())), "{src}");
        let structs = emit_widget_names(&[("a", &a.page), ("b", &b.page)]).unwrap();
        assert_eq!(structs.matches("pub struct BarNames").count(), 1, "one struct for the build: {structs}");
        assert!(structs.contains("pub struct BarNames {\n    pub root: NodeId,\n    pub caption: NodeId,\n    pub track: NodeId,\n}"), "{structs}");
        // A body node named `root` cannot share the struct with the instance field.
        let clash = compile("widget Bar { Column { Label root text=\"c\" } }\nView v { Bar vol }", &|_| None);
        assert!(emit_rust(&clash.page, "v").unwrap_err().contains("named `root`"));
    }

    #[test]
    fn case_helpers() {
        assert_eq!(snake_case("saveButton"), "save_button");
        assert_eq!(snake_case("save_button"), "save_button");
        assert_eq!(field_name("box"), "r#box");
        assert_eq!(field_name("type"), "r#type");
        assert_eq!(field_name("caption"), "caption");
        assert_eq!(snake_case("HTTPServer2x"), "httpserver2x");
        assert_eq!(pascal_case("clear_all"), "ClearAll");
        assert_eq!(pascal_case("saveButton"), "SaveButton");
    }
}
