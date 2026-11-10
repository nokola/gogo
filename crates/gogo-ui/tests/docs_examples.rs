//! Every gogo example in the docs is run through the compiler.
//!
//! Needed because people and AI assistants learn gogo by copying from
//! `docs/` and the README: an example that no longer compiles teaches a
//! dead spelling, and nothing else notices when the language moves on.
//!
//! What counts as an example: a fenced block tagged `kdl` or `gogo` in the
//! README, `docs/gogo-spec.md`, `docs/REFERENCE.md` and `docs/COOKBOOK.md`,
//! and in `docs/cards/` also a fenced block with no tag (the cards' habit).
//! A block tagged with another language (`rust`, `text`) is not gogo.
//!
//! A second word after the tag says how an example is held to account:
//!
//! - no second word: it compiles with no error. A snippet with no `View`
//!   or `Panel` root is put inside a `View` first, and it may use the
//!   names of the docs' shared theme (`docs/examples/styles/theme.gogo`)
//!   as if it began with `uses "styles/theme.gogo"`.
//! - `partial`: it leans on something outside itself (a node from an
//!   earlier snippet, a control or a picture only a host has), so only
//!   its grammar is checked.
//! - `error=G1234`: it shows a mistake on purpose, and the compiler must
//!   refuse it with exactly that code.
//! - `stale`: it was written for the earlier Godot implementation and is
//!   not rewritten yet. The compiler must still refuse it; once it
//!   compiles, the word comes off. A reader of the Markdown source sees
//!   the word and knows not to copy the example.
//!
//! `...` on a line of its own (also `...like this...`) and `{ ... }` mean
//! "parts left out"; they are dropped before the check.

use gogo_ui::{Diagnostic, Severity, compile};
use std::fs;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// One fenced block of a Markdown file.
struct Fence {
    /// Line of the opening fence in the file, counted from 1.
    line: usize,
    /// The words after the three backticks: `kdl`, `kdl partial`, or none.
    words: Vec<String>,
    body: String,
}

/// The fenced blocks of a Markdown text, in order.
///
/// A fence may sit inside a list item; the indent of its opening line is
/// taken off every body line, so the example reads as it would in a file.
fn fences(markdown: &str) -> Vec<Fence> {
    let mut out: Vec<Fence> = Vec::new();
    let mut open: Option<(Fence, usize)> = None;
    for (i, raw) in markdown.lines().enumerate() {
        let trimmed: &str = raw.trim_start();
        let indent: usize = raw.len() - trimmed.len();
        if let Some(info) = trimmed.strip_prefix("```") {
            match open.take() {
                Some((fence, _)) => out.push(fence),
                None => open = Some((Fence { line: i + 1, words: info.split_whitespace().map(str::to_string).collect(), body: String::new() }, indent)),
            }
            continue;
        }
        if let Some((fence, fence_indent)) = open.as_mut() {
            let cut: usize = (*fence_indent).min(indent);
            fence.body.push_str(&raw[cut..]);
            fence.body.push('\n');
        }
    }
    out
}

/// An example with its "parts left out" marks removed.
fn strip_elisions(example: &str) -> String {
    let mut out: String = String::new();
    for line in example.lines() {
        let t: &str = line.trim();
        if t == "// ..." || t == "…" || (t.starts_with("...") && t.ends_with("...")) {
            continue;
        }
        out.push_str(&line.replace("{ ... }", "{ }").replace("{...}", "{ }").replace("{ … }", "{ }"));
        out.push('\n');
    }
    out
}

/// Top-level words that declare something and are never a file's root
/// (the same list `compile` skips when it looks for the root).
const DECLARATIONS: &[&str] = &["uses", "allow", "theme", "style", "widget", "control", "anim", "arrangement", "atlas", "transition", "when"];

fn first_word(statement: &str) -> &str {
    let t: &str = statement.trim_start();
    let end: usize = t.char_indices().find(|(i, c)| !(c.is_alphanumeric() || *c == '_' || (*i == 0 && *c == '$'))).map(|(i, _)| i).unwrap_or(t.len());
    &t[..end]
}

