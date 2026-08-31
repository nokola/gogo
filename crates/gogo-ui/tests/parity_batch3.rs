//! Pins for E4 — refuse what cannot be honored (docs/todo-gogo-parity.md
//! GPAR9, 17, 18, 20-leftover, 22, 23, 24, 25, 34, 45; decided
//! 2026-08-31): where the file says something a runtime used to drop,
//! the build now fails naming the rule. The Godot-suite ports carry most
//! of the per-rule pins (`godot_*.rs`, un-ignored with E4); these cover
//! the rest and the two design calls (one hex form, one child in
//! `empty { }`).
use gogo::page::{Len, PropKey, Rgba, State};
use gogo::runtime::{BASE, HELD, Runtime, resolve_animated};
use gogo::{Compiled, compile};

fn ok(src: &str) -> Compiled {
    let c = compile(src, &|_| None);
    assert!(!gogo::has_errors(&c.diags), "{:#?}", c.diags);
    c
}

fn codes(src: &str) -> Vec<String> {
    compile(src, &|_| None).diags.iter().map(|d| d.code.clone()).collect()
}

fn messages(src: &str) -> String {
    compile(src, &|_| None).diags.iter().map(|d| d.message.clone()).collect::<Vec<_>>().join("\n")
}

/// GPAR9: `transition base -> held` on a node matches a press (`base` =
/// the rest state a built-in flip comes from), and the release mirrors
/// it — the background used to snap both ways because the flip carried
/// no `base` end. At the view, going home mirrors a `base -> wide` rule
/// too (only `any -> wide` was searched before): the box glides back
/// instead of snapping.
#[test]
fn base_is_the_rest_end_of_a_builtin_flip_and_home_mirrors_it() {
    let c = ok("View v { Button b text=\"x\", background=#000000FF { transition base -> held duration=1, easing=linear\n when held background=#FFFFFFFF } }");
    let mut page = c.page;
    let b = page.find("b").unwrap();
    let mut rt = Runtime::new(&page);
    resolve_animated(&page, &mut rt);
    rt.set_state(b, HELD, true);
    resolve_animated(&page, &mut rt);
    rt.tick(&mut page, 0.5);
    let r = resolve_animated(&page, &mut rt);
    assert_eq!(r.color(&page, b, PropKey::Background), Some(Rgba([128, 128, 128, 255])), "half way up after 0.5 s of the 1 s rule");
    rt.set_state(b, HELD, false);
    resolve_animated(&page, &mut rt);
    rt.tick(&mut page, 0.5);
    let r = resolve_animated(&page, &mut rt);
    assert_eq!(r.color(&page, b, PropKey::Background), Some(Rgba([64, 64, 64, 255])), "the release mirrors the entering rule");

    let c = ok("arrangement wide\ntransition base -> wide duration=1, easing=linear\nwhen wide { bar.width = 300 }\nView v { Panel bar width=100, height=20, halign=left, valign=top }");
    let mut page = c.page;
    let bar = page.find("bar").unwrap();
    let mut rt = Runtime::new(&page);
    resolve_animated(&page, &mut rt);
    rt.go(0);
    resolve_animated(&page, &mut rt);
    rt.tick(&mut page, 1.0);
    let r = resolve_animated(&page, &mut rt);
    assert_eq!(r.placement[bar as usize].width, Len::Exact(300.0));
    rt.go(BASE);
    resolve_animated(&page, &mut rt);
    rt.tick(&mut page, 0.5);
    let r = resolve_animated(&page, &mut rt);
    assert_eq!(r.placement[bar as usize].width, Len::Exact(200.0), "home glides back on the mirrored rule, no snap");
}

/// GPAR45 (decided 2026-08-31): `empty { }` holds one child — with two,
/// Godot stacked them and Rust overlaid them, neither written anywhere;
/// the error names the fix. Nothing goes on the `empty` line itself.
#[test]
fn empty_holds_one_child_and_takes_nothing_on_its_line() {
    let two = "View v { Column c { repeat t max=1 { empty { Label a text=\"none\"; Label b text=\"yet\" } Label i text=\"x\" } } }";
    assert!(codes(two).contains(&"G9007".to_string()), "{:?}", codes(two));
    assert!(messages(two).contains("wrap them in a Column or Layers"), "{}", messages(two));
    ok("View v { Column c { repeat t max=1 { empty { Column { Label a text=\"none\"; Label b text=\"yet\" } } Label i text=\"x\" } } }");
    ok("View v { Column c { repeat t max=1 { empty { } Label i text=\"x\" } } }");
    assert!(codes("View v { Column c { repeat t max=1 { empty width=10 { } Label i text=\"x\" } } }").contains(&"G9007".to_string()));
}

