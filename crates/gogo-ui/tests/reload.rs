//! Hot reload, the pieces that need no feature (`gogo_ui::reload`): a page
//! compiled again against a build's tables indexes them the way the built
//! page does; a key, a picture or an atlas line not in the build is
//! refused AT ITS LINE; a changed node list is refused naming the node;
//! the runtime's state carries into the new page, list rows included. The
//! `Reloader` on top of these is `reloader.rs` (feature on) and
//! `reloader_off.rs` (feature off).
mod reload_kit;

use gogo_ui::reload::{BuiltShape, ShapeChange, Why, same_shape, swap};
use gogo_ui::runtime::{Item, ItemKey, ItemValue, Runtime, SELECTED, resolve};
use gogo_ui::{ArrangementId, ImageKey, Kind, NodeId, Page, PropKey, Value};
use reload_kit::{Kit, build, png};

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

fn plain() -> Kit {
    build(&[("page.gogo", PAGE)], &[("theme.gogo", THEME)], &[])
}

fn prop(page: &Page, name: &str, key: PropKey) -> Value {
    page.prop(page.find(name).expect(name), key).unwrap_or_else(|| panic!("{name} has no {key:?}"))
}

#[test]
fn a_recompiled_page_has_the_baked_shape_and_indexes_the_builds_tables() {
    let kit = plain();
    assert_eq!(kit.tables.keys.len(), 2, "iconSize and ok (`/_` is no key)");
    assert_eq!(kit.tables.images.len(), 1);
    let built: Page = kit.page("page.gogo");
    let fresh = kit.recompile("page.gogo").unwrap();
    same_shape(&built, &fresh).unwrap();
    for (name, key) in [("title", PropKey::Text), ("go", PropKey::Text), ("icon", PropKey::Source)] {
        let (baked, hot) = (prop(&built, name, key), prop(&fresh, name, key));
        match (baked, hot) {
            (Value::VText { key: a, .. }, Value::VText { key: b, .. }) => assert_eq!(a, b, "{name}: the same key id as the build"),
            (Value::VImage(a), Value::VImage(b)) => assert_eq!(a, b, "{name}: the same picture id as the build"),
            other => panic!("{name}: {other:?}"),
        }
    }
    // A reload that only changes a value is the point: no error, new value.
    kit.edit("page.gogo", "font_color=$accent", "font_color=#00FF00FF");
    let fresh = kit.recompile("page.gogo").unwrap();
    assert_eq!(prop(&fresh, "title", PropKey::FontColor), Value::VColor(gogo_ui::Color_Straight_Srgb_4xU8_RGBA([0, 255, 0, 255])));
}

#[test]
fn a_new_key_a_new_picture_and_a_typo_are_refused_at_their_line() {
    let kit = plain();
    kit.save("page.gogo", &PAGE.replace("/iconSize", "/iconSizeBrandNew"));
    let errs = kit.recompile("page.gogo").unwrap_err();
    assert_eq!(errs.len(), 1, "{errs:#?}");
    assert_eq!((errs[0].code.as_str(), errs[0].line), ("G8004", 5), "{}", errs[0]);
    assert!(errs[0].message.contains("iconSizeBrandNew") && errs[0].message.contains("rebuild"), "{}", errs[0]);

    kit.save("page.gogo", &PAGE.replace("res://a.png", "res://not_in_this_build.png"));
    let errs = kit.recompile("page.gogo").unwrap_err();
    assert_eq!(errs.len(), 1, "{errs:#?}");
    assert_eq!((errs[0].code.as_str(), errs[0].line), ("G2021", 6), "{}", errs[0]);
    assert!(errs[0].message.contains("not_in_this_build.png") && errs[0].message.contains("rebuild"), "{}", errs[0]);

    kit.save("page.gogo", &PAGE.replace("font_color=$accent", "font_colour=$accent"));
    let errs = kit.recompile("page.gogo").unwrap_err();
    assert!(errs.iter().all(|d| d.severity == gogo_ui::Severity::Error), "{errs:#?}");
    assert_eq!(errs[0].line, 5, "a compile error keeps its line: {}", errs[0]);
    assert!(errs[0].code.starts_with('G'), "{}", errs[0]);
}

