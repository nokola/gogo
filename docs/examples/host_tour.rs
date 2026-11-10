//! A small host for gogo pages, in pieces.
//!
//! Needed because the docs teach the host side of gogo with Rust
//! examples, and an example nobody compiles goes stale without a sound.
//! Every `rust` example in `docs/` is cut from this file, and
//! `crates/gogo-ui/tests/docs_examples.rs` compiles it, runs `tour`, and
//! checks each example against the text here.
//!
//! The pages here are compiled while the program runs (`compile`), which
//! needs no build step. A shipping app compiles its pages in `build.rs`
//! (`build_pages`) and gets its names as generated code; the docs show
//! that form beside these.

use std::sync::Arc;

use gogo_ui::layout::{FixedAdvance, Rect};
use gogo_ui::locale::Keys;
use gogo_ui::page::Action;
use gogo_ui::reload::{BuildTables, recompile, same_shape};
use gogo_ui::runtime::{Item, ItemKey, ItemValue};
use gogo_ui::{ArrangementId, Behavior, BehaviorSpec, Claim, Compiled, CompileOptions, Cx, Draw, Event, EventKind, Finish, Input, KEYS_ARROWS, KeptFrame, KeyAction, KeyId, Locale, NO_NODE, NO_STR, NodeId, Orientation, Page, Payload, Phase, PropKey, Runtime, ScreenClass, StrId, Touch, Value, compile, compile_with, has_errors};

/// Compiles a page from text and prints what the compiler says.
///
/// Returns the page with its translation keys, or `None` when the text has
/// an error.
pub fn compile_page(source: &str) -> Option<(Page, Keys)> {
    // The second argument reads the files that `uses "..."` lines name.
    // This page names none.
    let compiled: Compiled = compile(source, &|_| None);
    for d in &compiled.diags {
        eprintln!("{d}"); // error G2001 line 3: unknown property ...
    }
    if has_errors(&compiled.diags) {
        return None;
    }
    Some((compiled.page, compiled.keys))
}

/// One page on screen: the page, its state, and the last frame.
pub struct Hosted {
    pub page: Page,
    /// Everything that changes while the page lives: states, scroll
    /// positions, running animations, what code wrote.
    pub rt: Runtime,
    /// The touch and key machine: it turns raw touches into events.
    pub input: Input,
    /// Where every node sat in the last frame, and how it looked.
    pub kept: KeptFrame,
    /// The last frame's draw commands, in paint order.
    pub draws: Vec<Draw>,
    /// The screen in epx.
    pub screen: Rect,
}

impl Hosted {
    pub fn new(page: Page, width_epx: f32, height_epx: f32) -> Hosted {
        let rt: Runtime = Runtime::new(&page);
        let mut hosted = Hosted { page, rt, input: Input::new(), kept: KeptFrame::default(), draws: Vec::new(), screen: Rect::new(0.0, 0.0, width_epx, height_epx) };
        // Tell the page which `when screen.*` words hold.
        hosted.rt.set_screen(&hosted.page, Orientation::from_view(width_epx, height_epx), ScreenClass::from_view(width_epx, height_epx));
        // Play each visible node's `on:show` once: the page's entrance.
        hosted.rt.play_entrances(&hosted.page);
        hosted.frame(0.0);
        hosted
    }

    /// One frame, `dt` seconds after the last.
    ///
    /// Returns true while something still moves; a host may stop asking
    /// for frames when it returns false.
    pub fn frame(&mut self, dt: f32) -> bool {
        let mut moving: bool = self.rt.tick(&mut self.page, dt);
        if self.kept.is_laid_out() {
            moving |= self.input.tick(&self.page, &mut self.rt, self.kept.frame(), dt);
        }
        // Lays the page out and fills `draws`. `FixedAdvance` measures
        // text as if every letter had one width; a real host passes its
        // own `TextMeasure`.
        self.kept.frame_retained(&mut self.rt, &self.page, self.screen, &FixedAdvance, &mut self.draws, None);
        moving | self.rt.is_moving()
    }

