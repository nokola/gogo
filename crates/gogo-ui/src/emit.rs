//! Emits a compiled page as Rust source for `build.rs`: the pools as
//! `static` slices (`Debug` output of the `Copy` page types is valid Rust
//! with the variant globs imported), a `Handler` enum from the file's
//! `on:` names and auto-wired Buttons, and a `Names` struct of node ids.
//! `emit_keys`: the build-wide translation keys as `KEYS` + `enum Key`.

use crate::locale::Keys;
use crate::page::*;

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
    out.push_str("    impl gogo::ArrangementName for Arrangement {\n        fn id(self) -> u32 {\n            self as u32\n        }\n    }\n\n");
}

/// The Rust source of `pub mod <module>`.
pub fn emit_rust(page: &Page, module: &str) -> String {
    let mut out = String::new();
    out.push_str(&format!("#[allow(unused_imports, clippy::all)]\npub mod {module} {{\n"));
    out.push_str("    use gogo::page::*;\n    use gogo::page::{Kind::*, Len::*, Align::*, PropKey::*, Value::*, State::*, Action::*, Auto::*};\n\n");
    emit_arrangements(page, &mut out);
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

    // Names struct.
    out.push_str("    /// Every named node, by id.\n    #[derive(Clone, Copy, Debug)]\n    pub struct Names {\n");
    // Widget-body names are per instance (`inst/name`) — not addressable from code.
    let named: Vec<(String, NodeId)> = page.nodes.iter().enumerate().filter(|(_, n)| n.name != NO_STR && !page.str(n.name).contains('/')).map(|(i, n)| (snake_case(page.str(n.name)), i as NodeId)).collect();
    for (n, _) in &named {
        out.push_str(&format!("        pub {n}: NodeId,\n"));
    }
    out.push_str("    }\n\n    pub const NAMES: Names = Names {\n");
    for (n, id) in &named {
        out.push_str(&format!("        {n}: {id},\n"));
    }
    out.push_str("    };\n}\n");
    out
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
    out.push_str("            _ => None,\n        }\n    }\n\n    pub const fn id(self) -> gogo::KeyId {\n        self as u32\n    }\n}\n\n");
    out.push_str("impl gogo::KeyName for Key {\n    fn id(self) -> gogo::KeyId {\n        self as u32\n    }\n}\n\n");
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
        let src = emit_rust(&c.page, "t");
        assert!(src.contains("pub enum Arrangement"), "{src}");
        assert!(src.contains("Wide = 0"), "{src}");
        assert!(src.contains("impl gogo::ArrangementName for Arrangement"), "{src}");
        let c = compile("View v { Panel p width=10 }", &|_| None);
        assert!(!emit_rust(&c.page, "t").contains("enum Arrangement"), "no arrangements, no enum");
        let mut clash = Keys::default();
        clash.intern("fxColor", "a", "").unwrap();
        clash.intern("fx_color", "b", "").unwrap();
        let err = emit_keys(&clash).unwrap_err();
        assert!(err.contains("`fxColor` and `fx_color` both become `Key::FxColor`"), "{err}");
    }

    #[test]
    fn names_and_handlers_are_code() {
        let c = compile("View v { Button saveButton text=\"Save\"; Button b text=\"x\", on:activate=clear_all }", &|_| None);
        let src = emit_rust(&c.page, "main");
        assert!(src.contains("pub enum Handler {\n        ClearAll,\n        SaveButton,\n    }"), "{src}");
        assert!(src.contains("pub save_button: NodeId"));
        assert!(src.contains("kind: Button"));
        assert!(src.contains("strings: &[\"\", \"dark\", \"View\", \"v\""), "the well-known variant name is interned on every page (GPAR111)");
    }

    #[test]
    fn case_helpers() {
        assert_eq!(snake_case("saveButton"), "save_button");
        assert_eq!(snake_case("save_button"), "save_button");
        assert_eq!(snake_case("HTTPServer2x"), "httpserver2x");
        assert_eq!(pascal_case("clear_all"), "ClearAll");
        assert_eq!(pascal_case("saveButton"), "SaveButton");
    }
}