/// The top-level statements of an example: each starts on a line at brace
/// depth zero and runs to the line before the next one. Braces inside
/// comments and strings do not count.
fn statements(example: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut cur: String = String::new();
    let mut depth: i32 = 0;
    for line in example.lines() {
        let starts_statement: bool = depth == 0 && !line.trim().is_empty() && !line.starts_with(' ') && !line.starts_with('\t') && !line.starts_with('}');
        if starts_statement && !cur.trim().is_empty() {
            out.push(std::mem::take(&mut cur));
        }
        let mut in_string: bool = false;
        let mut prev: char = ' ';
        let mut skip_next: bool = false;
        for c in line.chars() {
            if skip_next {
                skip_next = false;
            } else if in_string {
                match c {
                    '\u{5c}' => skip_next = true,
                    '"' => in_string = false,
                    _ => {}
                }
            } else if c == '"' {
                in_string = true;
            } else if c == '/' && prev == '/' {
                break;
            } else if c == '{' {
                depth += 1;
            } else if c == '}' {
                depth -= 1;
            }
            prev = c;
        }
        cur.push_str(line);
        cur.push('\n');
    }
    if !cur.trim().is_empty() {
        out.push(cur);
    }
    out
}

/// A whole file made from an example: one that already has a single
/// `View`/`Panel` root (or a root that is a control it declares) is
/// returned as it is; a snippet gets its nodes put inside
/// `View example { }`, with its declarations left at the top level.
fn as_file(example: &str) -> String {
    let all: Vec<String> = statements(example);
    let is_declaration = |s: &String| -> bool {
        let w: &str = first_word(s);
        s.trim_start().starts_with("//") || w.starts_with('$') || DECLARATIONS.contains(&w)
    };
    let declared_controls: Vec<&str> = all.iter().filter(|s| matches!(first_word(s), "control" | "widget")).filter_map(|s| s.split_whitespace().nth(1)).map(|w| w.trim_end_matches('{')).collect();
    let nodes: Vec<&String> = all.iter().filter(|s| !is_declaration(s)).collect();
    if nodes.is_empty() {
        return example.to_string();
    }
    if nodes.len() == 1 {
        let root: &str = first_word(nodes[0]);
        if root == "View" || root == "Panel" || declared_controls.contains(&root) {
            return example.to_string();
        }
    }
    let mut out: String = String::new();
    for s in all.iter().filter(|s| is_declaration(s)) {
        out.push_str(s);
    }
    out.push_str("View example {\n");
    for s in nodes {
        for line in s.lines() {
            out.push_str("    ");
            out.push_str(line);
            out.push('\n');
        }
    }
    out.push_str("}\n");
    out
}

/// Reads a file an example names with `uses "…"`: paths are relative to
/// `docs/examples/`, the sample project the docs speak about.
fn load_example_file(rel: &str) -> Option<String> {
    fs::read_to_string(repo_root().join("docs/examples").join(rel)).ok()
}

fn errors_of(source: &str) -> Vec<Diagnostic> {
    compile(source, &load_example_file).diags.into_iter().filter(|d| matches!(d.severity, Severity::Error)).collect()
}

/// The `$name = value` lines of the docs' shared theme.
fn theme_values() -> String {
    let theme: String = load_example_file("styles/theme.gogo").expect("docs/examples/styles/theme.gogo");
    theme.lines().filter(|l| l.starts_with('$')).map(|l| format!("{l}\n")).collect()
}

/// The errors of an example checked as a whole file.
///
/// A snippet that does not compile alone gets up to two more tries with
/// the docs' shared theme in scope: first with the theme's values only
/// (a snippet may define a style the theme also has), then with the whole
/// theme, as `uses "styles/theme.gogo"` would bring it. The first try
/// with no error is the answer; failing all, the try with the fewest.
fn check_whole(example: &str) -> (String, Vec<Diagnostic>) {
    let file: String = as_file(&strip_elisions(example));
    let mut best: (String, Vec<Diagnostic>) = (file.clone(), errors_of(&file));
    if best.1.is_empty() || file.lines().any(|l| l.trim_start().starts_with("uses ")) {
        return best;
    }
    for with_theme in [format!("{}{file}", theme_values()), format!("uses \"styles/theme.gogo\"\n{file}")] {
        let errors: Vec<Diagnostic> = errors_of(&with_theme);
        if errors.len() < best.1.len() {
            best = (with_theme, errors);
        }
        if best.1.is_empty() {
            break;
        }
    }
    best
}

