//! What a file wires by itself and the anim rules around it: the anim a
//! tap plays, the hooks `on:show` and `on:hide` take, the anim a
//! `transition … anim=` names, the order an `each` runs in, where a step
//! starts, and the words an anim line takes.

use gogo_ui::KeptFrame;
use gogo_ui::anim::Finish;
use gogo_ui::compile;
use gogo_ui::input::{Event, Input, Phase, Touch};
use gogo_ui::layout::{FixedAdvance, Rect};
use gogo_ui::page::{ArrangementId, BASE, Kind, NodeId, Page, PropKey, StrId};
use gogo_ui::runtime::{Resolved, Runtime, resolve, resolve_animated};

const VIEW: Rect = Rect { x: 0.0, y: 0.0, w: 400.0, h: 800.0 };

fn codes(src: &str) -> Vec<String> {
    compile(src, &|_| None).diags.iter().map(|d| d.code.clone()).collect()
}

fn messages(src: &str) -> String {
    compile(src, &|_| None).diags.iter().map(|d| format!("{}: {}\n", d.code, d.message)).collect()
}

fn builds(src: &str) -> bool {
    !gogo_ui::has_errors(&compile(src, &|_| None).diags)
}

/// A compiled page with its runtime and input, laid out in a 400x800
/// view for every touch and every read of a rect.
struct Fx {
    page: Page,
    rt: Runtime,
    input: Input,
}

impl Fx {
    fn new(src: &str) -> Fx {
        let c = compile(src, &|_| None);
        assert!(!gogo_ui::has_errors(&c.diags), "{:#?}", c.diags);
        let page = c.page;
        let rt = Runtime::new(&page);
        Fx { page, rt, input: Input::default() }
    }

    fn node(&self, name: &str) -> NodeId {
        self.page.find(name).unwrap_or_else(|| panic!("no node named {name}"))
    }

    fn anim(&self, name: &str) -> StrId {
        self.page.intern_lookup(name)
    }

    fn touch(&mut self, phase: Phase, x: f32, y: f32, t: f64) -> Vec<Event> {
        let kept = KeptFrame::fresh(&self.page, resolve(&self.page, &self.rt), VIEW, &FixedAdvance);
        self.input.touch(&self.page, &mut self.rt, kept.frame(), &FixedAdvance, Touch { phase, x, y, t, id: 1 })
    }

    /// The look after this frame's state changes have met their rules.
    fn look(&mut self) -> Resolved {
        resolve_animated(&self.page, &mut self.rt)
    }

    fn rect(&mut self, node: NodeId) -> Rect {
        let look = self.look();
        KeptFrame::fresh(&self.page, look, VIEW, &FixedAdvance).rects()[node.index()]
    }

    fn center(&mut self, node: NodeId) -> (f32, f32) {
        let r = self.rect(node);
        (r.x + r.w / 2.0, r.y + r.h / 2.0)
    }

    fn tick(&mut self, dt: f32) {
        self.rt.tick(&mut self.page, dt);
    }

    fn opacity(&mut self, node: NodeId) -> f32 {
        let look = self.look();
        look.num(&self.page, node, PropKey::Opacity).unwrap_or(1.0)
    }
}

fn near(got: f32, want: f32) -> bool {
    (got - want).abs() < 0.001
}

/// `on:activate=play(pulse)`: the tap itself starts the anim, with no
/// handler in the host.
#[test]
fn a_tap_plays_the_anim_its_button_names() {
    let mut fx = Fx::new("anim pulse { b.opacity to=0.5, duration=1 }\nView v { Button b text=\"Go\", on:activate=play(pulse), width=100, height=50 }");
    let pulse = fx.anim("pulse");
    let b = fx.node("b");
    let (x, y) = fx.center(b);
    fx.touch(Phase::Down, x, y, 0.0);
    assert!(!fx.rt.is_playing_id(pulse), "a press is not a tap yet");
    let events = fx.touch(Phase::Up, x, y, 0.05);
    assert!(fx.rt.is_playing_id(pulse), "the tap plays the anim");
    assert_eq!(events.len(), 1, "the host still hears the tap: {events:?}");
}

