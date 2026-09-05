//! Pins for E4 — refuse what cannot be honored (docs/todo-gogo-parity.md
//! GPAR9, 17, 18, 20-leftover, 22, 23, 24, 25, 34, 45; decided
//! 2026-08-31): where the file says something a runtime used to drop,
//! the build now fails naming the rule. The Godot-suite ports carry most
//! of the per-rule pins (`godot_*.rs`, un-ignored with E4); these cover
//! the rest and the two design calls (one hex form, one child in
//! `empty { }`).
use gogo::page::{Len, PropKey, Color_Straight_Srgb_4xU8_RGBA, State, Value};
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
    assert_eq!(r.color(&page, b, PropKey::Background), Some(Color_Straight_Srgb_4xU8_RGBA([99, 99, 99, 255])), "half way up after 0.5 s of the 1 s rule");
    rt.set_state(b, HELD, false);
    resolve_animated(&page, &mut rt);
    rt.tick(&mut page, 0.5);
    let r = resolve_animated(&page, &mut rt);
    assert_eq!(r.color(&page, b, PropKey::Background), Some(Color_Straight_Srgb_4xU8_RGBA([33, 33, 33, 255])), "the release mirrors the entering rule");

    let c = ok("arrangement wide\ntransition base -> wide duration=1, easing=linear\nwhen wide { bar.width = 300 }\nView v { Panel bar width=100, height=20, halign=left, valign=top }");
    let mut page = c.page;
    let bar = page.find("bar").unwrap();
    let mut rt = Runtime::new(&page);
    resolve_animated(&page, &mut rt);
    rt.go_id(gogo::ArrangementId::new(0));
    resolve_animated(&page, &mut rt);
    rt.tick(&mut page, 1.0);
    let r = resolve_animated(&page, &mut rt);
    assert_eq!(r.placement[bar.index()].width, Len::Exact(300.0));
    rt.go_id(BASE);
    resolve_animated(&page, &mut rt);
    rt.tick(&mut page, 0.5);
    let r = resolve_animated(&page, &mut rt);
    assert_eq!(r.placement[bar.index()].width, Len::Exact(200.0), "home glides back on the mirrored rule, no snap");
}

