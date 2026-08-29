//! Emits a compiled page as Rust source for `build.rs`: the pools as
//! `static` slices (`Debug` output of the `Copy` page types is valid Rust
//! with the variant globs imported), a `Handler` enum from the file's
//! `on:` names and auto-wired Buttons, and a `Names` struct of node ids.

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
/// Toggle/Radio without an `on:activate` (the auto-wiring convention).
pub fn handler_names(page: &Page) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for h in &page.handlers {
        if let Action::Named(id) = h.action {
            let n = page.str(id).to_string();
            if !names.contains(&n) {
                names.push(n);
            }
        }
    }
    let activate = page.intern_lookup("activate");
    for node in &page.nodes {
        if node.kind.is_press_surface() && node.name != NO_STR {
            let has_activate = page.handlers[node.handlers.0 as usize..(node.handlers.0 + node.handlers.1) as usize].iter().any(|h| h.event == activate);
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

/// The Rust source of `pub mod <module>`.
pub fn emit_rust(page: &Page, module: &str) -> String {
    let mut out = String::new();
    out.push_str(&format!("#[allow(unused_imports, clippy::all)]\npub mod {module} {{\n"));
    out.push_str("    use gogo::page::*;\n    use gogo::page::{Kind::*, Len::*, Align::*, PropKey::*, Value::*, State::*, Action::*, Join::*};\n\n");
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
    out.push_str(&format!("        transitions: {},\n", slice(&page.transitions)));
    out.push_str(&format!("        anims: {},\n", slice(&page.anims)));
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
    let named: Vec<(String, NodeId)> = page.nodes.iter().enumerate().filter(|(_, n)| n.name != NO_STR).map(|(i, n)| (snake_case(page.str(n.name)), i as NodeId)).collect();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::compile;

    #[test]
    fn names_and_handlers_are_code() {
        let c = compile("View v { Button saveButton text=\"Save\"; Button b text=\"x\", on:activate=clear_all }", &|_| None);
        let src = emit_rust(&c.page, "main");
        assert!(src.contains("pub enum Handler {\n        ClearAll,\n        SaveButton,\n    }"), "{src}");
        assert!(src.contains("pub save_button: NodeId"));
        assert!(src.contains("kind: Button"));
        assert!(src.contains("strings: &[\"\", \"View\", \"v\""));
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