/// The same for the other events a button wires: `on:press` and
/// `on:release`.
#[test]
fn press_and_release_play_their_anims() {
    let mut fx = Fx::new("anim down { b.scale to=0.9, duration=1 }\nanim up { b.opacity to=0.5, duration=1 }\nView v { Button b text=\"Go\", on:press=play(down), on:release=play(up), width=100, height=50 }");
    let (down, up) = (fx.anim("down"), fx.anim("up"));
    let b = fx.node("b");
    let (x, y) = fx.center(b);
    fx.touch(Phase::Down, x, y, 0.0);
    assert!(fx.rt.is_playing_id(down), "the press plays its anim");
    assert!(!fx.rt.is_playing_id(up));
    fx.touch(Phase::Up, x, y, 0.05);
    assert!(fx.rt.is_playing_id(up), "the release plays its anim");
}

/// `on:show` and `on:hide` play an anim and do nothing else: every other
/// action and a handler name are build errors, because nothing would
/// ever carry them out.
#[test]
fn show_and_hide_hooks_take_play_only() {
    for hook in ["on:show=go(edit)", "on:show=toggle(edit)", "on:show=emit(Shown)", "on:show=Shown", "on:hide=go(edit)", "on:hide=toggle(edit)", "on:hide=emit(Hidden)", "on:hide=Hidden"] {
        let src = format!("arrangement edit\nView v {{ Panel p {hook}, width=10, height=10 }}");
        assert!(codes(&src).contains(&"G0084".to_string()), "{hook}: {}", messages(&src));
    }
    assert!(builds("anim a { p.opacity to=1, duration=1 }\nView v { Panel p on:show=play(a), on:hide=play(a), width=10, height=10 }"));
}

/// An anim line on a size moves the box: the rect follows the lane, as it
/// does under `transition card.width`.
#[test]
fn an_anim_line_on_a_size_moves_the_box() {
    let mut fx = Fx::new("anim grow { card.width to=200, duration=1, easing=linear\n and card.height to=150, duration=1, easing=linear }\nView v { Panel card width=100, height=50 }");
    let card = fx.node("card");
    let grow = fx.anim("grow");
    assert_eq!((fx.rect(card).w, fx.rect(card).h), (100.0, 50.0));
    fx.rt.play_id(&fx.page, grow, Finish::Nothing);
    fx.tick(0.5);
    assert_eq!((fx.rect(card).w, fx.rect(card).h), (150.0, 100.0), "half way");
    fx.tick(0.6);
    assert_eq!((fx.rect(card).w, fx.rect(card).h), (200.0, 150.0), "the end pose holds");
}

/// The same for a margin and a padding: the box moves, and what the
/// padding holds moves with it.
#[test]
fn an_anim_line_on_a_margin_or_a_padding_moves_the_box() {
    let mut fx = Fx::new("anim slide { card.margin_left to=50, duration=1, easing=linear\n and card.padding_left to=20, duration=1, easing=linear }\nView v { Row card width=100, height=50, margin_left=10, margin_top=10, padding_left=0 { Panel inner width=10, height=10 } }");
    let (card, inner) = (fx.node("card"), fx.node("inner"));
    let slide = fx.anim("slide");
    assert_eq!((fx.rect(card).x, fx.rect(inner).x), (10.0, 10.0));
    fx.rt.play_id(&fx.page, slide, Finish::Nothing);
    fx.tick(0.5);
    assert_eq!((fx.rect(card).x, fx.rect(inner).x), (30.0, 40.0), "half way: margin 30, padding 10");
    fx.tick(0.6);
    assert_eq!((fx.rect(card).x, fx.rect(inner).x), (50.0, 70.0), "the end pose holds");
}

/// A rule inside a widget names the widget's own anim: pressing an
/// instance plays that instance's copy.
#[test]
fn a_rule_inside_a_widget_plays_the_widgets_own_anim() {
    let mut fx = Fx::new("widget FxButton {\n anim pressFx { icon.scale to=1.25, duration=0.3 }\n style Root background=#202020FF {\n  transition any -> held anim=pressFx\n }\n Button .Root width=100, height=50 { Panel icon width=10, height=10 }\n}\nView v { FxButton paintButton }");
    let b = fx.node("paintButton");
    let (x, y) = fx.center(b);
    fx.touch(Phase::Down, x, y, 0.0);
    fx.look();
    let anim = fx.anim("paintButton/pressFx");
    assert!(fx.rt.is_playing_id(anim), "the press plays the instance's anim");
}

