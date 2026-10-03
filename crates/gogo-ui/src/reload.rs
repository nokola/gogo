//! Hot reload of a page inside a running program — the half that knows
//! only gogo. A host that notices a changed `.gogo` file (a folder watch,
//! a socket, an editor) reads it, calls [`recompile`] against the tables
//! its binary was built with, checks [`same_shape`] against the page it
//! shows, and swaps the page in with the runtime's state carried over
//! (`Runtime::carried_to`). Where the file comes from, when, and what the
//! host does with a refusal (a toast, a log line) stay the host's.
//!
//! Why the shape must hold: a host's code-behind addresses nodes by index
//! fixed at build (`NAMES`), so a node added or removed would point every
//! later index at the wrong node. Why the tables are fixed: a `/key` or a
//! `res://` picture the binary never saw has no string and no pixels to
//! show. Both are a rebuild, each refused with its line.

use std::cell::RefCell;

use crate::compile::{CompileOptions, Loader, compile_with};
use crate::diag::{Diagnostic, Severity, has_errors};
use crate::locale::Keys;
use crate::page::{Images, Kind, Page};

/// What the running binary was built with — the tables a reloaded page
/// must index the same way the baked pages do.
#[derive(Clone, Copy)]
pub struct BuildTables<'a> {
    /// The build's translation keys, `(name, English)` in id order.
    pub keys: &'a [(&'a str, &'a str)],
    /// The build's picture names in id order.
    pub images: &'a [&'a str],
    /// Node types the host draws from code — the same list the build
    /// compiled with (`CompileOptions::code_controls`).
    pub code_controls: &'a [&'a str],
    /// The build's `CompileOptions::require_keys`.
    pub require_keys: bool,
}

/// Compiles `source` (called `source_name` in diagnostics, e.g.
/// `effect_select.gogo`; `loader` reads its `uses` targets) so every key
/// and picture id in the result means what the baked page's does. `Err`
/// = the errors, each at its line: the compiler's own, a `/key` not in the
/// build (G8004) or a picture not in the build (G2021).
pub fn recompile(source: &str, source_name: &str, loader: Loader<'_>, baked: &BuildTables<'_>) -> Result<Page, Vec<Diagnostic>> {
    let keys = RefCell::new(Keys::from_table(baked.keys));
    let images = RefCell::new(Images::from_names(baked.images.iter().copied()));
    let options = CompileOptions { code_controls: baked.code_controls, keys: Some(&keys), images: Some(&images), require_keys: baked.require_keys, source_name, refuse_new_keys_and_pictures: true, ..CompileOptions::default() };
    let compiled = compile_with(source, loader, options);
    if has_errors(&compiled.diags) {
        return Err(compiled.diags.into_iter().filter(|d| d.severity == Severity::Error).collect());
    }
    Ok(compiled.page)
}

/// Why two pages are not the same list of nodes (`same_shape`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShapeChange {
    /// A node was added or removed.
    Count { old: usize, new: usize },
    /// The node at `index` has another kind or name.
    Node { index: usize, old_kind: Kind, old_name: String, new_kind: Kind, new_name: String },
}

impl std::fmt::Display for ShapeChange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ShapeChange::Count { old, new } => write!(f, "node count changed ({old} -> {new}): adding or removing a node needs a rebuild"),
            ShapeChange::Node { index, old_kind, old_name, new_kind, new_name } => {
                write!(f, "node {index} changed ({old_kind:?} `{old_name}` -> {new_kind:?} `{new_name}`): a changed node list needs a rebuild")
            }
        }
    }
}

/// Do two pages have the same node list — count, kinds and names in
/// order? A host's `NAMES` are indices into that list; a reload may change
/// anything else. `Err` names the first node that differs.
pub fn same_shape(old: &Page, new: &Page) -> Result<(), ShapeChange> {
    if old.nodes.len() != new.nodes.len() {
        return Err(ShapeChange::Count { old: old.nodes.len(), new: new.nodes.len() });
    }
    for (index, (a, b)) in old.nodes.iter().zip(new.nodes.iter()).enumerate() {
        let (old_name, new_name) = (old.str(a.name), new.str(b.name));
        if a.kind != b.kind || old_name != new_name {
            return Err(ShapeChange::Node { index, old_kind: a.kind, old_name: old_name.to_string(), new_kind: b.kind, new_name: new_name.to_string() });
        }
    }
    Ok(())
}
