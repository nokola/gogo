//! G8002: with `CompileOptions::require_keys` on, every string a file
//! displays carries a translation key — `/key`, or `/_` for text that
//! stays English on purpose — unless the file starts with `allow
//! untranslated`. The app's build turns it on so no English-only text
//! ships by accident (2026-09-29: the audit found ~190 page strings and
//! ~75 code strings with no key).
use gogo::page::{NO_KEY, PropKey, Value};
use gogo::{CompileOptions, Compiled, compile_with};

fn build(src: &str, files: &[(&str, &str)], require_keys: bool) -> Compiled {
    let loader = |path: &str| files.iter().find(|(p, _)| *p == path).map(|(_, text)| text.to_string());
    compile_with(src, &loader, CompileOptions { require_keys, source_name: "page.gogo", ..CompileOptions::default() })
}

fn codes(c: &Compiled) -> Vec<String> {
    c.diags.iter().map(|d| d.code.clone()).collect()
}

fn g8002(c: &Compiled) -> Vec<String> {
    c.diags.iter().filter(|d| d.code == "G8002").map(|d| d.message.clone()).collect()
}

#[test]
fn text_with_no_key_is_an_error_when_keys_are_required() {
    let c = build("View v {\n Label a text=\"Hello\"\n}\n", &[], true);
    assert_eq!(g8002(&c).len(), 1, "{:#?}", c.diags);
    assert_eq!(c.diags.iter().find(|d| d.code == "G8002").map(|d| d.line), Some(2));
}

#[test]
fn nothing_is_checked_when_keys_are_not_required() {
    let c = build("View v {\n Label a text=\"Hello\"\n}\n", &[], false);
    assert!(g8002(&c).is_empty(), "{:#?}", c.diags);
}

#[test]
fn a_key_or_the_discard_passes() {
    let c = build("View v {\n Label a text=\"Hello\"/hello\n Label b text=\"test only\"/_\n}\n", &[], true);
    assert!(g8002(&c).is_empty(), "{:#?}", c.diags);
}

/// `/_` is not a key named `_`: two discards with different English are
/// fine (before, the second was G8001), and the text carries no key.
#[test]
fn the_discard_is_no_key_at_all() {
    let c = build("View v {\n Label a text=\"one\"/_\n Label b text=\"two\"/_\n}\n", &[], false);
    assert!(!codes(&c).contains(&"G8001".to_string()), "{:#?}", c.diags);
    let keyed: Vec<_> = c.page.props.iter().filter_map(|p| match p.value {
        Value::VText { key, .. } if p.key == PropKey::Text => Some(key),
        _ => None,
    }).collect();
    assert_eq!(keyed, vec![NO_KEY, NO_KEY]);
}

#[test]
fn allow_untranslated_skips_the_whole_file() {
    let c = build("allow untranslated\nView v {\n Label a text=\"Hello\"\n Label b text=\"World\"\n}\n", &[], true);
    assert!(codes(&c).is_empty(), "{:#?}", c.diags);
}

/// Each file answers for the text written in it: a widget body in an
/// `allow untranslated` file passes, the page's own instance line does
/// not; and the other way round.
#[test]
fn allow_covers_only_the_file_that_says_it() {
    let widget = "allow untranslated\nwidget Card $title=\"\" {\n Column {\n  Label t text=$title\n  Label note text=\"dev note\"\n }\n}\n";
    let c = build("uses \"card.gogo\"\nView v {\n Card c title=\"Welcome\"\n}\n", &[("card.gogo", widget)], true);
    let errs = g8002(&c);
    assert_eq!(errs.len(), 1, "{errs:#?}");
    assert!(!errs[0].contains("card.gogo") && errs[0].contains("Welcome"), "{errs:#?}");

    let widget = "widget Card $title=\"\" {\n Column {\n  Label t text=$title\n  Label note text=\"dev note\"\n }\n}\n";
    let c = build("allow untranslated\nuses \"card.gogo\"\nView v {\n Card c title=\"Welcome\"\n}\n", &[("card.gogo", widget)], true);
    let errs = g8002(&c);
    assert_eq!(errs.len(), 1, "{errs:#?}");
    assert!(errs[0].contains("card.gogo") && errs[0].contains("dev note"), "{errs:#?}");
}

/// Paths, host picture addresses, `areas=` rows and text with no letter
/// need no key.
#[test]
fn paths_rows_and_letterless_text_are_not_checked() {
    let c = build("$shots = [\"collage://before\"]\nView v {\n Label a text=\"100%\"\n Label b text=\"1/1\"\n Image i source=\"res://a.png\", 10x10\n}\n", &[], true);
    assert!(g8002(&c).is_empty(), "{:#?}", c.diags);
}

/// A `when` response and a theme value are displayed text too; a theme
/// value may say `/_` (a real key there is still G8001).
#[test]
fn when_responses_and_theme_values_are_checked() {
    let c = build("$greeting = \"Hi\"\nView v {\n Label a text=$greeting\n}\nwhen pressed a.text = \"Bye\"\n", &[], true);
    assert_eq!(g8002(&c).len(), 2, "{:#?}", c.diags);
    let c = build("$greeting = \"Hi\"/_\nView v {\n Label a text=$greeting\n}\n", &[], true);
    assert!(codes(&c).iter().all(|c| c != "G8002" && c != "G8001"), "{:#?}", c.diags);
}

#[test]
fn allow_takes_one_word_at_the_top_of_a_file() {
    let c = build("allow translations\nView v {\n}\n", &[], false);
    assert!(codes(&c).contains(&"G8003".to_string()), "{:#?}", c.diags);
    let c = build("View v {\n allow untranslated\n}\n", &[], false);
    assert!(codes(&c).contains(&"G8003".to_string()), "{:#?}", c.diags);
}