/// Two buttons with no name wear one style whose rule plays a `self`
/// anim: pressing one moves that one only.
#[test]
fn unnamed_wearers_of_a_style_move_one_at_a_time() {
    let mut fx = Fx::new("anim pressBounce { self.scale to=1.2, duration=0.3, easing=linear }\nstyle Bouncy corner_radius=4 {\n transition any -> held anim=pressBounce\n}\nView v { Column { Button .Bouncy text=\"A\", width=100, height=50\n Button .Bouncy text=\"B\", width=100, height=50 } }");
    let buttons: Vec<NodeId> = (0..fx.page.nodes.len()).map(NodeId::from_index).filter(|n| fx.page.nodes[*n].kind == Kind::Button).collect();
    assert_eq!(buttons.len(), 2);
    let (x, y) = fx.center(buttons[0]);
    fx.touch(Phase::Down, x, y, 0.0);
    fx.look();
    fx.tick(0.15);
    let look = fx.look();
    assert!(look.motion[buttons[0].index()].scale[0] > 1.05, "the pressed button moves: {:?}", look.motion[buttons[0].index()].scale);
    assert_eq!(look.motion[buttons[1].index()].scale, [1.0, 1.0], "the other one stays");
}

/// Going back with no rule of its own borrows the entering rule's timing
/// and never its anim: the property ends at its base value.
#[test]
fn going_home_does_not_play_the_entering_anim_again() {
    let mut fx = Fx::new("arrangement edit\nanim grow { card.scale to=1.5, duration=0.2, easing=linear }\ntransition base -> edit anim=grow\nView v { Panel card width=100, height=50 }");
    let card = fx.node("card");
    fx.look();
    fx.rt.go_id(ArrangementId::from_index(0));
    fx.look();
    fx.tick(0.3);
    assert_eq!(fx.look().motion[card.index()].scale, [1.5, 1.5], "entered: the anim's end stands");
    fx.rt.go_id(BASE);
    fx.look();
    assert!(!fx.rt.is_playing_id(fx.anim("grow")), "the way back plays nothing");
    fx.tick(1.0);
    assert_eq!(fx.look().motion[card.index()].scale, [1.0, 1.0], "home: the scale is at its base value");
}

/// A presence slot leaves the way every other node does: its `on:hide`
/// anim plays, then it is gone.
#[test]
fn a_presence_slot_plays_its_hide_before_it_goes() {
    let mut fx = Fx::new("control Marker $value=0, $min=0, $max=1 {\n state marked\n anim out { slot.mark.opacity to=0, duration=0.5, easing=linear }\n Layers {\n  slot track height=10 {\n   slot mark automatic=presence by marked, on:hide=play(out) { Circle 6x6, color=#FFFFFFFF }\n  }\n }\n}\nMarker m 100x10\n");
    let (m, mark) = (fx.node("m"), fx.node("m/mark"));
    let out = fx.anim("m/out");
    fx.look();
    fx.rt.set_custom(&fx.page, m, "marked", true).unwrap();
    assert!(!fx.look().gone(&fx.page, mark), "the mark shows while the state holds");
    fx.rt.set_custom(&fx.page, m, "marked", false).unwrap();
    let look = fx.look();
    assert!(fx.rt.is_playing_id(out), "the hide anim starts");
    assert!(!look.gone(&fx.page, mark), "the mark is still there while its hide anim plays");
    fx.tick(0.25);
    assert!(!fx.look().gone(&fx.page, mark), "half way through the hide");
    fx.tick(0.3);
    assert!(fx.look().gone(&fx.page, mark), "gone once the anim ends");
}