#[test]
fn a_changed_node_list_is_refused_naming_the_node() {
    let kit = plain();
    let built: Page = kit.page("page.gogo");
    kit.save("page.gogo", &PAGE.replace("        Button go", "        Rectangle extra\n        Button go"));
    let extra = kit.recompile("page.gogo").unwrap();
    let err = same_shape(&built, &extra).unwrap_err();
    assert_eq!(err, ShapeChange::Count { old: built.nodes.len(), new: built.nodes.len() + 1 });
    assert!(err.to_string().contains("adding or removing a node needs a rebuild"), "{err}");

    kit.save("page.gogo", &PAGE.replace("Button go ", "Button go2 "));
    let renamed = kit.recompile("page.gogo").unwrap();
    let err = same_shape(&built, &renamed).unwrap_err();
    let ShapeChange::Node { old_kind, old_name, new_kind, new_name, .. } = &err else { panic!("{err}") };
    assert_eq!((old_kind, old_name.as_str(), new_kind, new_name.as_str()), (&Kind::Button, "go", &Kind::Button, "go2"));
    assert!(err.to_string().contains("`go` -> Button `go2`"), "{err}");

    kit.save("page.gogo", &PAGE.replace("Button go ", "Check go "));
    let rekinded = kit.recompile("page.gogo").unwrap();
    let ShapeChange::Node { new_kind, .. } = same_shape(&built, &rekinded).unwrap_err() else { panic!() };
    assert_eq!(new_kind, Kind::Check);
}

#[test]
fn the_runtimes_state_carries_over_a_reload() {
    let kit = plain();
    let mut old_page: Page = kit.page("page.gogo");
    let (col, list, go, title) = (old_page.find("col").unwrap(), old_page.find("list").unwrap(), old_page.find("go").unwrap(), old_page.find("title").unwrap());
    let mut old = Runtime::new(&old_page);
    old.set_prop_instant(col, PropKey::Spacing, Value::VNum(4.0));
    old.set_scroll(list, [0.0, 321.0]);
    old.set_state(go, SELECTED, true);
    old.set_text(&mut old_page, title, "hello from before");
    old.go_instant_id(ArrangementId::new(0)); // `names`
    old.safe = [1.0, 2.0, 3.0, 4.0];

    kit.edit("page.gogo", "font_color=$accent", "font_color=#00FF00FF");
    let mut fresh = kit.recompile("page.gogo").unwrap();
    let rt = old.carried_to(&old_page, &mut fresh).unwrap();
    let look = resolve(&fresh, &rt);
    assert_eq!(look.prop(&fresh, col, PropKey::Spacing), Some(Value::VNum(4.0)), "a code write carries");
    assert_eq!(rt.scroll_of(list), [0.0, 321.0], "the scroll carries");
    assert!(rt.has(go, SELECTED), "a state bit carries");
    let Some(Value::VText { text, .. }) = look.prop(&fresh, title, PropKey::Text) else { panic!("the set text carries") };
    assert_eq!(fresh.str(text), "hello from before", "re-interned into the new page's strings");
    assert_eq!(rt.arrangement, ArrangementId::new(0), "the arrangement carries");
    assert_eq!(rt.safe, [1.0, 2.0, 3.0, 4.0]);
    assert!(rt.dirty, "the next frame lays out");
    assert_eq!(look.prop(&fresh, title, PropKey::FontColor), Some(Value::VColor(gogo_ui::Color_Straight_Srgb_4xU8_RGBA([0, 255, 0, 255]))), "the edit shows");
}

// ----- list rows -----------------------------------------------------------

/// A List with keyed rows the code fills from items, and a counted repeat
/// whose rows the code writes itself.
const LIST: &str = r#"View root {
    Column col {
        List picker width=200, height=120, select=one {
            empty { }
            repeat tasks by $item.id {
                ListItem { Label title text=$item.title }
            }
        }
        repeat sliders capacity=2 {
            Row srow height=20 { Label cap text="x"/_ }
        }
    }
}
"#;

