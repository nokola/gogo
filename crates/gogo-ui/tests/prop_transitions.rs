//! Property transitions (`transition commandBar.opacity duration=0.4`):
//! any change of a listed property's standing value — a code write, a
//! `when` response, a pose — slides there from what shows; the rules that
//! make it snap instead (the first value, a hidden node, `set_prop_instant`,
//! `go_instant`); and the load checks (gogo-spec.md "Transitions").
use gogo::page::{ArrangementId, NodeId, PropKey, Value};
use gogo::runtime::{Resolved, Runtime, resolve_animated};
use gogo::{Compiled, Page, compile};

fn ok(src: &str) -> Compiled {
    let c = compile(src, &|_| None);
    assert!(!gogo::has_errors(&c.diags), "{:#?}", c.diags);
    c
}

/// The diagnostics' codes and messages, for the load-check tests.
fn errors(src: &str) -> Vec<(String, u32, String)> {
    compile(src, &|_| None).diags.into_iter().map(|d| (d.code, d.line, d.message)).collect()
}

fn num(page: &Page, r: &Resolved, node: NodeId, key: PropKey) -> f32 {
    match r.prop(page, node, key) {
        Some(Value::VNum(v)) => v,
        other => panic!("{key:?} on {node:?}: {other:?}"),
    }
}

fn margin_left(r: &Resolved, node: NodeId) -> f32 {
    r.placement[node.index()].margin[0].unwrap_or(0.0)
}

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-3
}

const BAR: &str = "transition bar.opacity duration=1, easing=linear\nView v { Panel bar width=100, height=20, opacity=0 }";

/// A code write slides from the shown value; a write mid-slide turns
/// around from where the slide is, and a slide keeps the frame loop awake
/// until it lands.
#[test]
fn a_code_write_slides_and_a_write_mid_slide_turns_around() {
    let c = ok(BAR);
    let mut page = c.page;
    let bar = page.find("bar").unwrap();
    let mut rt = Runtime::new(&page);
    let r = resolve_animated(&page, &mut rt);
    assert!(near(num(&page, &r, bar, PropKey::Opacity), 0.0));
    rt.set_prop(bar, PropKey::Opacity, Value::VNum(1.0));
    let r = resolve_animated(&page, &mut rt);
    assert!(near(num(&page, &r, bar, PropKey::Opacity), 0.0), "starts where it shows");
    assert!(rt.is_moving(), "a slide is motion for the idle gate");
    rt.tick(&mut page, 0.5);
    let r = resolve_animated(&page, &mut rt);
    assert!(near(num(&page, &r, bar, PropKey::Opacity), 0.5));
    rt.set_prop(bar, PropKey::Opacity, Value::VNum(0.0));
    let r = resolve_animated(&page, &mut rt);
    assert!(near(num(&page, &r, bar, PropKey::Opacity), 0.5), "turns around from 0.5, no jump");
    rt.tick(&mut page, 0.5);
    let r = resolve_animated(&page, &mut rt);
    assert!(near(num(&page, &r, bar, PropKey::Opacity), 0.25));
    rt.tick(&mut page, 0.6);
    let r = resolve_animated(&page, &mut rt);
    assert!(near(num(&page, &r, bar, PropKey::Opacity), 0.0));
    assert!(!rt.is_moving(), "landed: nothing holds the frame loop");
}

/// `node.{margin, opacity}`: one rule per property, `margin` = the four
/// edges; a margin write slides the node's seat.
#[test]
fn the_group_sugar_names_each_property_and_margin_is_four_edges() {
    let c = ok("transition bar.{margin, opacity} duration=1, easing=linear\nView v { Panel bar width=100, height=20, halign=left, valign=top }");
    let mut page = c.page;
    let bar = page.find("bar").unwrap();
    let keys: Vec<PropKey> = page.prop_transitions.iter().map(|t| t.key).collect();
    assert_eq!(keys, vec![PropKey::MarginLeft, PropKey::MarginTop, PropKey::MarginRight, PropKey::MarginBottom, PropKey::Opacity]);
    let mut rt = Runtime::new(&page);
    resolve_animated(&page, &mut rt);
    rt.set_prop(bar, PropKey::MarginLeft, Value::VNum(100.0));
    let r = resolve_animated(&page, &mut rt);
    assert!(near(margin_left(&r, bar), 0.0), "an unset margin slides from 0");
    rt.tick(&mut page, 0.25);
    let r = resolve_animated(&page, &mut rt);
    assert!(near(margin_left(&r, bar), 25.0), "{}", margin_left(&r, bar));
}

