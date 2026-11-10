//! Hot reload, the gogo half (`gogo::reload`): a page recompiled against a
//! build's tables indexes them the way the baked page does; a key or a
//! picture not in the build is refused AT ITS LINE; a changed node list is
//! refused naming the node; the runtime's state carries into the new
//! page. The host's folder watch and toast are the host's tests.
use std::cell::RefCell;

use gogo::reload::{BuildTables, ShapeChange, recompile, same_shape};
use gogo::runtime::{Runtime, SELECTED, resolve};
use gogo::{ArrangementId, CompileOptions, Images, Keys, Kind, Page, PropKey, Value, compile_with};

const THEME: &str = "$accent = #FF0000FF\n";

/// Line 5 names the key, line 6 the picture — the refusals point there.
const PAGE: &str = r#"uses "theme.gogo"
arrangement names
View root {
    Column col {
        Label title text="Icon Size"/iconSize, font_color=$accent
        Image icon source="res://a.png"
        Button go text="OK"/ok
        Scroll list { Label inner text="x"/_ }
    }
}
"#;

fn loader(rel: &str) -> Option<String> {
    (rel == "theme.gogo").then(|| THEME.to_string())
}

/// "The build": the page compiled the way a build script does, filling
/// shared key and image tables, plus those tables as a `BuildTables` would carry
/// them in a binary.
struct Build {
    page: Page,
    keys: Vec<(String, String)>,
    images: Vec<String>,
}

impl Build {
    fn new() -> Build {
        let keys = RefCell::new(Keys::default());
        let images = RefCell::new(Images::default());
        let compiled = compile_with(PAGE, &loader, CompileOptions { keys: Some(&keys), images: Some(&images), require_keys: true, source_name: "page.gogo", ..CompileOptions::default() });
        assert!(!gogo::has_errors(&compiled.diags), "{:#?}", compiled.diags);
        let keys = keys.into_inner();
        let key_pairs = keys.names.iter().cloned().zip(keys.english.iter().cloned()).collect();
        Build { page: compiled.page, keys: key_pairs, images: images.into_inner().names }
    }

    fn recompile(&self, source: &str) -> Result<Page, Vec<gogo::Diagnostic>> {
        let keys: Vec<(&str, &str)> = self.keys.iter().map(|(n, e)| (n.as_str(), e.as_str())).collect();
        let images: Vec<&str> = self.images.iter().map(String::as_str).collect();
        let baked = BuildTables { keys: &keys, images: &images, code_controls: &[], require_keys: true };
        recompile(source, "page.gogo", &loader, &baked)
    }
}

fn prop(page: &Page, name: &str, key: PropKey) -> Value {
    page.prop(page.find(name).expect(name), key).unwrap_or_else(|| panic!("{name} has no {key:?}"))
}

#[test]
fn a_recompiled_page_has_the_baked_shape_and_indexes_the_builds_tables() {
    let build = Build::new();
    assert_eq!(build.keys.len(), 2, "iconSize and ok (`/_` is no key)");
    assert_eq!(build.images.len(), 1);
    let fresh = build.recompile(PAGE).unwrap();
    same_shape(&build.page, &fresh).unwrap();
    for (name, key) in [("title", PropKey::Text), ("go", PropKey::Text), ("icon", PropKey::Source)] {
        let (baked, hot) = (prop(&build.page, name, key), prop(&fresh, name, key));
        match (baked, hot) {
            (Value::VText { key: a, .. }, Value::VText { key: b, .. }) => assert_eq!(a, b, "{name}: the same key id as the build"),
            (Value::VImage(a), Value::VImage(b)) => assert_eq!(a, b, "{name}: the same picture id as the build"),
            other => panic!("{name}: {other:?}"),
        }
    }
    // A reload that only changes a value is the point: no error, new value.
    let fresh = build.recompile(&PAGE.replace("font_color=$accent", "font_color=#00FF00FF")).unwrap();
    assert_eq!(prop(&fresh, "title", PropKey::FontColor), Value::VColor(gogo::Color_Straight_Srgb_4xU8_RGBA([0, 255, 0, 255])));
}