/// GPAR17 (decided 2026-08-31): one hex form — `#F00` is refused with
/// its long spelling in hand; `/opacity` and `@pos` past 0..1 are refused
/// naming the range (they used to read as opaque and as a stop past the
/// end); a malformed number is an error, not a silent 0; a list lands only
/// on `cols=`/`areas=`/`stops=`.
#[test]
fn one_hex_form_bounded_suffixes_and_whole_numbers() {
    let short = "View v { Rectangle r color=#F00 }";
    assert!(codes(short).contains(&"G0005".to_string()));
    assert!(messages(short).contains("#FF0000"), "{}", messages(short));
    ok("View v { Rectangle r color=#FF0000; Rectangle s color=#FF000080 }");
    let big = "View v { Rectangle r color=black/50 }";
    assert!(codes(big).contains(&"G0007".to_string()) && messages(big).contains("0..1"), "{}", messages(big));
    let past = "View v { Gradient g stops=[white@0, black@2] }";
    assert!(codes(past).contains(&"G0008".to_string()), "{:?}", codes(past));
    assert!(codes("View v { Rectangle r min_height=1.2.3 }").contains(&"G0009".to_string()));
    let dot = "View v { Rectangle r opacity=.5 }";
    assert!(codes(dot).contains(&"G0009".to_string()) && messages(dot).contains("0.5"), "{}", messages(dot));
    assert!(codes("View v { Column c spacing=[1, 2] }").contains(&"G0040".to_string()));
    ok("View v { Grid g cols=[80, hug] { }; Gradient s stops=[white, black] }");
}

/// GPAR20 leftover: a `theme dark { }` block inside a node body did
/// nothing, silently — refused; the one-line override still works.
#[test]
fn a_theme_variant_block_inside_a_node_is_refused() {
    let nested = "View v { Button b text=\"x\" { theme dark { $accent = #FF0000FF } } }";
    assert!(codes(nested).contains(&"G3004".to_string()), "{:?}", codes(nested));
    ok("View v { Button b text=\"x\" { theme font_size=12 } }");
}

/// GPAR34: `state_builtin hovered, focused` is a NAME LIST (the first
/// word used to become the node's name and the rest were dropped); the
/// names must be built-ins; the heads live in a `control` body — a
/// `widget` has no script to raise or read them; an empty head is an
/// error.
#[test]
fn list_heads_take_every_name_and_belong_to_a_control() {
    ok("control C $value=0 { script \"res://x.gd\"\n state_builtin hovered, focused, held\n signal drag_started, drag_ended\n Layers { } }\nView v { C c }");
    let typo = "control C $value=0 { script \"res://x.gd\"\n state_builtin hover\n Layers { } }\nView v { C c }";
    assert!(codes(typo).contains(&"G0035".to_string()), "{:?}", codes(typo));
    let arr = "arrangement wide\ncontrol C $value=0 { script \"res://x.gd\"\n state_builtin wide\n Layers { } }\nView v { C c }";
    assert!(codes(arr).contains(&"G0077".to_string()), "{:?}", codes(arr));
    let widget = "widget W { state_builtin hovered\n Layers { } }\nView v { W w }";
    assert!(codes(widget).contains(&"G0077".to_string()), "{:?}", codes(widget));
    let bare = "control C $value=0 { script \"res://x.gd\"\n state_builtin\n Layers { } }\nView v { C c }";
    assert!(codes(bare).contains(&"G0076".to_string()), "{:?}", codes(bare));
}

/// GPAR25: a duplicate `$param` on a widget line is an error (last used
/// to win silently); `Check true` names no node `true` — the message says
/// `selected=true`.
#[test]
fn duplicate_params_and_true_as_a_name_are_refused() {
    assert!(codes("widget W $a=1, $a=2 { Layers { } }\nView v { W w }").contains(&"G0018".to_string()));
    let check = "View v { Check true }";
    assert!(codes(check).contains(&"G0046".to_string()) && messages(check).contains("selected=true"), "{}", messages(check));
}

/// GPAR23: a transition rule has one arrow and one endpoint each side —
/// extras used to be dropped; and the first word of a `transition` line
/// is its from-endpoint, never a node name (`transition wide -> any` used
/// to read as `any -> any`).
#[test]
fn a_transition_rule_has_one_arrow_and_names_its_from_end() {
    assert!(codes("arrangement a\narrangement b\ntransition base -> a -> b duration=0.1\nView v { Rectangle r }").contains(&"G0034".to_string()));
    assert!(codes("arrangement a\narrangement b\ntransition a b -> base duration=0.1\nView v { Rectangle r }").contains(&"G0034".to_string()));
    assert!(codes("View v { Button b text=\"x\" { transition -> held duration=0.1 } }").contains(&"G0034".to_string()));
    let c = ok("arrangement wide\ntransition wide -> any duration=0.2\nView v { Rectangle r }");
    assert_eq!(c.page.transitions[0].from, Some(State::Arrangement(0)));
    assert_eq!(c.page.transitions[0].to, None);
}