    /// Hands one touch to the page. `t` is the time in seconds.
    pub fn touch(&mut self, phase: Phase, x: f32, y: f32, t: f64) -> Vec<Event> {
        self.input.touch(&self.page, &mut self.rt, self.kept.frame(), &FixedAdvance, Touch { phase, x, y, t, id: 1 })
    }

    /// A finger down and up at the middle of `node`.
    pub fn tap(&mut self, node: NodeId, t: f64) -> Vec<Event> {
        let r: Rect = self.kept.rects()[node.index()];
        let (x, y) = (r.x + r.w / 2.0, r.y + r.h / 2.0);
        let mut events: Vec<Event> = self.touch(Phase::Down, x, y, t);
        events.extend(self.touch(Phase::Up, x, y, t + 0.05));
        events
    }
}

/// The handler name an event carries.
///
/// An `on:<event>=Name` or `on:<event>=emit(Name)` line names it. With no
/// such line the name is the node's own (a widget's inner node answers
/// with the name of the instance around it). `go()` and `play()` actions
/// are done by gogo itself and name nothing.
pub fn handler_name<'a>(page: &'a Page, ev: &Event) -> Option<&'a str> {
    match ev.action {
        Some(Action::Named(id)) | Some(Action::Emit(id)) => Some(page.str(id)),
        Some(Action::Go(_)) | Some(Action::ToggleArrangement(_)) | Some(Action::Play(_)) => None,
        None => {
            let mut n: NodeId = ev.node;
            loop {
                let node = &page.nodes[n.index()];
                if node.name != NO_STR && !page.str(node.name).contains('/') {
                    return Some(page.str(node.name));
                }
                if node.parent == NO_NODE {
                    return None;
                }
                n = node.parent;
            }
        }
    }
}

const MENU: &str = r#"
$ink = #202020
theme dark {
    $ink = #FFFFFF
}
arrangement expanded
anim appear {
    menu.opacity from=0, to=1, duration=0.3
}
View menuPage {
    Column menu halign=center, valign=center, spacing=12 {
        Label title text="Paint"/appTitle, font_size=24, font_color=$ink
        Button startButton text="Start"/start, min_width=160, min_height=44
        Button quitButton text="Quit"/quit, min_width=160, min_height=44, on:activate=emit(QuitPressed)
        Slider sizeSlider min=1, max=300, value=80, width=160
    }
    when expanded {
        menu.spacing = 24
    }
}
"#;

/// The same keys a build writes as `KEYS`: (name, English), in id order.
static MENU_KEYS: &[(&str, &str)] = &[("appTitle", "Paint"), ("start", "Start"), ("quit", "Quit")];

fn events_tour(host: &mut Hosted) {
    let start: NodeId = host.page.find("startButton").expect("startButton");
    let quit: NodeId = host.page.find("quitButton").expect("quitButton");

    // Which node would a touch at this point land on? `false`: only
    // nodes that take input count.
    let r: Rect = host.kept.rects()[start.index()];
    assert_eq!(gogo_ui::hit(&host.page, host.kept.frame(), r.x + 5.0, r.y + 5.0, false), Some(start));

    for ev in host.tap(start, 1.0) {
        if ev.kind == EventKind::Activate {
            // No `on:activate=` on the line: the handler is the node's name.
            assert_eq!(handler_name(&host.page, &ev), Some("startButton"));
        }
    }
    for ev in host.tap(quit, 2.0) {
        if ev.kind == EventKind::Activate {
            assert_eq!(handler_name(&host.page, &ev), Some("QuitPressed"));
        }
    }

    // A drag on the Slider: `Edit` while the finger moves, one `Change`
    // when it lifts. Both carry the value.
    let slider: NodeId = host.page.find("sizeSlider").expect("sizeSlider");
    let r: Rect = host.kept.rects()[slider.index()];
    let y: f32 = r.y + r.h / 2.0;
    let mut events: Vec<Event> = host.touch(Phase::Down, r.x + 10.0, y, 3.0);
    events.extend(host.touch(Phase::Move, r.x + 120.0, y, 3.1));
    events.extend(host.touch(Phase::Up, r.x + 120.0, y, 3.2));
    let mut committed: Option<f32> = None;
    for ev in &events {
        match ev.kind {
            EventKind::Edit(Value::VNum(_live)) => {} // update a preview
            EventKind::Change(Value::VNum(v)) => committed = Some(v),
            _ => {}
        }
    }
    assert!(committed.is_some(), "a drag commits one value: {events:?}");

    // Code writes a value the same way a finger would have left it.
    host.rt.set_prop(slider, PropKey::Value, Value::VNum(42.0));
    host.frame(0.0);
    assert_eq!(host.kept.look().slider_value(&host.page, slider), 42.0);
}