/// GPAR45 (decided 2026-08-31): `empty { }` holds one child — with two,
/// Godot stacked them and Rust overlaid them, neither written anywhere;
/// the error names the fix. Nothing goes on the `empty` line itself.
#[test]
fn empty_holds_one_child_and_takes_nothing_on_its_line() {
    let two = "View v { Column c { repeat t capacity=1 { empty { Label a text=\"none\"; Label b text=\"yet\" } Label i text=\"x\" } } }";
    assert!(codes(two).contains(&"G9007".to_string()), "{:?}", codes(two));
    assert!(messages(two).contains("wrap them in a Column or Layers"), "{}", messages(two));
    ok("View v { Column c { repeat t capacity=1 { empty { Column { Label a text=\"none\"; Label b text=\"yet\" } } Label i text=\"x\" } } }");
    ok("View v { Column c { repeat t capacity=1 { empty { } Label i text=\"x\" } } }");
    assert!(codes("View v { Column c { repeat t capacity=1 { empty width=10 { } Label i text=\"x\" } } }").contains(&"G9007".to_string()));
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
    assert!(codes(nested).contains(&"G3006".to_string()), "{:?}", codes(nested));
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

/// GPAR28 (E5, 2026-08-31): `base`/`any` are reserved everywhere, not
/// only in type position — a child called `base` next to `transition
/// base -> held` would read as two things. The message names the rule
/// (it used to say "a property is `key=value`", the wrong reason).
#[test]
fn base_and_any_never_name_a_node() {
    for word in ["base", "any"] {
        let src = format!("View v {{ Button {word} }}");
        assert!(codes(&src).contains(&"G0046".to_string()), "{}", messages(&src));
        assert!(messages(&src).contains("reserved") && !messages(&src).contains("key=value"), "{}", messages(&src));
    }
    ok("View v { Button basic }
");
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
    assert_eq!(c.page.transitions[0].from, Some(State::Arrangement(gogo::ArrangementId::new(0))));
    assert_eq!(c.page.transitions[0].to, None);
}

/// GPAR31 / E6 (decided 2026-08-31, A): `/o` replaces the alpha of the
/// value it is written on — the suffix at the slot is the alpha the slot
/// gets, through a theme value with its own alpha, a variant, a
/// `$a = $b/o` hop and a widget param, and on a pasted 8-digit hex. Rust
/// used to apply the INNER suffix on a chain: `$a = $b/0.5` + `$a/0.3`
/// drew 0.5 while Godot drew 0.3, both from a clean build.
#[test]
fn the_suffix_at_the_slot_sets_the_alpha() {
    fn bg(src: &str, node: &str, variant: Option<&str>) -> Color_Straight_Srgb_4xU8_RGBA {
        let c = ok(src);
        let page = c.page;
        let n = page.find(node).unwrap();
        let mut rt = Runtime::new(&page);
        if let Some(v) = variant {
            rt.variant = page.intern_lookup(v);
        }
        let r = resolve_animated(&page, &mut rt);
        r.color(&page, n, PropKey::Background).unwrap()
    }
    let a = |o: f32| (o * 255.0).round() as u8;
    assert_eq!(bg("View v { Panel p background=#00000080/0.5 }", "p", None), Color_Straight_Srgb_4xU8_RGBA([0, 0, 0, a(0.5)]), "a pasted 8-digit hex takes the suffix");
    assert_eq!(bg("$flash = #FFFFFF6B\nView v { Panel p background=$flash/0 }", "p", None), Color_Straight_Srgb_4xU8_RGBA([0xFF, 0xFF, 0xFF, 0]), "the fade idiom: the same color, transparent");
    assert_eq!(bg("$dim = #80808080\nView v { Panel p background=$dim/0.3 }", "p", None), Color_Straight_Srgb_4xU8_RGBA([0x80, 0x80, 0x80, a(0.3)]), "over a theme value with its own alpha");
    assert_eq!(bg("$dim = gray\ntheme dark { $dim = #80808080 }\nView v { Panel p background=$dim/0.3 }", "p", Some("dark")), Color_Straight_Srgb_4xU8_RGBA([0x80, 0x80, 0x80, a(0.3)]), "through a variant");
    assert_eq!(bg("$a = $b/0.5\n$b = #101010\nView v { Panel p background=$a/0.3 }", "p", None), Color_Straight_Srgb_4xU8_RGBA([0x10, 0x10, 0x10, a(0.3)]), "the slot's suffix over a hop's (Rust drew the hop's 0.5)");
    assert_eq!(bg("$a = $b/0.3\n$b = #10101080\nView v { Panel p background=$a }", "p", None), Color_Straight_Srgb_4xU8_RGBA([0x10, 0x10, 0x10, a(0.3)]), "a hop's suffix over a value defined later in the file");
    assert_eq!(bg("widget Chip $tint=red { Panel face background=$tint/0.2 }\nView v { Chip c tint=#20202080 }", "c", None), Color_Straight_Srgb_4xU8_RGBA([0x20, 0x20, 0x20, a(0.2)]), "the widget's suffix over the caller's alpha");
}

/// GPAR30: a theme `$value` is written in color context — `$dim = gray`
/// is the COLOR gray at its use site (`background=$dim` draws a plate),
/// `$scrim = black/0.5` carries its alpha — and `font_color=` takes
/// palette words (`font_color=accent` used to fall through to the white
/// default with a clean build). A bare word that is no color is refused.
#[test]
fn theme_values_and_font_color_are_color_positions() {
    let c = ok("$dim = gray\n$scrim = black/0.5\nView v { Rectangle r width=10, height=10, color=$dim\n Label l text=\"x\", font_color=accent }");
    let dim = c.page.theme.iter().find(|t| c.page.str(t.name) == "dim").expect("dim entry");
    assert!(matches!(dim.value, Value::VColor(_)), "{:?}", dim.value);
    let scrim = c.page.theme.iter().find(|t| c.page.str(t.name) == "scrim").expect("scrim entry");
    let Value::VColor(s) = scrim.value else { panic!("{:?}", scrim.value) };
    assert!(s.0[3] > 0 && s.0[3] < 255, "the /0.5 alpha rides the theme value: {s:?}");
    let l = c.page.find("l").expect("l");
    assert!(matches!(c.page.prop(l, PropKey::FontColor), Some(Value::VColor(_))));
    assert!(messages("View v { Label l text=\"x\", font_color=nope }").contains("is not a color"));
    assert!(messages("$bad = nope\nView v { Label l text=\"x\" }").contains("is not a color"));
}

/// GPAR10: a `when held sibling.color=` response is TIMED by the
/// DECLARING node's rules — one timing per flip, resolved where the
/// `when` lives; the target's own rules are only a fallback. (Also
/// exercises GPAR83: the target is a SIBLING outside b's subtree.)
#[test]
fn a_child_response_times_by_the_declaring_scope() {
    let c = ok("View v { Button b text=\"x\" { transition base -> held duration=1, easing=linear\n when held flash.color=#FFFFFFFF }\n Rectangle flash width=10, height=10, color=#000000FF }");
    let mut page = c.page;
    let b = page.find("b").unwrap();
    let flash = page.find("flash").unwrap();
    let mut rt = Runtime::new(&page);
    resolve_animated(&page, &mut rt);
    rt.set_state(b, HELD, true);
    resolve_animated(&page, &mut rt);
    rt.tick(&mut page, 0.5);
    let r = resolve_animated(&page, &mut rt);
    assert_eq!(r.color(&page, flash, PropKey::Color), Some(Color_Straight_Srgb_4xU8_RGBA([99, 99, 99, 255])), "half way, timed by b's own rule");
}