/// The grammar errors of an example: what the lexer and the parser say,
/// before any name is looked up.
fn grammar_errors(example: &str) -> Vec<Diagnostic> {
    let mut diags: Vec<Diagnostic> = Vec::new();
    gogo_ui::parser::parse(&strip_elisions(example), &mut diags);
    diags.into_iter().filter(|d| matches!(d.severity, Severity::Error)).collect()
}

fn describe(file: &str, errors: &[Diagnostic]) -> String {
    let lines: Vec<&str> = file.lines().collect();
    errors.iter().map(|e| format!("    error[{}]: {}\n        | {}", e.code, e.message, lines.get((e.line as usize).saturating_sub(1)).map(|l| l.trim()).unwrap_or(""))).collect::<Vec<String>>().join("\n")
}

fn markdown_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<PathBuf> = fs::read_dir(dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display())).flatten().map(|e| e.path()).collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            markdown_files(&path, out);
        } else if path.extension().is_some_and(|x| x == "md") {
            out.push(path);
        }
    }
}

/// The Markdown files whose examples are checked, each with its name for
/// messages and whether a fence with no tag counts as gogo there.
fn checked_files() -> Vec<(PathBuf, String, bool)> {
    let root: PathBuf = repo_root();
    let mut out: Vec<(PathBuf, String, bool)> = Vec::new();
    for name in ["README.md", "docs/gogo-spec.md", "docs/REFERENCE.md", "docs/COOKBOOK.md"] {
        out.push((root.join(name), name.to_string(), false));
    }
    let cards_dir: PathBuf = root.join("docs/cards");
    let mut cards: Vec<PathBuf> = Vec::new();
    markdown_files(&cards_dir, &mut cards);
    for path in cards {
        let rel: String = path.strip_prefix(&cards_dir).expect("under cards").to_string_lossy().replace(std::path::MAIN_SEPARATOR, "/");
        out.push((path, format!("docs/cards/{rel}"), true));
    }
    out
}

#[test]
fn every_docs_example_holds() {
    let mut failures: Vec<String> = Vec::new();
    let (mut whole, mut partial, mut shown_errors, mut stale) = (0u32, 0u32, 0u32, 0u32);
    for (path, rel, bare_is_gogo) in checked_files() {
        let text: String = fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        for fence in fences(&text) {
            let tag: &str = fence.words.first().map(String::as_str).unwrap_or("");
            let is_gogo: bool = tag == "kdl" || tag == "gogo" || (tag.is_empty() && bare_is_gogo);
            if !is_gogo {
                continue;
            }
            let mode: &str = fence.words.get(1).map(String::as_str).unwrap_or("");
            if mode == "partial" {
                partial += 1;
                let errors: Vec<Diagnostic> = grammar_errors(&fence.body);
                if !errors.is_empty() {
                    failures.push(format!("{rel}:{} (partial: grammar only)\n{}", fence.line, describe(&strip_elisions(&fence.body), &errors)));
                }
            } else if let Some(code) = mode.strip_prefix("error=") {
                shown_errors += 1;
                let (file, errors) = check_whole(&fence.body);
                if !errors.iter().any(|e| e.code == code) {
                    let got: String = if errors.is_empty() { "no error".to_string() } else { errors.iter().map(|e| e.code.clone()).collect::<Vec<String>>().join(", ") };
                    failures.push(format!("{rel}:{} says it shows {code}, but the compiler reports {got}\n{}", fence.line, describe(&file, &errors)));
                }
            } else if mode == "stale" {
                stale += 1;
                if check_whole(&fence.body).1.is_empty() {
                    failures.push(format!("{rel}:{} is marked `stale` but compiles now: take the word off its fence", fence.line));
                }
            } else if mode.is_empty() {
                whole += 1;
                let (file, errors) = check_whole(&fence.body);
                if !errors.is_empty() {
                    failures.push(format!("{rel}:{}\n{}", fence.line, describe(&file, &errors)));
                }
            } else {
                failures.push(format!("{rel}:{}: `{mode}` is not a word this test knows after the language tag (`partial`, `error=G1234`, `stale`)", fence.line));
            }
        }
    }
    println!("docs examples: {whole} compiled whole, {partial} checked for grammar only, {shown_errors} shown errors, {stale} stale");
    assert!(failures.is_empty(), "{} docs example(s) do not hold:\n\n{}\n", failures.len(), failures.join("\n\n"));
    assert!(whole >= 250, "only {whole} examples were compiled whole: did a docs folder move, or did the examples lose their tags?");
    assert!(partial <= whole / 5, "{partial} examples are `partial` against {whole} compiled whole: a partial example is checked for grammar only, so they should stay the few");
}