fn anim_tour(host: &mut Hosted) {
    // An anim's id on a page compiled while the program runs.
    let appear: StrId = host.page.intern_lookup("appear");
    host.rt.play_id(&host.page, appear, Finish::Nothing); // restarts if already running
    assert!(host.rt.is_playing_id(appear));
    host.frame(0.5); // the anim lasts 0.3 s
    assert!(!host.rt.is_playing_id(appear));
    assert!(host.rt.take_finished().contains(&appear)); // the finished anims, once each

    host.rt.play_id(&host.page, appear, Finish::Nothing);
    host.rt.stop_id(appear, true); // true: every property goes back to what the file says
    assert!(!host.rt.is_playing_id(appear));
}

fn arrangement_tour(host: &mut Hosted) {
    let menu: NodeId = host.page.find("menu").expect("menu");
    let expanded = ArrangementId::from_index(host.page.arrangements.iter().position(|a| host.page.str(*a) == "expanded").expect("arrangement expanded"));
    host.rt.go_id(expanded);
    host.frame(1.0);
    assert_eq!(host.kept.look().num(&host.page, menu, PropKey::Spacing), Some(24.0));
    host.rt.go_id(gogo_ui::BASE); // back to what the page declares
    host.frame(1.0);
    assert_eq!(host.kept.look().num(&host.page, menu, PropKey::Spacing), Some(12.0));
}

fn writes_tour(host: &mut Hosted) {
    let title: NodeId = host.page.find("title").expect("title");
    // A text code writes is shown as it is, never translated.
    host.rt.set_text(&mut host.page, title, "Paint 2");
    assert_eq!(host.rt.text_of(&host.page, title), Some("Paint 2"));
    // Any property the file could write, code can write. A code write
    // wins over the file until `clear_prop` takes it back.
    host.rt.set_prop(title, PropKey::Width, Value::VNum(200.0));
    host.frame(0.0);
    assert_eq!(host.kept.rects()[title.index()].w, 200.0);
    host.rt.clear_prop(title, PropKey::Width);
    // Hidden nodes take no space and no touches.
    host.rt.set_visible(&mut host.page, title, false);
    host.frame(0.0);
    assert!(host.kept.look().gone(&host.page, title));
    host.rt.set_visible(&mut host.page, title, true);
    host.rt.set_text(&mut host.page, title, "Paint");
    host.frame(0.0);
}

fn theme_tour(host: &mut Hosted) {
    let title: NodeId = host.page.find("title").expect("title");
    let light = host.kept.look().font_color(&host.page, title);
    host.rt.set_theme_variant(&host.page, "dark"); // "" = back to the defaults
    host.frame(0.0);
    assert_ne!(host.kept.look().font_color(&host.page, title), light);
}

fn locale_tour(host: &mut Hosted, keys: &Keys) {
    // One language: (key, translated text). A key the list leaves out
    // shows its English.
    let title_key: KeyId = keys.get("appTitle").expect("the page names /appTitle");
    let german: Locale = Locale::build(MENU_KEYS, &[(title_key, "Malen")]);
    host.rt.set_locale(Some(Arc::new(german)));
    host.frame(0.0);
    assert_eq!(host.rt.translate("appTitle", "Paint"), "Malen");
    assert_eq!(host.rt.translate("start", "Start"), "Start");
    host.rt.set_locale(None); // the literals as the file wrote them
}

