//! Two lists in `docs/` mirror tables of this crate, and these tests hold
//! each pair together.
//!
//! Needed because a list a person keeps by hand goes stale without a
//! sound: a property word a kind gained that the docs' table lacks, or an
//! error code with no entry a reader can look up.
//!
//! This file reads `docs/` at the top of the repository, which a
//! crates.io package does not carry, so `Cargo.toml` leaves it out of the
//! package (`exclude`).

use gogo_ui::page::{KIND_WORDS, Kind, kind_own_words, kind_words};
use std::fs;
use std::path::{Path, PathBuf};

fn docs_file(name: &str) -> String {
    let path: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs").join(name);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// The per-kind word lists in `docs/gogo-design.md` are generated from
/// the table the compiler checks property words against (`page::legal`);
/// on a mismatch the test prints the block to paste.
#[test]
fn kind_words_doc_is_current() {
    let doc: String = docs_file("gogo-design.md");
    let start = "<!-- kind-words:start -->";
    let end = "<!-- kind-words:end -->";
    let generated: String = kind_words_markdown();
    let (a, b) = (doc.find(start).expect("start marker"), doc.find(end).expect("end marker"));
    let current: &str = doc[a + start.len()..b].trim();
    assert!(current == generated.trim(), "docs/gogo-design.md kind-words block is stale — replace it with:\n\n{generated}");
}

/// `kind_words` per kind as a Markdown table, one row per kind: the
/// words every box takes once at the top, then each kind's own.
fn kind_words_markdown() -> String {
    // The words every box takes are what a kind takes less its own words;
    // a Panel has every one of them.
    let panel_own: Vec<&str> = kind_own_words(Kind::Panel);
    let every_box: Vec<&str> = kind_words(Kind::Panel).into_iter().filter(|w| !panel_own.contains(w)).collect();
    let mut out = String::new();
    out.push_str("Every box takes: ");
    out.push_str(&every_box.iter().map(|w| format!("`{w}`")).collect::<Vec<_>>().join(", "));
    out.push_str(".\nA Grid cell adds `col_span`, `row_span`; a Tiles cell adds `span`, `full_row`.\n");
    let kinds: Vec<(&str, Kind)> = KIND_WORDS.iter().map(|(w, k)| (*w, *k)).chain(std::iter::once(("<code control>", Kind::Custom))).collect();
    // A kind that takes the box words less a few: the table shows own
    // words only, so the few are said here.
    for (word, kind) in &kinds {
        let words: Vec<&str> = kind_words(*kind);
        let less: Vec<&str> = every_box.iter().copied().filter(|w| !words.contains(w)).collect();
        if !words.is_empty() && !less.is_empty() {
            out.push_str(&format!("A `{word}` takes these less {}.\n", less.iter().map(|w| format!("`{w}`")).collect::<Vec<_>>().join(", ")));
        }
    }
    out.push_str("\n| kind | own words |\n|---|---|\n");
    for (word, kind) in kinds {
        let own: Vec<&str> = kind_own_words(kind);
        let cell: String = if kind_words(kind).is_empty() {
            "(none — the line takes clauses, not properties)".to_string()
        } else if own.is_empty() {
            "(none of its own)".to_string()
        } else {
            own.iter().map(|w| format!("`{w}`")).collect::<Vec<_>>().join(", ")
        };
        out.push_str(&format!("| `{word}` | {cell} |\n"));
    }
    out
}

/// GPAR61 (round 7, 2026-08-31): the addon's third leg — every
/// registered code has a teaching entry in docs/gogo-errors.md and every
/// entry is a registered code (no page for a retired number can linger).
#[test]
fn every_diag_code_has_a_teaching_entry_and_no_entry_is_stale() {
    let doc: String = docs_file("gogo-errors.md");
    let headings: Vec<&str> = doc.lines().filter_map(|l| l.strip_prefix("## ")).filter_map(|l| l.split_whitespace().next()).collect();
    for (code, _) in gogo_ui::diag::REGISTRY {
        assert!(headings.contains(code), "`{code}` is registered but has no `## {code}` entry in docs/gogo-errors.md");
    }
    for h in &headings {
        assert!(gogo_ui::diag::REGISTRY.iter().any(|(c, _)| c == h), "docs/gogo-errors.md teaches `{h}`, which is not a registered code");
        assert!(!gogo_ui::diag::RETIRED.contains(h), "docs/gogo-errors.md teaches retired `{h}`");
    }
}
