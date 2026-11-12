//! The VS Code grammar (`vscode/syntaxes/gogo.tmLanguage.json`) names
//! gogo's node kinds by hand — a TextMate grammar is a JSON file, it
//! cannot read `KIND_WORDS`. This keeps the two equal, so a kind added to
//! `page.rs` cannot stay highlighted as an unknown widget: the test fails
//! with the list to paste.
use gogo_ui::page::KIND_WORDS;

const GRAMMAR: &str = include_str!("../../../vscode/syntaxes/gogo.tmLanguage.json");

/// The `(a|b|c)` word list of the named repository rule: the first group
/// after `\s*` in its `match`. The grammar writes every head rule as
/// `(?:^|(?<=[{;]))\s*(word|word|...)\b`, so this is the one shape to find.
fn head_words(rule: &str) -> Vec<&'static str> {
    let at = GRAMMAR.find(&format!("\"{rule}\"")).unwrap_or_else(|| panic!("the grammar has no `{rule}` rule"));
    let rest = &GRAMMAR[at..];
    let open = rest.find("\\\\s*(").unwrap_or_else(|| panic!("`{rule}` has no `\\s*(` word list")) + 5;
    let close = rest[open..].find(')').unwrap();
    rest[open..open + close].split('|').collect()
}

#[test]
fn the_grammar_names_every_builtin_kind_and_only_those() {
    let capitalized: Vec<&str> = KIND_WORDS.iter().map(|(w, _)| *w).filter(|w| w.starts_with(|c: char| c.is_ascii_uppercase())).collect();
    assert_eq!(head_words("builtin-type"), capitalized, "paste into the `builtin-type` rule: ({})", capitalized.join("|"));

    let lowercase: Vec<&str> = KIND_WORDS.iter().map(|(w, _)| *w).filter(|w| w.starts_with(|c: char| c.is_ascii_lowercase())).collect();
    let mut structure = head_words("structure-keyword");
    structure.extend(head_words("control-keyword"));
    for word in lowercase {
        assert!(structure.contains(&word), "`{word}` (a lowercase kind) is in neither the `structure-keyword` nor the `control-keyword` rule");
    }
}

/// The first `(a|b|c)` word list in the named repository rule's `match`,
/// wherever it stands in the pattern.
fn rule_words(rule: &str) -> Vec<&'static str> {
    let at = GRAMMAR.find(&format!("\"{rule}\"")).unwrap_or_else(|| panic!("the grammar has no `{rule}` rule"));
    let rest = &GRAMMAR[at..];
    let pattern = &rest[rest.find("\"match\"").unwrap_or_else(|| panic!("`{rule}` has no `match`"))..];
    let pattern = &pattern[..pattern.find('\n').unwrap_or(pattern.len())];
    let mut from = 0;
    while let Some(open) = pattern[from..].find('(') {
        let start = from + open + 1;
        let close = start + pattern[start..].find(')').unwrap_or(0);
        let inner = &pattern[start..close];
        if inner.contains('|') && inner.chars().all(|c| c.is_ascii_alphabetic() || c == '_' || c == '|') {
            return inner.split('|').collect();
        }
        from = start;
    }
    panic!("`{rule}` has no `(a|b|c)` word list");
}

/// Every word the grammar colors as a keyword is refused as a widget's
/// name: an instance of a widget named `each` or `held` would be read,
/// and colored, as the keyword.
#[test]
fn a_word_the_grammar_colors_as_a_keyword_cannot_name_a_widget() {
    for rule in ["control-keyword", "structure-keyword", "clause-keyword", "state-word", "boolean", "flag", "layout-word"] {
        let words = rule_words(rule);
        assert!(words.len() >= 2, "`{rule}`: {words:?}");
        for word in words {
            let src = format!("widget {word} {{\n Label text=\"x\"\n}}\nView v {{ Label l text=\"y\" }}");
            let diags = gogo_ui::compile(&src, &|_| None).diags;
            assert!(diags.iter().any(|d| d.code == "G5001" && d.line == 1), "`widget {word}` (a `{rule}` word) builds: {diags:?}");
        }
    }
}

#[test]
fn the_extension_declares_the_gogo_extension_and_the_grammar_file() {
    let manifest = include_str!("../../../vscode/package.json");
    assert!(manifest.contains("\".gogo\""), "package.json registers .gogo");
    assert!(manifest.contains("./syntaxes/gogo.tmLanguage.json") && manifest.contains("\"source.gogo\""));
    assert!(GRAMMAR.contains("\"scopeName\": \"source.gogo\""));
}
