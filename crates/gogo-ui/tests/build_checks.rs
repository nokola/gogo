//! Lines the build refuses, and what its messages say. Slot fills that
//! would never show, calls gogo does not have, keywords used as widget
//! names, small slips of syntax, words a kind takes no notice of, and
//! messages that used to point the wrong way.

use gogo_ui::input::{Event, EventKind, Input, Key, Mods, Phase, Touch};
use gogo_ui::layout::{FixedAdvance, Rect};
use gogo_ui::page::{NodeId, PropKey, Value};
use gogo_ui::runtime::{Runtime, resolve};
use gogo_ui::{Diagnostic, KeptFrame, Page, Severity, compile};

const VIEW: Rect = Rect { x: 0.0, y: 0.0, w: 400.0, h: 800.0 };

fn list(diags: &[Diagnostic]) -> String {
    diags.iter().map(|d| format!("  {d}\n")).collect()
}

fn diags(src: &str) -> Vec<Diagnostic> {
    compile(src, &|_| None).diags
}

/// The build refuses `src` with an error `code` whose message holds
/// `phrase`.
#[track_caller]
fn refused(src: &str, code: &str, phrase: &str) {
    let d = diags(src);
    let hit: bool = d.iter().any(|d| d.severity == Severity::Error && d.code == code && d.message.contains(phrase));
    assert!(hit, "want error {code} saying `{phrase}` for:\n{src}\ngot:\n{}", list(&d));
}

/// As `refused`, and the error is on `line`.
#[track_caller]
fn refused_at(src: &str, code: &str, line: u32) {
    let d = diags(src);
    let hit: bool = d.iter().any(|d| d.severity == Severity::Error && d.code == code && d.line == line);
    assert!(hit, "want error {code} on line {line} for:\n{src}\ngot:\n{}", list(&d));
}

/// No message for `src` holds `phrase`.
#[track_caller]
fn never_says(src: &str, phrase: &str) {
    let d = diags(src);
    assert!(!d.iter().any(|d| d.message.contains(phrase)), "no message should say `{phrase}` for:\n{src}\ngot:\n{}", list(&d));
}

#[track_caller]
fn builds(src: &str) -> Page {
    let c = compile(src, &|_| None);
    assert!(!gogo_ui::has_errors(&c.diags), "should build:\n{src}\ngot:\n{}", list(&c.diags));
    c.page
}

// ----- slot fills ------------------------------------------------------------

/// A widget with a named slot (`header`) and a default slot.
const CARD: &str = "widget Card {\n Panel p {\n Column c {\n slot header height=24 { Label text=\"Untitled\" }\n slot\n }\n }\n}\n";
/// A widget with one named slot and no default slot.
const FRAME: &str = "widget Frame {\n Panel p {\n slot body\n }\n}\n";

#[test]
fn a_fill_names_a_slot_the_widget_has() {
    refused(&format!("{CARD}View v {{\n Card c {{\n slot heder {{ Label text=\"x\" }}\n }}\n}}"), "G5004", "fills no slot of `Card`");
    refused(&format!("{CARD}View v {{\n Card c {{\n slot heder {{ Label text=\"x\" }}\n }}\n}}"), "G5004", "`header`");
    builds(&format!("{CARD}View v {{\n Card c {{\n slot header {{ Label text=\"x\" }}\n Label text=\"y\"\n }}\n}}"));
}

#[test]
fn bare_children_need_a_default_slot() {
    refused(&format!("{FRAME}View v {{\n Frame f {{\n Label text=\"x\"\n }}\n}}"), "G5004", "no default slot");
    refused_at(&format!("{FRAME}View v {{\n Frame f {{\n Label text=\"x\"\n }}\n}}"), "G5004", 8);
    builds(&format!("{FRAME}View v {{\n Frame f {{\n slot body {{ Label text=\"x\" }}\n }}\n}}"));
    // The lines an instance block holds beside content stay legal.
    builds(&format!("{FRAME}View v {{\n Frame f {{\n when screen.landscape opacity=0.5\n }}\n}}"));
}