/// `each .Style` runs its wearers in the order the file writes them, a
/// node before the nodes inside it.
#[test]
fn each_style_runs_in_file_order() {
    let c = compile("anim pop { each .Item stagger_by=0.1 { opacity from=0, to=1, duration=0.2 } }\nstyle Item corner_radius=2\nView v { Column {\n Panel a .Item {\n  Panel a1 .Item\n  Panel a2 .Item { Panel a21 .Item }\n }\n Panel b .Item { Panel b1 .Item }\n Panel c .Item\n} }", &|_| None);
    assert!(!gogo_ui::has_errors(&c.diags), "{:#?}", c.diags);
    let page = c.page;
    let pop = page.intern_lookup("pop");
    let mut order: Vec<(f32, String)> = page.anims.iter().filter(|t| t.anim == pop).map(|t| (t.start, page.str(page.nodes[t.node].name).to_string())).collect();
    order.sort_by(|a, b| a.0.total_cmp(&b.0));
    let names: Vec<&str> = order.iter().map(|(_, n)| n.as_str()).collect();
    assert_eq!(names, ["a", "a1", "a2", "a21", "b", "b1", "c"]);
}

/// A step that writes `from=` starts there, also when an earlier step
/// already moved the property.
#[test]
fn a_later_step_starts_at_its_own_from() {
    let mut fx = Fx::new("anim blink { x.opacity to=0.5, duration=1, easing=linear\n then x.opacity from=0, to=1, duration=1, easing=linear }\nView v { Panel x width=10, height=10 }");
    let x = fx.node("x");
    let blink = fx.anim("blink");
    fx.rt.play_id(&fx.page, blink, Finish::Nothing);
    fx.tick(0.5);
    assert!(near(fx.opacity(x), 0.75), "the first step: {}", fx.opacity(x));
    fx.tick(0.75);
    assert!(near(fx.opacity(x), 0.25), "a quarter into the second step, counted from its `from=0`: {}", fx.opacity(x));
}

/// The same on a property lane (`corner_radius`).
#[test]
fn a_later_step_on_a_property_starts_at_its_own_from() {
    let mut fx = Fx::new("anim round { x.corner_radius to=10, duration=1, easing=linear\n then x.corner_radius from=0, to=20, duration=1, easing=linear }\nView v { Panel x width=40, height=40, corner_radius=4 }");
    let x = fx.node("x");
    let round = fx.anim("round");
    fx.rt.play_id(&fx.page, round, Finish::Nothing);
    fx.tick(1.25);
    let got = fx.look().num(&fx.page, x, PropKey::CornerRadius).unwrap();
    assert!(near(got, 5.0), "a quarter into the second step, counted from its `from=0`: {got}");
}

/// Two steps may both write `from=` on one property when one runs after
/// the other (a blink); two that run at the same time are refused.
#[test]
fn two_steps_with_a_from_are_refused_only_when_they_overlap() {
    let page = "View v { Panel x width=10, height=10 }";
    assert!(builds(&format!("anim blink {{ x.opacity from=1, to=0, duration=1\n then x.opacity from=1, to=0, duration=1 }}\n{page}")), "one after the other");
    let clash = format!("anim clash {{ x.opacity from=1, to=0, duration=1\n and x.opacity from=0, to=1, duration=1 }}\n{page}");
    assert!(codes(&clash).contains(&"G0064".to_string()), "{}", messages(&clash));
}