const TASKS: &str = r#"
View tasksPage {
    List picker width=200, height=300, halign=left, valign=top {
        empty { Label text="Nothing to do" }
        repeat tasks as $task by $task.id {
            ListItem row min_height=40 {
                Label title text=$task.title
            }
        }
    }
}
"#;

fn task(id: i64, title: &str) -> Item {
    Item::Record(vec![("id".to_string(), ItemValue::Int(id)), ("title".to_string(), ItemValue::Str(title.to_string()))])
}

fn list_tour() {
    let (page, _) = compile_page(TASKS).expect("the tasks page compiles");
    let mut host: Hosted = Hosted::new(page, 360.0, 640.0);
    let tasks: NodeId = host.page.find("tasks").expect("repeat tasks");
    let picker: NodeId = host.page.find("picker").expect("List picker");

    // The whole collection, every time it changes. Rows are matched by
    // key: a kept key keeps its row.
    host.rt.set_items(&mut host.page, tasks, &[task(1, "Buy milk"), task(2, "Call mom")]).expect("two rows with different keys");
    host.frame(0.0);
    assert_eq!(host.rt.items(&host.page, tasks).len(), 2);

    // Two rows with one key: the whole write is refused, the rows stay.
    let refused = host.rt.set_items(&mut host.page, tasks, &[task(3, "a"), task(3, "b")]);
    assert!(refused.is_err());
    assert_eq!(host.rt.items(&host.page, tasks).len(), 2);

    // A tap on a row: `Change` on the List when the selection changed,
    // then `Activate` on the row. The row's key says which item it was.
    let first_row: NodeId = host.rt.items(&host.page, tasks)[0];
    for ev in host.tap(first_row, 1.0) {
        match ev.kind {
            EventKind::Change(_) => assert_eq!(handler_name(&host.page, &ev), Some("picker")),
            EventKind::Activate => assert_eq!(host.rt.item_key(&host.page, ev.node), Some(&ItemKey::Int(1))),
            _ => {}
        }
    }
    assert_eq!(host.rt.selected_keys(&host.page, picker), vec![ItemKey::Int(1)]);

    // Code writes the selection too: a list of keys, empty to clear.
    host.rt.set_selected(&host.page, picker, &[ItemKey::Int(2)]).expect("key 2 is a row");
    assert_eq!(host.rt.selected_keys(&host.page, picker), vec![ItemKey::Int(2)]);
}

fn reload_tour(host: &mut Hosted) {
    // What the running program was built with. A build passes its
    // generated `KEYS` and the names of its `IMAGES`.
    let baked = BuildTables { keys: MENU_KEYS, images: &[], code_controls: &[], require_keys: false };
    let edited: String = MENU.replace("spacing=12", "spacing=20");
    match recompile(&edited, "menu.gogo", &|_| None, &baked) {
        Err(errors) => {
            for d in &errors {
                eprintln!("menu.gogo: {d}"); // the page on screen stays
            }
        }
        Ok(mut new_page) => match same_shape(&host.page, &new_page) {
            Err(change) => eprintln!("menu.gogo: {change}"), // a node came or went: rebuild
            Ok(()) => {
                host.rt = host.rt.carried_to(&host.page, &mut new_page);
                host.page = new_page;
                host.frame(0.0);
            }
        },
    }
    let menu: NodeId = host.page.find("menu").expect("menu");
    assert_eq!(host.kept.look().num(&host.page, menu, PropKey::Spacing), Some(20.0));
}

const SAFE: &str = r#"
View safePage {
    Layers content margin=safe {
        Label corner text="x", halign=left, valign=top
    }
}
"#;