#[test]
fn a_fill_line_takes_a_name_and_nothing_else() {
    refused(&format!("{CARD}View v {{\n Card c {{\n slot header spacing=4 {{ Label text=\"x\" }}\n }}\n}}"), "G5004", "`spacing=`");
    refused(&format!("{CARD}View v {{\n Card c {{\n slot header 10x10 {{ Label text=\"x\" }}\n }}\n}}"), "G5004", "takes a name and nothing else");
}

#[test]
fn a_slot_is_filled_once() {
    refused(&format!("{CARD}View v {{\n Card c {{\n slot header {{ Label text=\"x\" }}\n slot header {{ Label text=\"y\" }}\n }}\n}}"), "G5004", "filled twice");
}

#[test]
fn a_widget_has_one_default_slot() {
    refused("widget Two {\n Column c {\n slot\n slot\n }\n}\nView v {\n Two t { Label text=\"x\" }\n}", "G5004", "second default");
    refused_at("widget Two {\n Column c {\n slot\n slot\n }\n}\nView v {\n Two t { Label text=\"x\" }\n}", "G5004", 4);
    // A default slot written among another widget's bare children is this
    // widget's own, and its content shows there.
    let page = builds(&format!("{CARD}widget Wrap {{\n Card inner {{\n slot header {{ Label text=\"h\" }}\n slot\n }}\n}}\nView v {{\n Wrap w {{ Label mine text=\"x\" }}\n}}"));
    assert!(page.find("mine").is_some(), "the instance's child is built");
    // The same inside a named fill.
    let page = builds(&format!("{CARD}widget Wrap {{\n Card inner {{\n slot header {{ slot }}\n }}\n}}\nView v {{\n Wrap w {{ Label mine text=\"x\" }}\n}}"));
    assert!(page.find("mine").is_some(), "the instance's child is built in the named fill");
}

#[test]
fn a_theme_entry_holds_no_children() {
    refused("theme {\n Button {\n Label text=\"x\"\n }\n}\nView v { Button b text=\"y\" }", "G3006", "holds no children");
}

// ----- calls -----------------------------------------------------------------

#[test]
fn a_call_gogo_does_not_have_is_refused_on_every_word() {
    refused("View v { Label l text=upper(\"x\") }", "G0085", "`upper(…)`");
    refused("View v { Rectangle r 10x10, opacity=foo(2) }", "G0085", "`foo(…)`");
    refused("View v { Column c spacing=double(4) { Label text=\"x\" } }", "G0085", "`double(…)`");
    refused("View v { Rectangle r 10x10, color=rgb(1, 2, 3) }", "G0085", "`rgb(…)`");
    refused("View v { Button b text=\"x\" { when held opacity=foo(2) } }", "G0085", "`foo(…)`");
    refused("$n = twice(4)\nView v { Rectangle r 10x10, opacity=$n }", "G0085", "`twice(…)`");
}

#[test]
fn a_size_call_goes_on_a_size() {
    refused("View v { Rectangle r 10x10, opacity=fill(2) }", "G0085", "`fill(…)`");
    refused("View v { Column c spacing=hug(4) { Label text=\"x\" } }", "G0085", "`hug(…)`");
    builds("View v { Row r { Rectangle a height=10, width=fill(2) } }");
    builds("View v { Grid g cols=[hug, fill(2), 60] {\n row { Label a text=\"a\"\n Label b text=\"b\"\n Label c text=\"c\" }\n }\n}");
    builds("View v { Label l text=round(2.4) }");
}

// ----- keywords as names -----------------------------------------------------

/// The words the editor grammar colors as keywords, by rule.
const KEYWORDS: &[&str] = &[
    "when", "transition", "anim", "repeat", "uses", "widget", "style", "theme", "slot", "seat", "atlas", "slice", "arrangement", "key", "keys", "allow", "layout", "script", "control", "signal", "signals", "state", "state_builtin", "behavior", "gap", "row", "skip", "choice", "empty", "as", "by", "in", "and", "then", "play", "each", "group", "delay", "held", "hovered", "focused", "focus_within", "disabled", "selected", "base", "any", "true", "false", "interactive",
    "fill", "hug", "go", "toggle", "emit", "screen",
];

