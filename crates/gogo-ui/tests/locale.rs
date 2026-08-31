//! Pins for translations (docs/todo-gogo-parity.md GPAR12): a keyed label
//! is measured at its TRANSLATED width, a literal stays as written under
//! any locale, a code-set key follows the locale, keys are one id across
//! files (and G8001 holds across files), and switching the locale is a
//! relayout. What a user would have seen without these: German captions
//! clipped at their English width, or an English word on a French phone.
use std::cell::RefCell;
use std::sync::Arc;

use gogo::draw::{Draw, draw};
use gogo::layout::{FixedAdvance, Rect, layout};
use gogo::locale::{KeyName, Keys, Locale, display};
use gogo::runtime::{Runtime, resolve};
use gogo::{CompileOptions, Compiled, NO_KEY, Page, PropKey, Value, compile, compile_with};

static TABLE: &[(&str, &str)] = &[("ok", "OK"), ("size", "Size")];

/// A stand-in for the build-emitted `Key` enum.
#[derive(Clone, Copy)]
struct K(u32);
impl KeyName for K {
    fn id(self) -> u32 {
        self.0
    }
}

fn ok(src: &str) -> Compiled {
    let c = compile(src, &|_| None);
    assert!(!gogo::has_errors(&c.diags), "{:#?}", c.diags);
    c
}

fn rects(page: &Page, rt: &Runtime) -> Vec<Rect> {
    let res = resolve(page, rt);
    layout(page, &res, Rect::new(0.0, 0.0, 400.0, 400.0), &FixedAdvance)
}

fn shown<'a>(page: &'a Page, rt: &'a Runtime, name: &str) -> &'a str {
    let res = resolve(page, rt);
    let id = page.find(name).unwrap();
    match res.prop(page, id, PropKey::Text) {
        Some(Value::VText { text, key }) => display(page, rt.locale.as_deref(), text, key),
        other => panic!("{name}: no text, {other:?}"),
    }
}

/// `font_size=10` under FixedAdvance = 5.5 epx per char: "OK" is 11 wide,
/// "Bestätigen" (10 chars) is 55.
#[test]
fn a_keyed_label_is_measured_at_its_translated_width() {
    let c = ok("View v { Label a text=\"OK\"/ok, font_size=10, halign=left, valign=top }");
    let page = c.page;
    let a = page.find("a").unwrap() as usize;
    let mut rt = Runtime::new(&page);
    assert_eq!(rects(&page, &rt)[a].w, 11.0, "no locale: the literal's width");
    rt.set_locale(Some(Arc::new(Locale::build(TABLE, &[(0, "Bestätigen")]))));
    assert_eq!(rects(&page, &rt)[a].w, 55.0, "with a locale: the translation's width, not the English one");
    assert_eq!(shown(&page, &rt, "a"), "Bestätigen");
    // The draw command still carries the ids; the renderer resolves them
    // through the same `display`, so what is measured is what is drawn.
    let res = resolve(&page, &rt);
    let list = draw(&page, &res, &rects(&page, &rt));
    let text = list.iter().find_map(|d| match d {
        Draw::Text { text, key, .. } => Some(display(&page, rt.locale.as_deref(), *text, *key)),
        _ => None,
    });
    assert_eq!(text, Some("Bestätigen"));
}

#[test]
fn a_key_the_locale_lacks_shows_english_and_a_literal_ignores_the_locale() {
    let c = ok("View v { Label a text=\"OK\"/ok; Label b text=\"plain\" }");
    let page = c.page;
    let mut rt = Runtime::new(&page);
    rt.set_locale(Some(Arc::new(Locale::build(TABLE, &[(1, "Größe")]))));
    assert_eq!(shown(&page, &rt, "a"), "OK", "key 0 untranslated: English");
    assert_eq!(shown(&page, &rt, "b"), "plain", "no key: the literal, whatever the locale");
}

#[test]
fn set_key_follows_the_locale_and_set_text_is_a_literal() {
    let c = ok("View v { Label a text=\"x\"; Label b text=\"OK\"/ok }");
    let mut page = c.page;
    let a = page.find("a").unwrap();
    let b = page.find("b").unwrap();
    let mut rt = Runtime::new(&page);
    rt.set_key(a, K(1));
    assert_eq!(shown(&page, &rt, "a"), "", "a code-set key with no locale has no string to show");
    rt.set_locale(Some(Arc::new(Locale::english(TABLE))));
    assert_eq!(shown(&page, &rt, "a"), "Size", "English-only locale: the table's English");
    rt.set_locale(Some(Arc::new(Locale::build(TABLE, &[(1, "Größe"), (0, "Okay")]))));
    assert_eq!(shown(&page, &rt, "a"), "Größe");
    assert_eq!(shown(&page, &rt, "b"), "Okay");
    // `set_text` = a literal: the key is gone, the locale does not apply.
    rt.set_text(&mut page, b, "Version 1.2");
    assert_eq!(shown(&page, &rt, "b"), "Version 1.2");
    let res = resolve(&page, &rt);
    assert!(matches!(res.prop(&page, b, PropKey::Text), Some(Value::VText { key: NO_KEY, .. })), "set_text writes NO_KEY");
}

#[test]
fn switching_the_locale_is_a_relayout() {
    let c = ok("View v { Label a text=\"OK\"/ok }");
    let page = c.page;
    let mut rt = Runtime::new(&page);
    rt.dirty = false;
    rt.set_locale(Some(Arc::new(Locale::english(TABLE))));
    assert!(rt.dirty, "a new language re-measures every keyed label");
    rt.dirty = false;
    rt.set_locale(None);
    assert!(rt.dirty);
}

/// Two files compiled against one shared table: `/ok` is ONE id in both
/// (the id a page bakes indexes the build's `KEYS`), and a second English
/// string for it in the other file is G8001 — across files, not per file.
#[test]
fn keys_are_global_across_files_and_g8001_crosses_files() {
    let keys = RefCell::new(Keys::default());
    let options = CompileOptions { keys: Some(&keys), ..CompileOptions::default() };
    let a = compile_with("View v { Label x text=\"Size\"/size; Label y text=\"OK\"/ok }", &|_| None, options);
    let b = compile_with("View w { Label z text=\"OK\"/ok }", &|_| None, options);
    assert!(!gogo::has_errors(&a.diags) && !gogo::has_errors(&b.diags), "{:#?} {:#?}", a.diags, b.diags);
    let key_of = |c: &Compiled, name: &str| match c.page.prop(c.page.find(name).unwrap(), PropKey::Text) {
        Some(Value::VText { key, .. }) => key,
        other => panic!("{other:?}"),
    };
    assert_eq!(key_of(&a, "x"), 0);
    assert_eq!(key_of(&a, "y"), 1);
    assert_eq!(key_of(&b, "z"), 1, "`/ok` in the second file is the same id");
    assert_eq!(keys.borrow().names, vec!["size".to_string(), "ok".to_string()]);
    let drift = compile_with("View u { Label q text=\"Okay\"/ok }", &|_| None, options);
    assert!(drift.diags.iter().any(|d| d.code == "G8001" && d.message.contains("carries \"OK\" elsewhere")), "{:#?}", drift.diags);
    // A table seeded from a build's KEYS gives a runtime compile the same ids.
    let seeded = RefCell::new(Keys::from_table(&[("size", "Size"), ("ok", "OK")]));
    let hot = compile_with("View h { Label z text=\"OK\"/ok }", &|_| None, CompileOptions { keys: Some(&seeded), ..CompileOptions::default() });
    assert_eq!(key_of(&hot, "z"), 1);
}