fn safe_tour() {
    let (page, _) = compile_page(SAFE).expect("the safe page compiles");
    let mut host: Hosted = Hosted::new(page, 360.0, 640.0);
    let corner: NodeId = host.page.find("corner").expect("corner");
    // The screen's insets in epx: left, top, right, bottom.
    host.rt.safe = [0.0, 24.0, 0.0, 0.0];
    host.rt.dirty = true; // a plain field: say that the page must lay out again
    host.frame(0.0);
    assert_eq!(host.kept.rects()[corner.index()].y, 24.0);
}

/// A vertical fader: the value follows y over the track, bottom = min.
#[derive(Default)]
struct VFader;

static VFADER: BehaviorSpec = BehaviorSpec { name: "vfader", slots: &["track"], keys: KEYS_ARROWS, payload: Payload::Number };

impl VFader {
    fn apply(cx: &mut Cx, y: f32) {
        let track = cx.slot(0); // the spec's slot order
        cx.set_fraction((track.y + track.h - y) / track.h); // snaps to step=, clamps
    }
}

impl Behavior for VFader {
    fn spec(&self) -> &'static BehaviorSpec {
        &VFADER
    }
    fn began(&mut self, cx: &mut Cx, _x: f32, y: f32) -> Claim {
        Self::apply(cx, y);
        Claim::Take
    }
    fn moved(&mut self, cx: &mut Cx, _x: f32, y: f32) {
        Self::apply(cx, y);
    }
    fn key(&mut self, cx: &mut Cx, action: KeyAction) -> bool {
        match action {
            KeyAction::Increment => cx.set_value(cx.value() + cx.key_step()),
            KeyAction::Decrement => cx.set_value(cx.value() - cx.key_step()),
            _ => return false, // Home/End: not ours
        }
        true
    }
}

const VOLUME: &str = r#"
control Vol $value=5, $min=0, $max=10, $step=0 {
    behavior vfader by $value in $min..$max, key_unit=10%
    state_builtin focused, held
    keys home_end
    Layers width=40, height=200 { slot track width=fill, height=fill { Rectangle color=gray } }
}
View volumePage {
    Vol volume halign=left, valign=top
}
"#;

fn behavior_tour() {
    // The compiler checks the `.gogo` text against the spec (slots, keys).
    let options = CompileOptions { behaviors: &[VFADER], ..CompileOptions::default() };
    let compiled: Compiled = compile_with(VOLUME, &|_| None, options);
    assert!(!has_errors(&compiled.diags), "{:?}", compiled.diags);
    let mut host: Hosted = Hosted::new(compiled.page, 360.0, 640.0);
    // While the program runs, the page's Input finds the behavior by name.
    host.input.register(Box::new(VFader));

    let volume: NodeId = host.page.find("volume").expect("Vol volume");
    let mut events: Vec<Event> = host.touch(Phase::Down, 20.0, 150.0, 1.0);
    events.extend(host.touch(Phase::Up, 20.0, 150.0, 1.1));
    let change = events.iter().find_map(|ev| if let EventKind::Change(Value::VNum(v)) = ev.kind { Some((ev.node, v)) } else { None });
    // The track is 200 tall and y = 150 is a quarter of the way up: 2.5 of 0..10.
    assert_eq!(change, Some((volume, 2.5)), "{events:?}");
}

/// Runs every piece above. The docs test calls it.
pub fn tour() {
    let (page, keys) = compile_page(MENU).expect("the menu compiles");
    assert_eq!(keys.names, MENU_KEYS.iter().map(|(name, _)| name.to_string()).collect::<Vec<String>>(), "MENU_KEYS lists the page's keys in id order");
    let mut host: Hosted = Hosted::new(page, 360.0, 640.0);
    host.frame(1.0); // let the entrance finish
    events_tour(&mut host);
    anim_tour(&mut host);
    arrangement_tour(&mut host);
    writes_tour(&mut host);
    theme_tour(&mut host);
    locale_tour(&mut host, &keys);
    reload_tour(&mut host);
    list_tour();
    safe_tour();
    behavior_tour();
    assert!(compile_page("View v { Label a no_such_word=1 }").is_none());
}