/// Every `.gogo` file under `docs/examples` compiles: the docs' shared
/// theme and the example pages the docs link to.
///
/// Needed because those files are what a reader opens to see a whole
/// project, and the snippets above lean on the theme being right.
#[test]
fn the_example_project_compiles() {
    fn gogo_files(dir: &Path, out: &mut Vec<PathBuf>) {
        for path in fs::read_dir(dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display())).flatten().map(|e| e.path()) {
            if path.is_dir() {
                gogo_files(&path, out);
            } else if path.extension().is_some_and(|x| x == "gogo") {
                out.push(path);
            }
        }
    }
    let dir: PathBuf = repo_root().join("docs/examples");
    let mut files: Vec<PathBuf> = Vec::new();
    gogo_files(&dir, &mut files);
    assert!(!files.is_empty(), "no .gogo file under {}", dir.display());
    let mut failures: Vec<String> = Vec::new();
    for path in files {
        let source: String = fs::read_to_string(&path).expect("example file");
        let errors: Vec<Diagnostic> = errors_of(&source);
        if !errors.is_empty() {
            failures.push(format!("{}\n{}", path.display(), describe(&source, &errors)));
        }
    }
    assert!(failures.is_empty(), "example files that do not compile:\n\n{}\n", failures.join("\n\n"));
}

/// The helpers above are run on inputs with known answers.
///
/// Needed so that "every example holds" cannot pass because a helper
/// quietly read nothing or wrapped nothing.
#[test]
fn the_example_helpers_do_what_they_say() {
    let md: &str = "text\n```kdl\nButton ok text=\"OK\"\n```\n- item\n  ```kdl partial\n  Label a\n  ```\n```rust\nlet x = 1;\n```\n";
    let found: Vec<Fence> = fences(md);
    assert_eq!(found.len(), 3);
    assert_eq!((found[0].line, found[0].words.join(" "), found[0].body.as_str()), (2, "kdl".to_string(), "Button ok text=\"OK\"\n"));
    assert_eq!((found[1].words.join(" "), found[1].body.as_str()), ("kdl partial".to_string(), "Label a\n"));
    assert_eq!(found[2].words, ["rust"]);

    assert_eq!(strip_elisions("Row {\n    ...\n    ...recents...\n}\nColumn { ... }\n"), "Row {\n}\nColumn { }\n");

    let snippet: &str = "style Big font_size=30\n$gap = 8\nLabel a .Big text=\"x\"\nLabel b text=\"y\"\n";
    assert_eq!(as_file(snippet), "style Big font_size=30\n$gap = 8\nView example {\n    Label a .Big text=\"x\"\n    Label b text=\"y\"\n}\n");
    let page: &str = "View v {\n    Label a text=\"x\"\n}\n";
    assert_eq!(as_file(page), page);

    assert!(errors_of(page).is_empty());
    assert!(errors_of("View v {\n    Label a no_such_word=1\n}\n").iter().any(|e| e.code == "G2001"));
    assert!(grammar_errors("Label a text=\"x\"\n").is_empty());
    assert!(!grammar_errors("Label a text=\n").is_empty());
}