/// Every word on an anim line is known or refused.
#[test]
fn an_unknown_word_on_an_anim_line_is_refused() {
    let page = "View v { Panel x width=10, height=10 }";
    let cases = [
        ("a property line", "anim a { x.opacity to=1, duration=0.3, wobble=2 }", "wobble"),
        ("a key line", "anim a { x.opacity { key t=0, v=0, wobble=1\n key t=1, v=1 } }", "wobble"),
        ("a group", "anim a { group wobble=1 { x.opacity to=1, duration=0.3 } }", "wobble"),
        ("an each", "anim a { each x wobble=1 { opacity to=1, duration=0.3 } }", "wobble"),
        ("a play step", "anim b { x.opacity to=1, duration=0.3 }\nanim a { play b wobble=1 }", "wobble"),
        ("a pause", "anim a { x.opacity to=1, duration=0.3\n then delay=0.2, wobble=1\n then x.opacity to=0, duration=0.3 }", "wobble"),
        ("`repeat=` on a step", "anim a { x.opacity to=1, duration=0.3, repeat=forever }", "header"),
        ("`after=` on an each", "anim a { group g { x.opacity to=1, duration=0.3 }\n each x after=g { opacity to=0, duration=0.3 } }", "after="),
        ("`after=` on a play step", "anim b { x.opacity to=1, duration=0.3 }\nanim a { group g { x.scale to=2, duration=0.3 }\n play b after=g }", "after="),
        ("`easing=` on a line with keys", "anim a { x.opacity easing=cubic_out { key t=0, v=0\n key t=1, v=1 } }", "easing"),
        ("`from=` on a line with keys", "anim a { x.opacity from=0 { key t=0.5, v=0\n key t=1, v=1 } }", "from"),
    ];
    for (what, anim, word) in cases {
        let src = format!("{anim}\n{page}");
        assert!(!builds(&src), "{what} builds: {src}");
        assert!(messages(&src).contains(word), "{what}: the message names `{word}`: {}", messages(&src));
    }
    // The words each line does take still build.
    let takes = format!("anim a repeat=forever, speed=2, duration_each=0.3, easing=cubic_out {{\n group g offset=0.1, autoreverse=true {{ x.opacity from=0, to=1, offset=0.1 }}\n and each x stagger_by=0.1, duration_each=0.2 {{ scale to=1.2 }}\n and x.rotate {{ key t=0, v=0\n key t=1, v=90, easing=back_out, jump=false }}\n}}\n{page}");
    assert!(builds(&takes), "{}", messages(&takes));
}

/// `offset=` shifts any step that starts something: a property line with
/// keys, an `each` and a `play` step, as it shifts a plain property line
/// and a group.
#[test]
fn offset_shifts_a_keyed_line_an_each_and_a_play_step() {
    let c = compile("anim b { x.scale to=2, duration=0.3 }\nanim a {\n x.opacity offset=0.2 { key t=0, v=0\n key t=1, v=1 }\n and each x offset=0.5 { rotate to=90, duration=0.3 }\n and play b offset=0.7\n}\nView v { Panel x width=10, height=10 }", &|_| None);
    assert!(!gogo_ui::has_errors(&c.diags), "{:#?}", c.diags);
    let page = c.page;
    let a = page.intern_lookup("a");
    let start_of = |channel: &str| page.anims.iter().filter(|t| t.anim == a && page.str(t.channel) == channel).map(|t| t.start).fold(f32::MAX, f32::min);
    assert_eq!(start_of("opacity"), 0.2, "the keyed line");
    assert_eq!(start_of("rotate"), 0.5, "the each");
    assert_eq!(start_of("scale"), 0.7, "the played anim");
}

/// A rule's `anim=` names a declared anim, on a node and at the top
/// level.
#[test]
fn a_rule_naming_no_anim_is_refused() {
    for src in ["View v { Button b text=\"x\" { transition any -> held anim=nosuch } }", "arrangement edit\ntransition base -> edit anim=nosuch\nView v { Panel p width=1, height=1 }"] {
        assert!(codes(src).contains(&"G4004".to_string()), "{src}: {}", messages(src));
    }
}

/// `play(pop)` from outside a widget cannot name the widget's own anim:
/// the name is unknown out there.
#[test]
fn play_cannot_name_an_anim_inside_a_widget() {
    let src = "widget W {\n anim pop { self.scale to=1.2, duration=0.2 }\n Panel width=10, height=10\n}\nView v { W w\n Button b text=\"x\", on:activate=play(pop) }";
    assert!(codes(src).contains(&"G4004".to_string()), "{}", messages(src));
}

/// A rule is one line: written as a block it timed nothing.
#[test]
fn a_transition_written_as_a_block_is_refused() {
    let src = "arrangement expanded\nView v { Panel p width=1, height=1 }\ntransition base -> expanded { duration=0.25 }";
    assert!(codes(src).contains(&"G0034".to_string()), "{}", messages(src));
}

/// A node's rule between two arrangements times that node; `anim=` on it
/// never played, so it is refused with the place that works.
#[test]
fn an_anim_on_a_nodes_arrangement_rule_is_refused() {
    let src = "arrangement option1\narrangement option2\nanim a { plate.opacity to=0.5, duration=0.1 }\nView v { Rectangle plate 60x120 { transition option2 -> option1 anim=a } }";
    assert!(codes(src).contains(&"G0034".to_string()), "{}", messages(src));
    assert!(messages(src).contains("top level"), "{}", messages(src));
    assert!(builds("arrangement option1\narrangement option2\nanim a { plate.opacity to=0.5, duration=0.1 }\ntransition option2 -> option1 anim=a\nView v { Rectangle plate 60x120 }"), "the same rule at the top level");
}