#[test]
fn a_keyword_cannot_name_a_widget() {
    for word in KEYWORDS {
        refused(&format!("widget {word} {{\n Label text=\"x\"\n}}\nView v {{ Label l text=\"y\" }}"), "G5001", &format!("`{word}`"));
        refused_at(&format!("widget {word} {{\n Label text=\"x\"\n}}\nView v {{ Label l text=\"y\" }}"), "G5001", 1);
    }
    for word in ["Button", "Label", "View"] {
        refused(&format!("widget {word} {{\n Label text=\"x\"\n}}\nView v {{ Label l text=\"y\" }}"), "G5001", "shadows a built-in");
    }
    builds("widget Held {\n Label text=\"x\"\n}\nView v { Held h }");
}

#[test]
fn a_name_shaped_like_a_size_says_so() {
    refused("View v { Rectangle x0 10x10 }", "G0017", "`x0` is read as a size");
    refused("View v { Rectangle x3 color=red }", "G0017", "`x3` is read as a size");
    never_says("View v { Rectangle r 10x10 color=red }", "is read as a size");
    builds("View v { Rectangle x3 }");
    builds("View v { Rectangle 10x, color=red }");
}

// ----- small slips -----------------------------------------------------------

#[test]
fn a_comma_before_a_brace_is_refused_on_the_same_line_too() {
    refused("View v {\n Button b text=\"x\", {\n Label text=\"y\"\n }\n}", "G0015", "comma before `{`");
    refused("View v {\n Button b text=\"x\",\n {\n Label text=\"y\"\n }\n}", "G0015", "comma before `{`");
}

#[test]
fn a_uses_line_takes_one_path() {
    let loader = |p: &str| -> Option<String> {
        match p {
            "a.gogo" => Some("style A min_height=1\n".to_string()),
            "b.gogo" => Some("style B min_height=2\n".to_string()),
            _ => None,
        }
    };
    let c = compile("uses \"a.gogo\", \"b.gogo\"\nView v { Rectangle .A }", &loader);
    assert!(c.diags.iter().any(|d| d.code == "G0023" && d.message.contains("one quoted path")), "{}", list(&c.diags));
    let c = compile("uses \"a.gogo\"\nuses \"b.gogo\"\nView v { Rectangle .A .B }", &loader);
    assert!(!gogo_ui::has_errors(&c.diags), "{}", list(&c.diags));
}

#[test]
fn a_translation_key_on_a_picture_address_is_refused() {
    refused("View v { Image i source=\"preview://brush\"/k }", "G0023", "`/k`");
    refused("View v { Image i source=\"res://a.png\"/k }", "G0023", "`/k`");
}

#[test]
fn an_opacity_suffix_on_a_number_is_refused() {
    refused("$n = 4\nView v { Column c spacing=$n/0.5 { Label text=\"x\" } }", "G0039", "`$n`");
    builds("$c = #ff0000\nView v { Rectangle r 10x10, color=$c/0.5 }");
}

#[test]
fn a_number_word_given_a_value_that_is_no_number_is_refused() {
    refused("$c = #ff0000\nView v { Column c spacing=$c { Label text=\"x\" } }", "G0058", "`$c`");
    refused("$c = \"hi\"\nView v { Rectangle r 10x10, opacity=$c }", "G0058", "`$c`");
    builds("$n = 4\nView v { Column c spacing=$n { Label text=\"x\" } }");
    builds("$n = 0.5\ntheme dark {\n $n = 0.8\n}\nView v { Rectangle r 10x10, opacity=$n }");
}

#[test]
fn emit_names_what_it_raises() {
    refused("View v { Button b text=\"x\", on:activate=emit() }", "G0052", "`emit(saved)`");
    builds("View v { Button b text=\"x\", on:activate=emit(saved) }");
}

#[test]
fn a_bare_svg_file_name_needs_the_scheme() {
    refused("View v { Image i source=\"icon.svg\" }", "G0071", "res://icon.svg");
}

#[test]
fn for_comes_first_on_a_widget_line() {
    refused("widget W $a=1, for=Button {\n Label text=\"x\"\n}\nView v { Button b { W } }", "G5001", "`for=` comes first");
    builds("widget W for=Button, $a=1 {\n Label text=\"x\"\n}\nView v { Button b { W } }");
}