#[test]
fn a_new_key_a_new_picture_and_a_typo_are_refused_at_their_line() {
    let build = Build::new();
    let errs = build.recompile(&PAGE.replace("/iconSize", "/iconSizeBrandNew")).unwrap_err();
    assert_eq!(errs.len(), 1, "{errs:#?}");
    assert_eq!((errs[0].code.as_str(), errs[0].line), ("G8004", 5), "{}", errs[0]);
    assert!(errs[0].message.contains("iconSizeBrandNew") && errs[0].message.contains("rebuild"), "{}", errs[0]);

    let errs = build.recompile(&PAGE.replace("res://a.png", "res://not_in_this_build.png")).unwrap_err();
    assert_eq!(errs.len(), 1, "{errs:#?}");
    assert_eq!((errs[0].code.as_str(), errs[0].line), ("G2021", 6), "{}", errs[0]);
    assert!(errs[0].message.contains("not_in_this_build.png") && errs[0].message.contains("rebuild"), "{}", errs[0]);

    let errs = build.recompile(&PAGE.replace("font_color=$accent", "font_colour=$accent")).unwrap_err();
    assert!(errs.iter().all(|d| d.severity == gogo::Severity::Error), "{errs:#?}");
    assert_eq!(errs[0].line, 5, "a compile error keeps its line: {}", errs[0]);
    assert!(errs[0].code.starts_with('G'), "{}", errs[0]);
}

#[test]
fn a_changed_node_list_is_refused_naming_the_node() {
    let build = Build::new();
    let extra = build.recompile(&PAGE.replace("        Button go", "        Rectangle extra\n        Button go")).unwrap();
    let err = same_shape(&build.page, &extra).unwrap_err();
    assert_eq!(err, ShapeChange::Count { old: build.page.nodes.len(), new: build.page.nodes.len() + 1 });
    assert!(err.to_string().contains("adding or removing a node needs a rebuild"), "{err}");

    let renamed = build.recompile(&PAGE.replace("Button go ", "Button go2 ")).unwrap();
    let err = same_shape(&build.page, &renamed).unwrap_err();
    let ShapeChange::Node { old_kind, old_name, new_kind, new_name, .. } = &err else { panic!("{err}") };
    assert_eq!((old_kind, old_name.as_str(), new_kind, new_name.as_str()), (&Kind::Button, "go", &Kind::Button, "go2"));
    assert!(err.to_string().contains("`go` -> Button `go2`"), "{err}");

    let rekinded = build.recompile(&PAGE.replace("Button go ", "Check go ")).unwrap();
    let ShapeChange::Node { new_kind, .. } = same_shape(&build.page, &rekinded).unwrap_err() else { panic!() };
    assert_eq!(new_kind, Kind::Check);
}

#[test]
fn the_runtimes_state_carries_over_a_reload() {
    let build = Build::new();
    let mut old_page = build.page.clone();
    let (col, list, go, title) = (old_page.find("col").unwrap(), old_page.find("list").unwrap(), old_page.find("go").unwrap(), old_page.find("title").unwrap());
    let mut old = Runtime::new(&old_page);
    old.set_prop_instant(col, PropKey::Spacing, Value::VNum(4.0));
    old.set_scroll(list, [0.0, 321.0]);
    old.set_state(go, SELECTED, true);
    old.set_text(&mut old_page, title, "hello from before");
    old.go_instant_id(ArrangementId::new(0)); // `names`
    old.safe = [1.0, 2.0, 3.0, 4.0];

    let mut fresh = build.recompile(&PAGE.replace("font_color=$accent", "font_color=#00FF00FF")).unwrap();
    let rt = old.carried_to(&old_page, &mut fresh);
    let look = resolve(&fresh, &rt);
    assert_eq!(look.prop(&fresh, col, PropKey::Spacing), Some(Value::VNum(4.0)), "a code write carries");
    assert_eq!(rt.scroll_of(list), [0.0, 321.0], "the scroll carries");
    assert!(rt.has(go, SELECTED), "a state bit carries");
    let Some(Value::VText { text, .. }) = look.prop(&fresh, title, PropKey::Text) else { panic!("the set text carries") };
    assert_eq!(fresh.str(text), "hello from before", "re-interned into the new page's strings");
    assert_eq!(rt.arrangement, ArrangementId::new(0), "the arrangement carries");
    assert_eq!(rt.safe, [1.0, 2.0, 3.0, 4.0]);
    assert!(rt.dirty, "the next frame lays out");
    assert_eq!(look.prop(&fresh, title, PropKey::FontColor), Some(Value::VColor(gogo::Color_Straight_Srgb_4xU8_RGBA([0, 255, 0, 255]))), "the edit shows");
}