fn task(id: i64, title: &str, note: &str) -> Item {
    Item::Record(vec![("id".to_string(), ItemValue::Int(id)), ("title".to_string(), ItemValue::Str(title.to_string())), ("note".to_string(), ItemValue::Str(note.to_string()))])
}

/// The LIST page running: three tasks, the second selected, two slider
/// rows with a caption the code wrote on the second, the List scrolled.
struct Listed {
    kit: Kit,
    page: Page,
    rt: Runtime,
    picker: NodeId,
    tasks: NodeId,
    sliders: NodeId,
}

impl Listed {
    fn new() -> Listed {
        let kit = build(&[("page.gogo", LIST)], &[], &[]);
        let mut page: Page = kit.page("page.gogo");
        let mut rt = Runtime::new(&page);
        let (picker, tasks, sliders) = (page.find("picker").unwrap(), page.find("tasks").unwrap(), page.find("sliders").unwrap());
        rt.set_items(&mut page, tasks, &[task(1, "one", "first"), task(2, "two", "second"), task(3, "three", "third")]).unwrap();
        rt.set_count(&mut page, sliders, 2);
        let caption: NodeId = page.children(rt.items(&page, sliders)[1]).next().unwrap();
        rt.set_text(&mut page, caption, "from code");
        assert_eq!(rt.set_selected(&page, picker, &[ItemKey::Int(2)]), Ok(true));
        rt.set_scroll(picker, [0.0, 12.0]);
        Listed { kit, page, rt, picker, tasks, sliders }
    }

    /// The text each task row's label shows, in the order shown.
    fn titles(&self) -> Vec<String> {
        self.rt.items(&self.page, self.tasks).iter().map(|row| self.rt.text_of(&self.page, self.page.children(*row).next().unwrap()).unwrap_or("").to_string()).collect()
    }

    /// Saves the page with `old` replaced by `new` and reloads it.
    fn reload(&mut self, old: &str, new: &str) -> Result<(), Why> {
        self.kit.edit("page.gogo", old, new);
        let fresh: Page = self.kit.recompile("page.gogo").expect("the edit compiles");
        swap(&mut self.page, &mut self.rt, fresh)
    }
}

/// A running page has grown by its rows; a fresh compile of its file has
/// none. The two still declare the same nodes.
#[test]
fn a_page_with_rows_has_the_shape_of_its_file() {
    let listed = Listed::new();
    let fresh: Page = listed.kit.recompile("page.gogo").unwrap();
    assert!(listed.page.nodes.len() > fresh.nodes.len(), "the running page holds rows the file does not declare");
    assert_eq!(listed.page.file_node_count(), fresh.nodes.len());
    same_shape(&listed.page, &fresh).unwrap();
}

#[test]
fn the_rows_of_a_page_carry_over_a_reload() {
    let mut listed = Listed::new();
    let (task_rows, slider_rows) = (listed.rt.items(&listed.page, listed.tasks), listed.rt.items(&listed.page, listed.sliders));
    let nodes_before: usize = listed.page.nodes.len();

    listed.reload("Label title text=$item.title", "Label title text=$item.title, font_size=30").unwrap();

    assert_eq!(listed.page.nodes.len(), nodes_before, "every row is there again");
    assert_eq!(listed.rt.items(&listed.page, listed.tasks), task_rows, "each task row is the node it was");
    assert_eq!(listed.rt.items(&listed.page, listed.sliders), slider_rows, "each slider row is the node it was");
    assert_eq!(listed.titles(), ["one", "two", "three"], "the rows hold their items");
    let look = resolve(&listed.page, &listed.rt);
    for row in &task_rows {
        let label: NodeId = listed.page.children(*row).next().unwrap();
        assert_eq!(look.prop(&listed.page, label, PropKey::FontSize), Some(Value::VNum(30.0)), "the edit shows on a row");
    }
    let caption: NodeId = listed.page.children(slider_rows[1]).next().unwrap();
    assert_eq!(listed.rt.text_of(&listed.page, caption), Some("from code"), "what the code wrote on a row is still on it");
    assert_eq!(listed.rt.item_key(&listed.page, task_rows[1]), Some(&ItemKey::Int(2)));
    assert!(listed.rt.has(task_rows[1], SELECTED) && !listed.rt.has(task_rows[0], SELECTED), "the selected row stays selected");
    assert_eq!(listed.rt.scroll_of(listed.picker), [0.0, 12.0], "the List stays where it was scrolled to");

    // The rows are the code's to fill as before: a changed and a moved
    // item keep their rows, and the selection follows its key.
    listed.rt.set_items(&mut listed.page, listed.tasks, &[task(2, "two!", "second"), task(1, "one", "first")]).unwrap();
    assert_eq!(listed.rt.items(&listed.page, listed.tasks), [task_rows[1], task_rows[0]]);
    assert_eq!(listed.titles(), ["two!", "one"]);
    assert!(listed.rt.has(task_rows[1], SELECTED));
}