const KNOB: &str = "control Knob $value=0, $min=0, $max=1 {\n behavior slider by $value in $min..$max\n Panel p 40x40 { slot track { slot dot automatic=position by $value in $min..$max { Circle 4x4 } } }\n}\n";

#[test]
fn a_for_style_written_above_its_control_says_where_the_control_is() {
    builds(&format!("{KNOB}style S for=Knob, opacity=0.5\nView v {{ Knob k .S }}"));
    refused(&format!("style S for=Knob, opacity=0.5\n{KNOB}View v {{ Knob k .S }}"), "G3006", "declared below");
    never_says(&format!("style S for=Knob, opacity=0.5\n{KNOB}View v {{ Knob k .S }}"), "unknown control");
    refused("style S for=Nob, opacity=0.5\nView v { Rectangle r 10x10 }", "G3006", "unknown control");
}

#[test]
fn a_shape_holds_no_children() {
    for kind in ["Rectangle r 10x10", "Circle r 10x10", "Arc r 10x10", "Image r 10x10", "Gradient r 10x10, stops=[#ff0000, #00ff00]"] {
        refused(&format!("View v {{ {kind} {{ Label l text=\"x\" }} }}"), "G7014", "holds no children");
    }
    // A shape's block still holds its `when` and `transition` lines.
    builds("View v { Rectangle r 10x10, color=red {\n when hovered color=blue\n transition any -> hovered duration=0.1\n }\n}");
    builds("View v { Panel p 10x10 { Label l text=\"x\" } }");
}

/// A page with one Input named `i`, focused by a tap.
struct Field {
    page: Page,
    rt: Runtime,
    input: Input,
    field: NodeId,
}

impl Field {
    fn new(src: &str) -> Field {
        let page = builds(src);
        let mut rt = Runtime::new(&page);
        let mut input = Input::new();
        let field = page.find("i").unwrap();
        let f = KeptFrame::fresh(&page, resolve(&page, &rt), VIEW, &FixedAdvance);
        let r = f.rects()[field.index()];
        let (x, y) = (r.x + r.w / 2.0, r.y + r.h / 2.0);
        input.touch(&page, &mut rt, f.frame(), &FixedAdvance, Touch { phase: Phase::Down, x, y, t: 0.0, id: 1 });
        input.touch(&page, &mut rt, f.frame(), &FixedAdvance, Touch { phase: Phase::Up, x, y, t: 0.1, id: 1 });
        Field { page, rt, input, field }
    }

    /// Presses one key on the focused field; returns the events it raised.
    fn key(&mut self, k: Key) -> Vec<Event> {
        let f = KeptFrame::fresh(&self.page, resolve(&self.page, &self.rt), VIEW, &FixedAdvance);
        self.input.key(&mut self.page, &mut self.rt, f.frame(), k, Mods::NONE)
    }

    fn text(&self) -> String {
        match resolve(&self.page, &self.rt).prop(&self.page, self.field, PropKey::Text) {
            Some(Value::VText { text, .. }) => self.page.str(text).to_string(),
            _ => String::new(),
        }
    }
}

fn edits(events: &[Event]) -> usize {
    events.iter().filter(|e| matches!(e.kind, EventKind::Edit(_))).count()
}

#[test]
fn typing_into_a_full_input_reports_nothing() {
    let mut f = Field::new("View v { Input i width=200, text=\"abc\", max_length=3, on:edit=Edited }");
    let events = f.key(Key::Char('d'));
    assert_eq!(edits(&events), 0, "a full field changed nothing: {events:?}");
    assert_eq!(f.text(), "abc");
    // With room for one more letter the field still reports its edit.
    let mut f = Field::new("View v { Input i width=200, text=\"ab\", max_length=3, on:edit=Edited }");
    let events = f.key(Key::Char('d'));
    assert_eq!(edits(&events), 1, "{events:?}");
    assert_eq!(f.text().len(), 3);
}

// ----- messages --------------------------------------------------------------

#[test]
fn style_as_a_property_points_at_the_dot() {
    refused("style X opacity=0.5\nView v { Rectangle r 10x10, style=X }", "G2001", "with a dot");
    never_says("style X opacity=0.5\nView v { Rectangle r 10x10, style=X }", "scale=");
}