/// Two targets on one line, and the brace group with spaces.
#[test]
fn a_rule_takes_several_targets() {
    let c = ok("transition a.opacity, b.{margin_left, margin_top} duration=0.4, easing=expo_in_out\nView v { Panel a width=10\n Panel b width=10 }");
    let a = c.page.find("a").unwrap();
    let b = c.page.find("b").unwrap();
    let got: Vec<(NodeId, PropKey)> = c.page.prop_transitions.iter().map(|t| (t.node, t.key)).collect();
    assert_eq!(got, vec![(a, PropKey::Opacity), (b, PropKey::MarginLeft), (b, PropKey::MarginTop)]);
    assert_eq!(c.page.str(c.page.prop_transitions[0].easing), "expo_in_out");
}

/// `set_prop_instant` lands at once; a `set_prop` right after it slides
/// from the instant value (a bar shown faded out, then faded in).
#[test]
fn set_prop_instant_snaps_and_a_following_write_slides_from_it() {
    let c = ok(BAR);
    let mut page = c.page;
    let bar = page.find("bar").unwrap();
    let mut rt = Runtime::new(&page);
    resolve_animated(&page, &mut rt);
    rt.set_prop_instant(bar, PropKey::Opacity, Value::VNum(0.8));
    let r = resolve_animated(&page, &mut rt);
    assert!(near(num(&page, &r, bar, PropKey::Opacity), 0.8), "no slide");
    assert!(!rt.is_moving());
    rt.set_prop_instant(bar, PropKey::Opacity, Value::VNum(0.2));
    rt.set_prop(bar, PropKey::Opacity, Value::VNum(1.0));
    let r = resolve_animated(&page, &mut rt);
    assert!(near(num(&page, &r, bar, PropKey::Opacity), 0.2), "the slide starts at the instant value, not at 0.8");
    rt.tick(&mut page, 0.5);
    let r = resolve_animated(&page, &mut rt);
    assert!(near(num(&page, &r, bar, PropKey::Opacity), 0.6));
    // An instant write mid-slide stops it where told.
    rt.set_prop_instant(bar, PropKey::Opacity, Value::VNum(1.0));
    let r = resolve_animated(&page, &mut rt);
    assert!(near(num(&page, &r, bar, PropKey::Opacity), 1.0));
    assert!(!rt.is_moving());
}

/// Snaps: the file's first value (a pose already on at build included)
/// and a change on a node that doesn't show.
#[test]
fn the_first_value_and_a_hidden_node_never_slide() {
    let c = ok("arrangement edit\nwhen edit { bar.opacity = 0.3 }\ntransition bar.opacity duration=1\nView v { Panel bar width=10, opacity=1 }");
    let mut page = c.page;
    let bar = page.find("bar").unwrap();
    let mut rt = Runtime::new(&page);
    rt.go_id(ArrangementId::new(0));
    let r = resolve_animated(&page, &mut rt);
    assert!(near(num(&page, &r, bar, PropKey::Opacity), 0.3), "the first look lands as is");
    rt.set_visible(&mut page, bar, false);
    resolve_animated(&page, &mut rt);
    rt.set_prop(bar, PropKey::Opacity, Value::VNum(0.9));
    let r = resolve_animated(&page, &mut rt);
    assert!(near(num(&page, &r, bar, PropKey::Opacity), 0.9), "hidden: nothing to watch");
    assert!(!rt.is_moving());
}