#[test]
fn a_changed_binding_fills_the_rows_again_and_a_deleted_one_is_gone() {
    let mut listed = Listed::new();
    listed.reload("text=$item.title", "text=$item.note").unwrap();
    assert_eq!(listed.titles(), ["first", "second", "third"], "the rows on screen are filled through the new binding");
    listed.reload("text=$item.note", "text=\"fixed\"/_").unwrap();
    assert_eq!(listed.titles(), ["fixed", "fixed", "fixed"], "what the item wrote before does not stay behind");
}

#[test]
fn an_edit_the_rows_items_cannot_fill_is_refused_and_nothing_changes() {
    let mut listed = Listed::new();
    let (rows, nodes) = (listed.rt.items(&listed.page, listed.tasks), listed.page.nodes.len());

    let Err(Why::Items(why)) = listed.reload("text=$item.title", "text=$item.missing") else { panic!("a field no item has") };
    assert!(why.contains("G9101") && why.contains("tasks") && why.contains("no field `missing`"), "{why}");

    let Err(Why::Items(why)) = listed.reload("text=$item.missing", "text=$item.title, font_size=$item.title") else { panic!("text where a number goes") };
    assert!(why.contains("field `title`") && why.contains("font_size"), "{why}");

    assert_eq!(listed.page.nodes.len(), nodes, "the page on screen is the one before");
    assert_eq!(listed.rt.items(&listed.page, listed.tasks), rows);
    assert_eq!(listed.titles(), ["one", "two", "three"]);
    assert!(listed.rt.has(rows[1], SELECTED));
}

#[test]
fn a_repeat_keyed_another_way_is_refused() {
    let mut listed = Listed::new();
    let refused = listed.reload("repeat tasks by $item.id", "repeat tasks by $item.title");
    assert_eq!(refused, Err(Why::Shape(ShapeChange::RowKey { repeat: "tasks".to_string() })));
    let message: String = ShapeChange::RowKey { repeat: "tasks".to_string() }.to_string();
    assert!(message.contains("`tasks`") && message.contains("needs a rebuild"), "{message}");
    assert_eq!(listed.titles(), ["one", "two", "three"], "nothing changed");
}