#[test]
fn a_palette_override_says_what_it_was_given() {
    refused("$brand = #112233\ntheme {\n red = $brand\n}\nView v { Rectangle r 10x10, color=red }", "G6004", "`$brand`");
    refused("theme {\n red = 5\n}\nView v { Rectangle r 10x10, color=red }", "G6004", "`5`");
    never_says("theme {\n red = 5\n}\nView v { Rectangle r 10x10, color=red }", "did you mean 'red'");
    builds("theme {\n red = #ff0000\n}\nView v { Rectangle r 10x10, color=red }");
    builds("theme {\n red = blue\n}\nView v { Rectangle r 10x10, color=red }");
}

const STATE_BODY: &str = " behavior slider by $value in $min..$max\n Panel p 40x40 { slot track { slot dot automatic=position by $value in $min..$max { Circle 4x4 } } }\n}\nView v { K k }";

#[test]
fn a_misspelled_builtin_state_gets_the_spelling_that_builds() {
    let with = |line: &str| format!("control K $value=0, $min=0, $max=1 {{\n {line}\n{STATE_BODY}");
    refused(&with("state hover"), "G0077", "`state_builtin hovered`");
    refused(&with("state pressed"), "G0077", "`state_builtin held`");
    refused(&with("state held"), "G0077", "`state_builtin held`");
    builds(&with("state_builtin hovered"));
    builds(&with("state armed"));
}

#[test]
fn a_fill_word_on_a_box_that_draws_nothing_says_so() {
    for word in ["background=#ff0000", "color=#ff0000"] {
        let src = format!("View v {{ Row r {{\n seat s {word}\n}}\n}}");
        refused(&src, "G2002", "draws nothing");
        never_says(&src, "its fill is `color=`");
        never_says(&src, "plate is `background=`");
    }
    let src = "View v { Grid g cols=[fill] {\n row color=#ff0000 { Label a text=\"a\" }\n }\n}";
    refused(src, "G2002", "draws nothing");
    // The two pointers that were right stay.
    refused("View v { Rectangle r 10x10, background=#ff0000 }", "G2002", "its fill is `color=`");
    refused("View v { Panel p 10x10, color=#ff0000 }", "G2002", "`background=`");
}

#[test]
fn a_reserved_arrangement_name_is_called_reserved() {
    refused("arrangement base\nView v { Rectangle r 10x10 }", "G0032", "reserved");
    refused("arrangement any\nView v { Rectangle r 10x10 }", "G0032", "reserved");
    refused("arrangement\nView v { Rectangle r 10x10 }", "G0031", "needs a name");
}

#[test]
fn an_error_at_the_end_of_a_line_names_that_line() {
    refused_at("$n\nView v { Rectangle r 10x10 }", "G0012", 1);
    refused_at("View v { Rectangle r 10x10 }\n$n\n$m = 2", "G0012", 2);
    refused_at("View v {\n Rectangle r 10x10, opacity=\n Label l text=\"x\"\n}", "G0021", 2);
    refused_at("View v {\n Rectangle r 10x10, opacity=\n}", "G0021", 2);
}

#[test]
fn a_value_that_is_no_action_is_shown_as_written() {
    refused("View v { Button b text=\"x\", on:activate=3 }", "G0052", "not `3`");
    refused("View v { Button b text=\"x\", on:activate=\"hi\" }", "G0052", "not `\"hi\"`");
    never_says("View v { Button b text=\"x\", on:activate=3 }", "Num {");
}

#[test]
fn toggle_on_a_press_control_says_it_is_a_buttons_word() {
    for kind in ["Check c text=\"x\"", "Toggle c"] {
        let src = format!("View v {{ {kind}, toggle=true }}");
        refused(&src, "G2002", "a Button's word");
        never_says(&src, "is not pressed");
    }
    refused("View v { Label c text=\"x\", toggle=true }", "G2002", "a Button's word");
    refused("View v { Label c text=\"x\", tap_pulse=true }", "G2002", "is not pressed");
}

#[test]
fn a_release_word_other_than_unused_is_the_release_error() {
    refused("View v { Image i source=\"res://a.png\", release=never }", "G2020", "release=unused");
    never_says("View v { Image i source=\"res://a.png\", release=never }", "want unused");
}