/// A `when` response slides by the property's rule; `go_instant` lands
/// its own responses at once.
#[test]
fn a_pose_change_slides_and_go_instant_snaps() {
    let c = ok("arrangement edit\nwhen edit { bar.opacity = 0 }\ntransition bar.opacity duration=1, easing=linear\nView v { Panel bar width=10 }");
    let mut page = c.page;
    let bar = page.find("bar").unwrap();
    let mut rt = Runtime::new(&page);
    resolve_animated(&page, &mut rt);
    rt.go_id(ArrangementId::new(0));
    let r = resolve_animated(&page, &mut rt);
    assert!(near(num(&page, &r, bar, PropKey::Opacity), 1.0), "starts at the base look");
    rt.tick(&mut page, 0.5);
    let r = resolve_animated(&page, &mut rt);
    assert!(near(num(&page, &r, bar, PropKey::Opacity), 0.5));
    rt.tick(&mut page, 0.6);
    resolve_animated(&page, &mut rt);
    rt.go_instant_id(gogo::BASE);
    let r = resolve_animated(&page, &mut rt);
    assert_eq!(r.prop(&page, bar, PropKey::Opacity), None, "go_instant times nothing: the base look (no opacity word) at once");
    assert!(!rt.is_moving());
}

#[test]
fn a_property_timed_twice_is_a_load_error() {
    let e = errors("transition bar.opacity duration=1\ntransition bar.{margin, opacity} duration=2\nView v { Panel bar width=10 }");
    assert!(e.iter().any(|(code, line, msg)| code == "G3008" && *line == 2 && msg.contains("line 1")), "{e:#?}");
}

/// A timed state-change rule over a `when` that sets the property is a
/// second timing; the message names both lines.
#[test]
fn a_property_also_timed_by_a_state_change_rule_is_a_load_error() {
    let e = errors("arrangement edit\ntransition base -> edit duration=0.3\nwhen edit { bar.opacity = 0 }\ntransition bar.opacity duration=1\nView v { Panel bar width=10 }");
    assert!(e.iter().any(|(code, line, msg)| code == "G3008" && *line == 4 && msg.contains("line 2") && msg.contains("line 3")), "{e:#?}");
    // Untimed state changes (no rule) leave the property rule the one timing.
    let c = compile("arrangement edit\nwhen edit { bar.opacity = 0 }\ntransition bar.opacity duration=1\nView v { Panel bar width=10 }", &|_| None);
    assert!(!gogo::has_errors(&c.diags), "{:#?}", c.diags);
}

#[test]
fn the_shape_checks() {
    let has = |src: &str, code: &str, words: &str| {
        let e = errors(src);
        assert!(e.iter().any(|(c, _, m)| c == code && m.contains(words)), "{src}\n{e:#?}");
    };
    let view = "\nView v { Panel bar width=10 { Label t text=\"x\" } }";
    has(&format!("transition bar.visibility duration=1{view}"), "G3007", "snaps");
    has(&format!("transition bar.opacity{view}"), "G3007", "duration=");
    has(&format!("transition bar.opacity duration=1, anim=pop{view}"), "G3007", "anim=");
    has(&format!("transition t.text duration=1{view}"), "G3007", "doesn't slide");
    has(&format!("transition nope.opacity duration=1{view}"), "G0060", "no node named `nope`");
    has(&format!("transition bar.opacityy duration=1{view}"), "G2001", "opacityy");
    has("View v { Panel bar width=10 { transition bar.opacity duration=1 } }", "G3007", "top level");
    has(&format!("arrangement edit\nwhen edit {{ bar.{{margin_left, margin_top}} = 3 }}{view}"), "G0059", "one property");
    has(&format!("transition bar.{{}} duration=1{view}"), "G0001", "names no property");
    has(&format!("transition bar.{{opacity margin}} duration=1{view}"), "G0001", "missing `,`");
    has(&format!("transition bar.{{opacity, margin duration=1{view}"), "G0001", "closes with `}`");
}