/// A node with no name moved from the repeat's `empty { }` into its row:
/// the file still declares the same kinds and names in the same order, so
/// only making the rows again shows that a row is another size now, and
/// that the row nodes the code holds would name other nodes.
#[test]
fn a_row_that_holds_other_nodes_is_refused() {
    const BEFORE: &str = "empty { Label text=\"none\"/_ }\n            Row item { Label a text=$item.title }";
    const AFTER: &str = "empty { }\n            Row item { Label a text=$item.title; Label text=\"none\"/_ }";
    let source: String = format!("View root {{\n    Column col {{\n        repeat rows by $item.id {{\n            {BEFORE}\n        }}\n    }}\n}}\n");
    let kit = build(&[("page.gogo", &source)], &[], &[]);
    let mut page: Page = kit.page("page.gogo");
    let mut rt = Runtime::new(&page);
    let rows: NodeId = page.find("rows").unwrap();
    rt.set_items(&mut page, rows, &[task(1, "one", ""), task(2, "two", "")]).unwrap();
    let shown: Vec<NodeId> = rt.items(&page, rows);

    kit.edit("page.gogo", BEFORE, AFTER);
    let fresh: Page = kit.recompile("page.gogo").unwrap();
    same_shape(&page, &fresh).expect("the file declares the same list");
    assert_eq!(swap(&mut page, &mut rt, fresh), Err(Why::Shape(ShapeChange::Row { repeat: "rows".to_string() })));
    assert_eq!(rt.items(&page, rows), shown, "nothing changed");
    let message: String = ShapeChange::Row { repeat: "rows".to_string() }.to_string();
    assert!(message.contains("`rows`") && message.contains("needs a rebuild"), "{message}");
}

// ----- picture sheets ------------------------------------------------------

/// A 64x32 picture cut into eight 16x16 cells; the page shows two of them.
const SHEET: &str = r#"atlas sheet source="res://s/sheet.png", cell=16x16
View root {
    Column col {
        Image pic source=sheet[1], width=16, height=16
        Image other source=sheet[0], width=16, height=16
    }
}
"#;

fn sheet() -> (Kit, Vec<ImageKey>) {
    let kit = build(&[("page.gogo", SHEET)], &[], &[("res://s/sheet.png", png(64, 32))]);
    let BuiltShape::Grid(cells) = kit.tables.atlases[0].shape else { panic!("a grid atlas") };
    assert_eq!(cells.len(), 8);
    (kit, cells.to_vec())
}

/// A reload reads no picture, yet the page it compiles names the build's
/// own slices: a cell the file names, and a cell code set, both show the
/// picture piece they showed before.
#[test]
fn a_page_with_a_picture_sheet_reloads_with_the_builds_slices() {
    let (kit, cells) = sheet();
    let mut page: Page = kit.page("page.gogo");
    let mut rt = Runtime::new(&page);
    let (pic, other) = (page.find("pic").unwrap(), page.find("other").unwrap());
    rt.set_prop(pic, PropKey::Source, Value::VImage(cells[5]));
    let rect_before: [u16; 4] = page.atlas_slice(cells[5]).unwrap().rect;
    assert_eq!(rect_before, [16, 16, 16, 16], "cell 5 of a grid four wide");

    kit.edit("page.gogo", "source=sheet[0]", "source=sheet[2]");
    let fresh: Page = kit.recompile("page.gogo").unwrap();
    swap(&mut page, &mut rt, fresh).unwrap();

    assert_eq!(page.prop(other, PropKey::Source), Some(Value::VImage(cells[2])), "the file's new cell, by the build's number");
    assert_eq!(resolve(&page, &rt).prop(&page, pic, PropKey::Source), Some(Value::VImage(cells[5])), "the cell code set");
    assert_eq!(page.atlas_slice(cells[5]).unwrap().rect, rect_before, "and it is the same piece of the picture");
    assert_eq!(page.slices.len(), 8, "the page holds the build's slice table");
}

#[test]
fn an_atlas_line_that_is_not_the_builds_is_refused_at_its_line() {
    let (kit, _) = sheet();
    // Spaces and a comment are not a change.
    kit.save("page.gogo", &SHEET.replace("atlas sheet source=\"res://s/sheet.png\", cell=16x16", "atlas sheet   source=\"res://s/sheet.png\",   cell=16x16   // the stickers"));
    kit.recompile("page.gogo").unwrap();

    for (what, text) in [
        ("another cell size", SHEET.replace("cell=16x16", "cell=32x32")),
        ("the words in another order", SHEET.replace("source=\"res://s/sheet.png\", cell=16x16", "cell=16x16, source=\"res://s/sheet.png\"")),
        ("a word more", SHEET.replace("cell=16x16", "cell=16x16, release=unused")),
    ] {
        kit.save("page.gogo", &text);
        let errs = kit.recompile("page.gogo").unwrap_err();
        assert_eq!(errs.len(), 1, "{what}: {errs:#?}");
        assert_eq!((errs[0].code.as_str(), errs[0].line), ("G2022", 1), "{what}: {}", errs[0]);
        assert!(errs[0].message.contains("`sheet`") && errs[0].message.contains("rebuild"), "{what}: {}", errs[0]);
    }

    kit.save("page.gogo", &format!("atlas extra source=\"res://s/sheet.png\", cell=32x32\n{SHEET}"));
    let errs = kit.recompile("page.gogo").unwrap_err();
    assert_eq!((errs[0].code.as_str(), errs[0].line), ("G2022", 1), "{}", errs[0]);
    assert!(errs[0].message.contains("`extra`") && errs[0].message.contains("not in this build"), "{}", errs[0]);
}