/// A screen word cannot end a rule: a screen flip lands at once.
#[test]
fn a_rule_cannot_end_on_a_screen_word() {
    for word in ["screen.landscape", "screen.landscapeSmall"] {
        let src = format!("View v {{ Panel p }}\ntransition any -> {word} duration=0.3\nwhen {word} {{ p.opacity = 0.5 }}");
        assert!(codes(&src).contains(&"G0034".to_string()), "{word}: {}", messages(&src));
    }
}

/// A `when` and a rule's anim that both write one look property are two
/// writers, as they are on a motion word.
#[test]
fn a_when_and_a_rules_anim_on_one_look_property_are_refused() {
    let src = "anim flash { b.background to=#FF0000FF, duration=0.2 }\nView v { Button b text=\"x\", background=#000000FF {\n when held background=#00FF00FF\n transition any -> held anim=flash\n} }";
    assert!(codes(src).contains(&"G4003".to_string()), "{}", messages(src));
}

/// An anim may be written below the line that names it, on a page and in
/// a widget.
#[test]
fn an_anim_written_below_its_use_is_found() {
    let page = "View v { Button b text=\"x\", on:activate=play(late) { transition any -> held anim=late } }\nanim late { b.opacity to=0.5, duration=1 }";
    assert!(builds(page), "{}", messages(page));
    let rule = "arrangement edit\ntransition base -> edit anim=late\nView v { Panel p width=10, height=10 }\nanim late { p.opacity to=0.5, duration=1 }";
    assert!(builds(rule), "{}", messages(rule));
    let mut fx = Fx::new("widget W {\n style Root background=#202020FF {\n  transition any -> held anim=fx\n }\n Button .Root width=100, height=50\n anim fx { self.scale to=1.2, duration=0.2 }\n}\nView v { W w }");
    let w = fx.node("w");
    let (x, y) = fx.center(w);
    fx.touch(Phase::Down, x, y, 0.0);
    fx.look();
    let anim = fx.anim("w/fx");
    assert!(fx.rt.is_playing_id(anim), "the press plays the instance's anim");
}

/// The same on the road a host draws by (`frame_retained`): the kept
/// layout sees the box change on every frame of the play, and a stop
/// that restores puts the written size back.
#[test]
fn a_size_under_an_anim_moves_on_the_road_a_host_draws_by() {
    let c = compile("anim grow { card.width to=200, duration=1, easing=linear }\nView v { Row { Panel card width=100, height=50\n Panel next width=20, height=50 } }", &|_| None);
    assert!(!gogo_ui::has_errors(&c.diags), "{:#?}", c.diags);
    let mut page = c.page;
    let (card, next) = (page.find("card").unwrap(), page.find("next").unwrap());
    let grow = page.intern_lookup("grow");
    let mut rt = Runtime::new(&page);
    let mut kept = KeptFrame::default();
    let mut draws = Vec::new();
    let mut frame = |rt: &mut Runtime, page: &Page| -> (f32, f32) {
        kept.frame_retained(rt, page, VIEW, &FixedAdvance, &mut draws, None);
        (kept.rects()[card.index()].w, kept.rects()[next.index()].x - kept.rects()[card.index()].x)
    };
    assert_eq!(frame(&mut rt, &page), (100.0, 100.0));
    rt.play_id(&page, grow, Finish::Nothing);
    rt.tick(&mut page, 0.5);
    assert_eq!(frame(&mut rt, &page), (150.0, 150.0), "half way: the box is wider and its neighbor moved over");
    rt.stop_id(grow, true);
    assert_eq!(frame(&mut rt, &page), (100.0, 100.0), "a stop that restores puts the written size back");
    rt.play_id(&page, grow, Finish::Nothing);
    rt.tick(&mut page, 1.1);
    assert_eq!(frame(&mut rt, &page), (200.0, 200.0), "the end pose holds");
}