#[test]
fn a_transparent_stop_is_told_how_to_write_it() {
    let src = "View v { Gradient g 10x10, stops=[#ff0000, transparent] }";
    refused(src, "G2006", "`white/0`");
    never_says(src, "passes through");
    builds("View v { Gradient g 10x10, stops=[#ff0000, red/0] }");
}

// ----- words a kind takes no notice of ---------------------------------------

#[test]
fn clip_text_is_not_a_press_controls_word() {
    for kind in ["Button b text=\"x\"", "Check b text=\"x\"", "Toggle b text=\"x\""] {
        refused(&format!("View v {{ {kind}, clip_text=true }}"), "G2002", "`clip_text=`");
    }
    refused("View v { choice c { Radio b text=\"x\", clip_text=true } }", "G2002", "`clip_text=`");
    builds("View v { Button b text=\"x\", wrap=true }");
    builds("View v { Label b text=\"x\", clip_text=true }");
}

#[test]
fn an_input_takes_no_wrap_alignment_or_value() {
    refused("View v { Input i wrap=true }", "G2002", "`wrap=`");
    refused("View v { Input i clip_text=true }", "G2002", "`clip_text=`");
    refused("View v { Input i horizontal_alignment=right }", "G2002", "`horizontal_alignment=`");
    refused("View v { Input i vertical_alignment=center }", "G2002", "`vertical_alignment=`");
    refused("View v { Input i value=3 }", "G2002", "`text=`");
    builds("View v { Input i text=\"x\", font_size=20, font_color=red, placeholder_text=\"p\" }");
}

#[test]
fn disabled_opacity_goes_in_the_theme_block() {
    refused("View v disabled_opacity=0.9 { Button b text=\"x\" }", "G2002", "`theme { }`");
    builds("theme {\n disabled_opacity = 0.9\n}\nView v { Button b text=\"x\" }");
}

#[test]
fn color_a_is_written_live_not_on_a_line() {
    refused("View v { Rectangle r 10x10, color=#336699, color.a=0.5 }", "G2005", "color=black/0.5");
    refused("style Dim color.a=0.5\nView v { Rectangle r .Dim 10x10, color=#336699 }", "G2005", "color=black/0.5");
    builds("View v { Rectangle r 10x10, color=#336699 { when hovered color.a=0.5 } }");
    builds("anim fade { r.color.a to=0, duration=0.2 }\nView v { Rectangle r 10x10, color=#336699 }");
}

#[test]
fn a_when_sets_no_state() {
    refused("View v { Button b text=\"x\", toggle=true { when held selected=true } }", "G0054", "`selected=`");
    refused("View v { Button b text=\"x\", toggle=true {\n when held {\n selected = true\n }\n}\n}", "G0054", "`selected=`");
    refused("arrangement open\nwhen open { b.selected = true }\nView v { Button b text=\"x\", toggle=true }", "G0054", "`selected=`");
    refused("View v { Panel p { when hovered b.selected=true\n Button b text=\"x\", toggle=true }\n}", "G0054", "`selected=`");
    refused("View v { Toggle t { when held value=1 } }", "G0054", "`value=`");
    refused("arrangement open\nwhen open { t.value = 1 }\nView v { Toggle t }", "G0054", "`value=`");
    builds("View v { Button b text=\"x\", toggle=true, selected=true }");
    builds("View v { Toggle t value=1 }");
}

#[test]
fn a_list_row_takes_no_placement_a_list_overrides() {
    let row = |word: &str| format!("View v {{\n List n width=200, height=300 {{\n empty {{ }}\n repeat rows by $item.id {{\n ListItem {word} {{ Label a text=$item.title }}\n }}\n }}\n}}");
    for word in ["valign=center", "aspect=2", "overlay=true"] {
        refused(&row(word), "G2002", "a List places");
    }
    for word in ["margin=4", "margin_left=4", "margin_top=4", "margin_right=4", "margin_bottom=4"] {
        refused(&row(word), "G1014", "a List places");
    }
    for word in ["max_height=20", "halign=left", "width=50", "max_width=80"] {
        builds(&row(word));
    }
}