// ----- what a build writes down ---------------------------------------------

/// `build_pages` writes `BUILD`: every page with its file, the files it
/// `uses`, and the stamp of what they said — the number a reload computes
/// again from the files.
#[test]
fn a_build_writes_down_what_a_reload_needs() {
    use gogo_ui::reload::TextStamp;
    let dir = std::env::temp_dir().join(format!("gogo_build_tables_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let ui = dir.join("ui");
    std::fs::create_dir_all(ui.join("styles")).unwrap();
    let page_text = "uses \"styles/theme.gogo\"\nView root { Label a text=\"A\"/_, font_color=$accent }\n";
    let theme_text = "$accent = #FF0000FF\n";
    std::fs::write(ui.join("first-page.gogo"), page_text).unwrap();
    std::fs::write(ui.join("styles").join("theme.gogo"), theme_text).unwrap();
    let out = dir.join("pages.rs");
    gogo_ui::build_pages(&ui, &out, gogo_ui::BuildOptions { compile: gogo_ui::CompileOptions { code_controls: &["Canvas"], ..Default::default() }, ..gogo_ui::BuildOptions::new(&dir) }).unwrap();
    let source: String = std::fs::read_to_string(&out).unwrap();
    let _ = std::fs::remove_dir_all(&dir);

    let mut stamp = TextStamp::of(page_text);
    stamp.used("styles/theme.gogo", Some(theme_text));
    let row = format!("gogo_ui::reload::BuiltPage {{ file: \"first-page.gogo\", page: &first_page::PAGE, uses: &[\"styles/theme.gogo\"], text: {} }},", stamp.finish());
    assert!(source.contains("pub static BUILD: gogo_ui::reload::BuildTables<'static> = gogo_ui::reload::BuildTables {\n    keys: KEYS,\n    images: IMAGES,\n    slices: &SLICES,\n"), "{source}");
    assert!(source.contains(&row), "the page's row: {row}\n{source}");
    assert!(source.contains("gogo_ui::reload::BuiltPage { file: \"\", page: &gogo_edit_bar::PAGE, uses: &[], text: 0 },"), "the edit bar has no file");
    assert!(source.contains("    code_controls: &[\"Canvas\"],\n"), "{source}");
    assert!(source.contains("    require_keys: false,\n"), "{source}");

    // Line endings do not count: a checkout that turns them is no change.
    let mut crlf = TextStamp::of(&page_text.replace('\n', "\r\n"));
    crlf.used("styles/theme.gogo", Some(&theme_text.replace('\n', "\r\n")));
    let mut again = TextStamp::of(page_text);
    again.used("styles/theme.gogo", Some(theme_text));
    assert_eq!(crlf.finish(), again.finish());
    // A changed `uses` file, and one that is gone, are.
    let mut changed = TextStamp::of(page_text);
    changed.used("styles/theme.gogo", Some("$accent = #00FF00FF\n"));
    let mut gone = TextStamp::of(page_text);
    gone.used("styles/theme.gogo", None);
    let mut same = TextStamp::of(page_text);
    same.used("styles/theme.gogo", Some(theme_text));
    let same: u64 = same.finish();
    assert_ne!(changed.finish(), same);
    assert_ne!(gone.finish(), same);
}
