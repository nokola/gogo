# Gogo Cookbook

Short answers to common questions. Each recipe is complete — copy it, rename things,
save, and watch it hot reload.

---

## How do I create a button?

```kdl
Button okButton text="OK"
```

The type is `Button`, `okButton` is its name. A named button auto-wires its events
by convention: `activate` (the completed tap, or a key/gamepad accept) calls
`OnOkButtonActivate()` in the code-behind. To wire explicitly instead:

```kdl
Button okButton text="OK", on:activate=HandleOk
```

---

## How do I choose a parent (Column, Row, Layers, Panel)?

Four answers cover ~every layout:

| use | you want |
|---|---|
| `Column` | children in a line, top-to-bottom |
| `Row` | children in a line, left-to-right |
| `Layers` | children overlapping (stacked in depth, later = on top) |
| `Panel` | a styled box around one thing — background, corner radius, padding |

`Column` stacks on y, `Row` on x, `Layers` on z — a `Layers` sizes itself to
its biggest child and lets each child place itself (`halign`/`valign`,
`width`/`height`, `margin`). A child with no placement keys takes the whole
space its parent gives it, so a backdrop is just the first layer with a
color — `Rectangle bg color=$chrome` (the explicit `width=fill,
height=fill` spelling means the same). Godot's stock classes
(`GridContainer`, `TextureRect`, ...) still work by name when you need
them, but these four are the vocabulary the rest of this cookbook speaks.
(A file's ROOT is `View` — see "How do I start a view file?".)

---

## How do I start a view file?

The root type is `View` — the host class IS this node, and it comes with
the defaults every view wants (transparent background, fill both axes):

```kdl
uses "styles/theme.gogo"

View myPageRoot {
    Column menu halign=center, valign=center, spacing=12 {
        Button startButton text="Start"
    }
}
```

Anything you spell wins over the defaults (`View splash background=black`
paints black; `View bar valign=bottom` hugs the bottom instead of filling).
When the root itself IS a styled box — a toast banner, a progress track —
use `Panel` at the root instead: it keeps the plate look that View's
transparent default exists to remove.

---

## How do I style a button's held look? (pressed feedback)

States are answered with `when` inside a style. The state is `held` —
input is down on the control; put the pressed-down look in a
`when held` response:

```kdl
style ActionButton min_height=36, background=$chrome {
    when held background=$accent
}

Button okButton .ActionButton text="OK"
```

Pressing the button applies the `when held` properties; releasing restores
the base style. With no `transition` rule, the change is instant.

To time the press and the release (press fast, release relaxed), add
`transition` rules — how a change *feels* is a separate line from what it
*is*:

```kdl
style ActionButton min_height=36, background=$chrome {
    transition any -> held duration=0.05, easing=cubic_out   // finger down
    transition held -> any duration=0.2,  easing=elastic_out // finger up
    when held background=$accent
}
```

With only the entering rule, leaving mirrors it. (A scale-down squish while
the press is held is just another standing value — `when held scale=0.95`,
next recipe.)

---

## How do I shrink (scale) a button while it's pressed?

Motion words are standing responses like any other — put the scale next to
the color:

```kdl
style MenuButton width=220, height=48 {
    transition duration=0.08
    when held background=#FFFFFF, scale=0.95
}
```

The value holds while the state holds and hands back on release, timed
by the `transition` rules. The vocabulary: `scale=` (uniform — pops from the
center, the WP tilt feel), `scale.x=`/`scale.y=` per axis, `translate.x=`/
`translate.y=` (epx offsets), `rotate=` (degrees).

For choreographed press feedback (bounce, shake, multi-part) point a
transition's `anim=` at an anim instead — but one property gets one writer:
a `when` can't hold a property the transition's anim also drives (load-time
error).

---

## How do I show keyboard/gamepad focus? (game menus)

You mostly don't have to: every gogo button shows its theme's focus ring
for key-driven focus out of the box — and ONLY key-driven (D129): a tap
parks engine focus silently, so no ring is left standing after every
touch. To replace the ring with your own look, respond to `focused` — your
response takes the default's place:

```kdl
// controller main menu: the focused entry pops instead of wearing a ring
style MenuEntry min_height=44 {
    transition duration=0.1, easing=cubic_out
    when focused background=accent/0.3, scale=1.05
}
Column spacing=8 {
    Button startButton .MenuEntry text="Start"
    Button optionsButton .MenuEntry text="Options"
}
```

When the focused control sits INSIDE the thing that should highlight —
the console settings-row pattern, where left/right drives a slider but
the whole row lights — that's `focus_within` on the container:

```kdl
style SettingsRow padding=8, corner_radius=4 {
    transition duration=0.08
    when focus_within background=#FFFFFF1E
}
Column spacing=2 {
    Panel musicRow .SettingsRow {
        Row spacing=12 {
            Label text="Music volume", width=fill
            Slider music value=40, min=0, max=100, width=160, valign=center
        }
    }
}
```

Both are visible-focus states: a text field (`Input`) is the exception
and indicates on any focus — the caret is coming.

---

## How do I make my UI accessible?

Most of it is already done: buttons show a keyboard-only focus ring,
composites (`Slider render=gogo`, `Toggle`) report role/value/on-off to
screen readers as ONE element with working increment/decrement actions,
native nodes report themselves, and the touch_target lint flags anything
interactive under 44 epx. What's left for you is mostly ONE property:
`accessibility_name` wherever a control has no text of its own — an
icon-only button, or a slider whose caption is a separate `Label`:

```kdl
style SettingsRow padding=8, corner_radius=4 {
    transition duration=0.08
    when focus_within background=#FFFFFF1E
}
Column spacing=2 {
    Panel musicRow .SettingsRow {
        Row spacing=12 {
            Label text="Music volume", width=fill
            Slider music value=40, min=0, max=100, width=160, valign=center, accessibility_name="Music volume"
        }
    }
    Panel muteRow .SettingsRow {
        Row spacing=12 {
            Label text="Mute effects", width=fill
            Toggle muteToggle accessibility_name="Mute effects"
        }
    }
}
```

A reader hears "Music volume, slider, 40" / "Mute effects, switch, off" —
and adjusts both without touching the screen. Three rules come with it: when
you replace the focus default (`when focused ...`), keep the look clearly
distinct from resting; custom `control`s declare what they are with one
behavior line (`accessibility_role() -> &"slider"`); and a toast/banner
that appears on its own gets `accessibility_live=polite` so it's spoken.
Reduced motion is one call — `gogo.set_reduce_motion("on")` completes
every transition and anim instantly, values intact; wire it to an app
setting. The full story lives in the info pane's **Accessibility**
section.

---

## How do I change a button's text color (foreground)?

Text color is `font_color=` — the text half of `background=`. It pairs with
`font_size`, works statically and in `when` responses, and round-trips with
the theme the same way `background` does:

```kdl
style MenuButton width=220, height=48 {
    when held background=#FFFFFF, font_color=#000000
}
```

WPF's `Foreground` and CSS's `color` both land here. Gogo's `color=` is
only the fill of nodes that own one (`Rectangle`, `Circle`) — on anything
else it's an error that points you to `font_color=`/`background=`/`opacity=`.

---

## How do I style a toggled (selected) button?

`held` is the momentary state while a finger is down; `selected` persists while a
`toggle=true` button is on. A selection look belongs in `selected`, so it doesn't
inherit the press choreography:

```kdl
style GalleryItem background=$chrome {
    transition any -> selected duration=0.1, easing=cubic_out
    when held background=#FFFFFF22          // momentary flash while the press is held
    when selected background=$accent        // persists while toggled on
}

Button paintRow .GalleryItem toggle=true, text="paint"
```

---

## How do I make one button style differently on press?

`when` responses work directly on a node, same form as in a style:

```kdl
Button dangerRow .GalleryItem text="Delete" {
    when held background=#B00020
}
```

Merge order, most local wins: style base → style when → node properties → node
when override.

---

## How do I style multiple buttons the same way?

Define a style once, apply it with `.StyleName`:

```kdl
style ToolButton min_height=36, tap_pulse=true, background=$chrome

Button paintButton  .ToolButton text="Paint"
Button smudgeButton .ToolButton text="Smudge"
Button undoButton   .ToolButton text="Undo"
```

Local properties override the style, so exceptions stay one-liners:

```kdl
Button bigButton .ToolButton text="Huge", min_height=72
```

Styles usually live in a shared file, pulled in with `uses`:

```kdl
uses "styles/theme.gogo"
```

(If you guess `style=ToolButton` instead, the error redirects you: *"use
`.ToolButton` to apply a style"*.)

---

## How do I style a control I build in code?

A component that creates its buttons in code (GDScript or C#) uses the same
declared styles through `apply_style` — name the file the style lives in,
because styles are file-scoped:

```gdscript
var button := Button.new()
gogo.apply_style(button, "res://ui/object_page.gogo", "ToolButton")
```

```csharp
// C# hosts call through the shared runtime object (see Runtime API)
gogo.Call("apply_style", button, "res://ui/object_page.gogo", "ToolButton");
```

The control gets the style's standing props now and its `when` responses
(held, hovered, disabled, ...) through the same machinery a declared
`.ToolButton` node uses. A style that needs declared structure around the
control is refused with an error: responses reaching other nodes
(`mark.opacity=`), `slot.` paths, and `anim=` transition rules.

---

## How do I style multiple buttons the same way, with animation?

Put the responses and their timing in the shared style — every button using
the style gets the same behavior:

```kdl
// styles/theme.gogo
$accent = #E51400
$chrome = #1F1F1F

style ToolButton min_height=36, background=$chrome {
    transition any -> held duration=0.05, easing=cubic_out
    transition held -> any duration=0.2,  easing=elastic_out
    when held     background=$accent
    when disabled opacity=0.4
}
```

```kdl
// toolbar.gogo
uses "styles/theme.gogo"

Row spacing=10 {
    Button paintButton  .ToolButton text="Paint"
    Button smudgeButton .ToolButton text="Smudge"
    Button undoButton   .ToolButton text="Undo"
}
```

Edit the style's timing, save, and every button updates live.

---

## How do I define theme colors and use them?

Declare values with `$name = value` (usually in a shared file); reference them
anywhere a value goes:

```kdl
// styles/theme.gogo
$accent        = #E51400      // #RRGGBB — opaque
$foreground    = #FFFFFF
$swatchDefault = #EB6428
$scrim         = #000000AA    // #RRGGBBAA — alpha last
```

```kdl
Rectangle swatch color=$swatchDefault
Label title text="Hello", font_color=$foreground
```

A typo like `$swatchDefalt` is a load-time error with a suggestion.

Code that draws with a theme color (a GPU outline, a chart) reads it by
name, following the dark/light variant:

```rust
let accent = rt.theme_color(&page, Theme::Accent); // the page's `enum Theme`
```

(You don't have to start from hex: named colors — `red`, `accent`,
`surface`, ... — are ready to use in any color position; next recipe.)

---

## How do I use named colors — and get dark mode for free?

Gogo ships a built-in palette: bare color names, legal in any color-typed
position (the way `fill`/`hug` work on `width=`).

```kdl
style DeleteButton {
    when held background=danger       // semantic token — reads as intent
}
Panel card background=surface {      // flips with dark mode
    Label hint text="3 items", font_color=text_muted
}
Rectangle stripe color=teal           // literal — same in every mode
Rectangle scrim  color=black/0.5      // any color takes /opacity (final alpha)
```

Three kinds of names:

- **Literals** — `red orange yellow green teal blue indigo purple pink
  brown` plus the neutral ramp `smoke silver gray charcoal` and
  `transparent`. Softened, readable defaults (not CSS's neon).
  `black`/`white` are exact `#000`/`#FFF`, reserved for testing — leave
  them alone.
- **Semantic tokens** — `accent on_accent danger warning success info`.
  Aliases into the literals, resolved at theme time: remap
  `theme { accent = purple }` once and every widget follows.
- **Mode tokens** — `surface panel border text text_muted`. These carry
  light *and* dark values; `Gogo.set_theme_variant("dark")` flips them all
  (the well-known variant name), no theme file needed.

Overrides go inside theme blocks, no `$` prefix — and your values always
win. Override one literal and the semantic tokens pointing at it follow:

```kdl
theme {
    red = #E51400         // WP-red everywhere, danger included
}
theme dark {
    surface = #141414     // beats the built-in dark surface
}
```

---

## How do I make a reusable component?

Declare a `widget`. `$name=default` on the declaration line is a parameter;
reference it inside the body:

```kdl
widget LabeledSlider $label="", $min=0, $max=100, $value=0 {
    Column spacing=4 {
        Label text=$label
        Slider signals min=$min, max=$max, value=$value
    }
}
```

Instantiate it like a built-in:

```kdl partial
LabeledSlider sizeSlider     label="Size", min=1, max=300, value=80
LabeledSlider hardnessSlider label="Hardness", value=30
```

`signals` marks which inner node exposes events: `sizeSlider`'s `change`
auto-wires to `OnSizeSliderChange(value)` — the committed value; the live
per-tick stream is `edit` → `OnSizeSliderEdit(value)`.

---

## How do I let a component accept children?

Put a `slot` where instance children should go:

```kdl
widget Card {
    Panel padding=12 {
        Column spacing=8 {
            slot
        }
    }
}

Card infoCard {
    Label text="Any children go into the slot"
    Button moreButton text="More"
}
```

---

## How do I add named slots to a widget?

Name a slot to give a widget several content regions; give the region props
to place and paint it. Instances fill by name with `slot <name> { ... }`;
bare children keep going to the bare (default) slot:

```kdl
widget StatCard {
    Panel corner_radius=6, padding=12 {
        Column spacing=8 {
            slot header height=24, width=fill { Label text="Untitled" }  // default content
            slot                                  // the default slot, as before
            slot footer background=#00000033, corner_radius=3, padding=4  // a region needs no content
        }
    }
}

StatCard runCard {
    slot header { Label text="Run stats", font_size=14 }   // replaces "Untitled"
    Label text="12 strokes"                                // -> default slot
    slot footer { Label text="updated now", font_size=11 }
}
```

- A named slot is a **region**: its line takes placement (`width`/`height`/
  `halign`/`valign`/margins/`WxH`) and the box family (`background`/
  `corner_radius`/`border-*`/`padding`). Content is clipped to it and fills
  it; `padding` pushes content in; the region takes its content's space in a
  stack, so it never collapses.
- Filling replaces that slot's **default content** entirely. Box props stay
  (they belong to the region). Slots declared *inside* another slot's content are
  structure — they survive a fill and draw above it.
- Restyling content is filling: `slot label font_size=12` errors — bring the
  node instead (`slot label { Label font_size=12 }`).
- Typos are caught: an unknown fill name errors with the widget's slot list;
  bare children on a widget without a default slot error naming the fills.

---

## How do I show a list of items (repeat)?

Declare the shape in gogo; hand the data over from code. `repeat` renders
one template per item, and the `by` key is each item's identity.

```kdl partial
Column spacing=8 {
    Label text="Today"
    repeat tasks by $item.id {
        empty { Label text="Nothing due today" }
        TaskRow title=$item.title, done=$item.done
    }
}
```

```gdscript
# In the view's code-behind (a GogoView/GogoPanelView subclass), typically
# from _on_view_loaded(); elsewhere: Gogo.set_items(view_root, "tasks", ...).
set_items("tasks", [
    {"id": 1, "title": "Sketch", "done": false},
    {"id": 2, "title": "Ink",    "done": true},
])
```

- Assignment IS the change event: re-assign and gogo reconciles by key —
  items whose data didn't change keep their nodes (focus and states
  survive), changed items rebuild, removed keys free, order follows the
  array. Sort or filter app-side and just assign the result.
- `$item.field` reads the item's data; `as $task` renames the binding
  (`repeat tasks as $task by $task.id`). Primitive collections key on the
  item itself: `repeat names by $item`.
- `empty { ... }` is REQUIRED, first in the body — the zero-items screen,
  which also shows before the first assignment; `set_items(view, "tasks",
  [])` clears back to it. Truly nothing to show? A literal `empty { }`
  says so explicitly.
- Buttons inside the template know their row: any handler wired in the body
  receives the item's key as its last argument
  (`func OnRowActivate(key): ...`).
- Bad data — a duplicate `id` above all — refuses the WHOLE assignment with
  a loud console block, and the list keeps showing the last good items.
  Fix the data, assign again.
- Selection, keyboard navigation and activation belong to the `List`
  control (next recipe) — a plain `repeat` in a `Column`/`Scroll` is pure
  rendering.

---

## How do I make a selectable list (List)?

Put the repeat inside a `List` instead of a `Column`, and root each row in
a `ListItem` — the selectable/focusable/tappable unit (required, D121).
The chosen row's `ListItem` holds the universal `selected` (style it THERE),
the keyboard cursor is the `focused` state, `when held` gives WP press
feedback, and tapping a row fires `On<Name>Activate`. The List scrolls
itself — no `Scroll` wrapper, ever. A bare list already shows selection,
the cursor and press feedback (the theme's list-selection fill and its
echoes, D122); styles like the ones below replace those defaults per state.

```kdl
style TaskLook padding=8 {
    when selected background=accent
    when held opacity=0.7
}
List picker select=many, spacing=2 {
    Label text="Today"
    empty { Label text="Nothing due today" }
    repeat tasks by $item.id {
        ListItem .TaskLook { Label text=$item.title }
    }
}
```

```gdscript
set_items("tasks", tasks)                      # data, exactly as before

func OnPickerChange(selected) -> void:         # the selection changed —
    save_selection(selected)                   # an Array under select=many

func OnPickerActivate(key, item) -> void:      # an item was tapped/Entered
    open_task(key)

# Code owns the selection too — a live property on the node:
var picker := view("picker")
picker.selected = ["t1", "t9"]                 # keys; [] clears
```

- `select=one` (default) — `selected` is a key or `null`; tapping selects,
  tapping the selected item again does NOT deselect (radio doctrine).
  `select=many` — taps toggle. `select=off` — no selection at all, items
  just focus and activate (menus).
- Selecting keys whose items haven't loaded yet is fine — they sit
  **inert** and light up when the data arrives (select-then-load).
- The body is statics plus exactly ONE repeat, and each row template is
  ONE `ListItem` root — a drawn box like `Panel` (`background=`,
  `padding=`); multi-part rows lay out with a `Row`/`Column` inside it.
  A `when selected` on a node INSIDE the row never fires — the List drives
  the root (linted, G9014).
- The List's box is its VIEWPORT: unplaced, it expands to the space its
  parent offers and the rows scroll inside (statics scroll along, the
  cursor stays in view); `height=` pins it. Fixed content — statics and
  the `empty` text — counts into the minimum (D127), so a bare list shows
  its header and "none yet" line even in a HUG parent (a centered menu
  `Column`, a scrolling page) — but the ROWS starve there: provable cases
  warn at build time (G9015), the rest at runtime. Fixes: `height=<epx>`,
  or `height=hug` to size to the items (a bare hug never scrolls —
  content past the screen overflows). `height=hug, max_height=400` is the
  full centered-menu arc: the block grows with the items until the cap,
  then the list scrolls.

---

## How do I build a control (slider, knob)?

When input must become a VALUE, declare a `control`: a widget plus an
input behavior (`behavior`), declared states/signals, and slots the value
drives. For a slider-shaped control the stock behavior does it all:

```kdl
control Fader $value=0, $min=0, $max=100, $step=0 {
    behavior slider by $value in $min..$max, step=$step   // the stock slider behavior
    // `held` IS the drag state — the runtime's gesture tracking maintains
    // it (D181); a behavior never writes a state
    state_builtin hovered, focused, disabled, held
    signal drag_started, drag_ended             // raised at the drag's edges
    transition duration=0.12, easing=cubic_out  // an unstyled Fader still glides

    Layers {
        slot track width=fill, height=24, background=$chrome {
            slot progress automatic=length by $value in $min..$max { Rectangle color=$accent }
        }
        slot label { Label text=live($value) }  // updates as it drags
        slot thumb automatic=position by $value in $min..$max { Circle width=20, height=fill, color=white }
    }
}

Column width=fill, spacing=10 {
    Fader gain value=25, width=fill   // drag_started auto-wires OnGainDragStarted
    Fader vol  value=50, width=fill { when held opacity=0.85 }
}
```

Every automatic coupling is fully spelled — the kind (config in its
parens for the radial kinds), the driver after `by`, the range after
`in`. The forms at a glance:

```kdl
slot fill  automatic=length by $value in $min..$max
slot thumb automatic=position by $value in $min..$max
slot mark  automatic=presence by selected
slot wedge automatic=arc(from=-225, sweep=270) by $value in $min..$max
slot dot   automatic=angle(from=-225, sweep=270) by $value in $min..$max
```

A switch is the other stock behavior — `behavior toggle by $value in
$min..$max` over a `slot track` holding a `slot thumb`: a tap glides to
the other end, a drag settles nearest, Enter/Space flip.

A shape neither fits (a knob, a vertical fader) is a behavior of your own:
a Rust type implementing `gogo_ui::Behavior` that only ever sees values —
the value triad and the slots as control-local rects — and writes only
the value. The runtime keeps everything else (`held`, `edit`/`change`,
the drag edges, restore on cancel, the glide):

```rust
use gogo_ui::{Behavior, BehaviorSpec, Claim, Cx, KeyAction, KEYS_ARROWS, Payload};

/// A vertical fader: the value follows y over the track, bottom = min.
#[derive(Default)]
struct VFader;

static VFADER: BehaviorSpec = BehaviorSpec { name: "vfader", slots: &["track"], keys: KEYS_ARROWS, payload: Payload::Number };

impl VFader {
    fn apply(cx: &mut Cx, y: f32) {
        let track = cx.slot(0);                                   // the spec's slot order
        cx.set_fraction((track.y + track.h - y) / track.h);       // snaps to step=, clamps
    }
}

impl Behavior for VFader {
    fn spec(&self) -> &'static BehaviorSpec { &VFADER }
    fn began(&mut self, cx: &mut Cx, _x: f32, y: f32) -> Claim { Self::apply(cx, y); Claim::Take }
    fn moved(&mut self, cx: &mut Cx, _x: f32, y: f32) { Self::apply(cx, y); }
    fn key(&mut self, cx: &mut Cx, action: KeyAction) -> bool {
        match action {
            KeyAction::Increment => cx.set_value(cx.value() + cx.key_step()),
            KeyAction::Decrement => cx.set_value(cx.value() - cx.key_step()),
            _ => return false,                                    // Home/End: not ours
        }
        true
    }
}
```

Two registrations, then the file names it:

```rust
// build.rs — the compiler checks the `.gogo` against the spec (slots, keys)
CompileOptions { behaviors: &[VFADER], ..CompileOptions::default() }
// at run — the page's Input routes to the behavior, by name
input.register(Box::new(VFader));
```

```kdl partial
control Vol $value=5, $min=0, $max=10, $step=0 {
    behavior vfader by $value in $min..$max, key_unit=10%
    state_builtin focused, held
    keys home_end                     // the behavior leaves Home/End: yours to claim
    Layers width=40, height=200 { slot track width=fill, height=fill { Rectangle color=$chrome } }
}
```

`began` returns `Claim::Pass` to let a press go on as if there were no
behavior (a knob that turns only from its handle). Dragging writes the
value directly; a tap's settle, a key step or a code write glides through
the `transition` rule. The arrows, Home/End AND the mouse wheel all reach
the behavior as logical actions — a vertical wheel tick is
`Increment`/`Decrement` on the instance under the pointer, no focus
needed. `live($value)` keeps the label current — a bare `$value` in the
body errors with the wrap fix-it (it would freeze). Float displays round
to 0.1 by default; `live(round($value, 1))` picks another step. The
position thumb takes its width from its content — give the Circle a real
width (`width=20`), not `width=fill`.

---

## How do I restyle a control's parts (slots)?

Point a style at the control with `for=` (always the first property) — slot
paths and state names are then checked as you type:

```kdl partial
style ThinFader for=Fader, slot.track.height=4, slot.thumb.size=8 {
    when held    slot.thumb.scale=1.2
    when focused slot.track.background=$accent
}

Fader vol .ThinFader value=50
Fader gain slot.track.corner_radius=3, value=25   // one-off: no style needed
```

- Typos die at the declaration: `slot.thmb` lists the real slots,
  `when hovred` lists the real states, and a driven property
  (`slot.thumb.x`) errors — the control moves it, you style its look.
- Slot responses in `when` take the motion words (`scale`, `translate.x`,
  `rotate`, `opacity`) and the box family (`background`, `corner_radius`,
  `border-*`).
- A plain style (no `for=`) may respond to `held` too — it's checked
  where you apply it, and works on any press surface (buttons, `ListItem`
  rows, sliders, controls declaring `state_builtin held`).

---

## How do I restyle the built-in Slider?

A plain `Slider` is Godot's native HSlider — it has no slots to restyle.
Opt the instance into gogo's implementation with `render=gogo` (it looks
the same until you style it), then style it like any control:

```kdl stale
style ThinSlider for=Slider, slot.track.corner_radius=0 {
    when held    slot.thumb.scale=1.15
    when focused slot.track.background=$accent
}

Slider volume .ThinSlider render=gogo, value=$vol, min=0, max=100, step=5
```

The slots are `track`, `progress` (grows with the value), `thumb` (follows
the value) and `label` (empty by default — fill it):

```kdl stale
Slider gain render=gogo, value=0, min=-60, max=6 {
    slot label { Label text="gain", font_size=12 }
}
```

To flip every slider at once, make it app policy in the theme — and opt
out per instance where code-behind still casts `as HSlider`:

```kdl stale
theme {
    Slider .ThinSlider render=gogo    // every Slider: this look, gogo backend
}
Slider legacy render=native, value=1  // opts out — stays a real HSlider
```

You don't have to memorize any of this: using a `for=Slider` style (or a
fill) on a native slider errors with `render=gogo` offered as the fix. The
one cost is stated there too — `as HSlider` stops working on an opted-in
instance; read `value` / listen to `on:change` (committed) or `on:edit`
(live) instead, both backends speak them.

---

## How do I animate a panel entrance (choreography)?

For anything beyond A→B state changes, declare a named `anim`. An anim is a
chain of steps — every line says when it runs: `and` joins the previous
step (they run together), `then` starts after it. Each line targets
`nodeName.property.path`; `duration_each=` on the header is every step's
default (per-step by NAME — it is never the anim's total):

```kdl partial
anim appear duration_each=0.5, easing=cubic_out {
    listBrush.translate.x from=-60, to=0
    and panelMiddle.opacity from=0, to=1, duration=0.3   // its own wins
    and imgArrow.translate.x {
        key t=0,   v=14, jump=true
        key t=0.8, v=0,  easing=cubic_out
        key t=0.8, v=4,  jump=true        // same t twice = discontinuity
        key t=1.0, v=0                    // easing defaults to linear
    }
    then imgArrow.scale to=1.05, duration=0.08, autoreverse=true
}
```

Motion words (`translate`/`scale`/`rotate`/`opacity`) use a visual-only
channel — layout never moves, containers can't stomp it, and it works on any
node (no restructuring a tree to animate it). Layout geometry (`position`,
`offset_*`, sizes) is not animatable: layout is declared, motion is
transformed.

- `from`/`to` is shorthand for two keys. `to=` alone starts from the LIVE
  value when the step begins (interruptible fades); `from=` pins the start
  and holds it from play start.
- Key `t` is in seconds; a keyframed line's duration **is** its last key's `t`.
- `jump=true` sets the value instantly (no interpolation into that key).
- `then` waits for the whole `and`-cluster before it (longest member,
  autoreverse's way back included); a pause in the chain is its own step
  (`then delay=0.3`), and a signed `offset=` shifts any step (negative
  overlaps the previous step's tail).
- Crossing dependencies graduate to named `group`s + `after=` (next
  recipe); staggering wearers to `each` blocks ("How do I stagger buttons
  into a menu?").

Play it from code:

```gdscript
gogo.play(view, "appear")
```

---

## How do I sequence animations across nodes? (dim, then drop)

`then` already sequences across nodes — lines keep their dotted targets, so
a chain choreographs several controls. `group` blocks come in when steps
SHARE timing words, or when a step must be referenced by name (`after=` —
the escape hatch for dependencies that cross the `and`/`then` flow):

```kdl partial
anim openMenu {
    group dim { backdrop.opacity to=0.6, duration=0.15 }
    then group drop duration_each=0.25 {          // starts when dim ends
        menu.translate.y from=-40, to=0, easing=back_out
        and menu.opacity from=0, to=1
    }
    group flash after=dim { backdrop.scale to=1.02, duration=0.1 }
}
```

- `then` waits for the previous step's whole cluster; `after=dim` starts
  when THAT group's chain completes, no matter what else runs — a
  referenced step stands alone (no connective; the reference places it).
  To overlap an anchor's tail, shift with `offset=` (`after=dim,
  offset=-0.05`). Unknown names and cycles are load-time errors — retiming
  `dim` never breaks what follows it.
- `group duration_each=` is each inner step's default (a line's own
  `duration=` wins) — never the group's total; names are opt-in, only
  referenced groups need one.
- `self` works as a target too (`self.scale to=1.05`) — handy for anims
  attached to a style's `transition ... anim=`.
- Inside an `each` block, lines drop the target prefix — the `each` names
  it once for every line (next recipe). `play <anim>` runs a whole other
  anim as one step of the chain.

---

## How do I stagger buttons into a menu? (slide, settle, pop)

`each` names the wearers (every wearer of a style, tree order), its body is
a step chain every wearer copies, and `stagger_by` shifts each successive
copy:

```kdl
style MenuButton width=220, height=48 {
    transition duration=0.08                  // press in/out feel
    when held background=#FFFFFF22
}

Column menu halign=center, valign=center, spacing=12, on:show=play(appear) {
    Button newGame  .MenuButton text="New Game"
    Button settings .MenuButton text="Settings"
    Button quit     .MenuButton text="Quit"
}

anim appear duration_each=0.2 {
    each .MenuButton stagger_by=0.1 {
        translate.y from=40, to=0, easing=back_out   // settle = overshoot
        and opacity from=0,  to=1
        then scale.x to=1.05, duration=0.08, autoreverse=true   // a little pop
    }
}
```

- `from=` values are asserted and HELD from play start, so the later buttons
  never flash at their final spot while waiting out the stagger.
- `then` means "when the slide-and-fade cluster ends" — retime the slide
  and the pop follows.
- `autoreverse=true` plays the pop out and back (WPF AutoReverse).
- With `stagger_by` > 0 the each's length depends on how many wearers play
  it, so nothing may sequence AFTER the each — keep it the tail of its
  chain.
- More wiggle than `back_out`'s single overshoot? `easing=elastic_out,
  oscillations=2`. Springy words map to the WPF easings: overshoot/settle →
  `back`, wiggle/springy → `elastic`, bouncing → `bounce` (see REFERENCE
  "Easing" for the parameters).

---

## How do I play an animation on a state change?

A value that simply holds while the state holds needs no anim — write it
in the `when` (`when held scale=0.95`, see the shrink recipe). Reach for
`anim=` when the flip deserves *choreography* — keyframes, overshoot, several
properties on their own clocks. Point a `transition` rule's `anim=` at a
named anim:

```kdl partial
anim pressBounce {
    self.scale from=1.0 {
        key t=0.1,  v=0.92, easing=cubic_out
        key t=0.25, v=1.0,  easing=back_out
    }
}

style ToolButton background=$chrome {
    transition any -> held duration=0.1,  easing=cubic_out, anim=pressBounce
    transition held -> any duration=0.15, easing=cubic_out
    when held background=$accent
}
```

`self` inside a style's anim targets whichever node the style is applied to.

`anim=` runs the choreography; `duration`/`easing` on the same rule time the
state's *plain* responses (`background` here). Without them the plain
responses change **instantly** — predictable, but an instant color pop next
to a smooth scale reads as a bug, so time them whenever a rule carries an
anim. Leaving the state cancels the anim and tweens the displaced
properties back with the leaving rule's timing.

---

## How do I animate parts of a button differently on press?

A button with named internals is a widget — and a widget can carry its own scoped
`anim` and `style`, targeting its internal nodes by name. Here the icon "pops" and
the caption "shakes" on press:

```kdl
widget FxButton $icon="", $text="" {

    anim pressFx {
        icon.scale {
            key t=0,   v=1.0
            key t=0.1, v=1.25, easing=cubic_out   // pop out fast
            key t=0.3, v=1.0,  easing=back_out    // settle with overshoot
        }
        caption.translate.x {
            key t=0,    v=0
            key t=0.05, v=-3
            key t=0.10, v=3
            key t=0.15, v=-2
            key t=0.20, v=1
            key t=0.25, v=0                       // shake = decaying jitter keys
        }
    }

    style Root background=$chrome {
        transition any -> held anim=pressFx
        transition held -> any duration=0.15, easing=cubic_out
        when held background=$accent
    }

    Button signals .Root {
        Row spacing=6 {
            Image icon texture=$icon
            Label caption text=$text
        }
    }
}
```

Every instance gets the behavior:

```kdl partial
FxButton paintButton icon="res://icons/brush.png", text="Paint"
FxButton undoButton  icon="res://icons/undo.png",  text="Undo"
```

Notes:
- One `anim` holds both effects — tracks run in parallel.
- Widget-scoped anims and styles see the widget's internal names (`icon`,
  `caption`) and are invisible outside it.
- Leaving the state restores displaced properties to base with the
  leaving rule's timing; tracks that end at their base value (the shake ends
  at 0) need no undo.
- Rapid press/release is safe: entering a state cancels its exit tween
  (and vice versa), always starting from current values.
- Scaling pops from the center: motion uses the offset transform, whose
  engine pivot default is the control's center (the WP tilt feel).
- The `$icon=""` default is safe: an empty string on a resource-typed property
  means null — no load attempt, no error. A *non-empty* path that fails to load
  is still a load-time error.
- Names you invent share one convention: camelCase for nodes and anims
  (`caption`, `pressFx`), PascalCase for widgets and styles (`FxButton`, `Root`).

---

## How do I trigger an animation declaratively?

`on:show` and `on:hide` fire when the node is shown or hidden:

```kdl partial
Panel optionsPanel on:show=play(appear), on:hide=play(disappear) {
    ...
}
```

`on:hide` plays the animation first and hides the node when it finishes — the
deferred hide everyone otherwise hand-rolls wrong once. Showing during a running
hide cancels it and plays the show animation from current values (same
interruption rule as states). `play(name)` is a call value: an ident plus
parenthesized argument, naming an anim in scope.

Arrangements flip declaratively the same way — no host handler needed:

```kdl partial
Button settingsButton text="Settings", on:activate=go(settings)
Button backButton     text="Back",     on:activate=go(base)
```

---

## How do I stop an animation and put things back?

`stop_anim("name")` from code (`gogo.stop(view, "name")`, C# `StopAnim`)
ends the anim without playing it out and returns every property it
animated to what the file says without the anim — the declared value, the
`when` value holding at the time, or a finished anim's end pose the play
started from. No `finished` signal fires.

```kdl
anim pulse repeat=forever {
    dot.scale from=1, to=1.2, duration=0.4, autoreverse=true
}
style DotLook {
    when held scale=0.9
}
Button dotButton .DotLook width=40, height=40 {
    Circle dot width=24, height=24, color=accent, halign=center, valign=center
}
```

```gdscript
play_anim("pulse")     # the dot breathes 1 -> 1.2 -> 1 ...
stop_anim("pulse")     # dot.scale = 1 again: the declared value
```

Three answers to "back to what?":

- Nothing else writes the property → the **declared** value (`scale` 1).
- A `when` is holding it (the button is held: `when held scale=0.9`) →
  the **when's** value, 0.9. The anim was the top writer while it played;
  stopping hands the property back to the `when`.
- The play started from a **finished** anim's end pose (`vanish` faded the
  dot to `opacity` 0 and ended) → that pose stays: a finished anim's end
  is a standing value the author chose, not something `stop` undoes.

One property, one writer: a `when` and a `transition … anim=` may not both
drive the same property in one scope (load error G4003 — "two writers
would fight on every flip"); an anim started from code simply outranks the
`when` while it plays.

---

## How do I animate a color (a glow, a flash)?

*(FanRust runtime, 2026-09-02 — the Godot addon predates these lanes.)*

Any word a `transition` rule can move, an anim can drive — colors
included, with everything a track already has (chains, `autoreverse`,
`repeat=forever`, easings):

```kdl partial
anim warn {
    saveButton.background to=#E51400FF, duration=0.2,
        autoreverse=true, repeat=forever
}

anim fadeHint {
    hint.font_color.a to=0.3, duration=0.6      // alpha alone — the hue
}                                               // keeps following its ladder
```

While the anim plays it OWNS the lane: a `when held background=` on the
same node waits its turn, and `stop(warn, restore)` puts the pre-play
color back. Colors mix in premultiplied Oklab (one law with gradients and
transitions), so a red→green flash passes through a real midtone, not
brown, and a fade to transparent never darkens.

---

## How do I scale or rotate from a corner (pivot)?

*(FanRust runtime, 2026-09-02.)* `pivot=` places the fixed point `scale`
and `rotate` transform about — a 0..1 box point, the Gradient `from=`
spelling; unset = the center:

```kdl
Panel card width=220, height=120, background=surface, pivot=0x0 {
    // scale=1.1 grows RIGHT and DOWN — the top-left corner holds still
    transition any -> held duration=0.1
    when held scale=1.05
}

Image door source="res://door.png", pivot=0x0.5      // rotate swings on the left
                                            // edge like a hinge
```

Taps follow the picture — a scaled or rotated box takes input where it is
drawn, pivot and all.

---

## How do I give my control arrow keys?

*(FanRust runtime, 2026-09-02.)* Declare the claim in the contract — a
list line like `state_builtin`:

```kdl
control Stepper $value=0, $min=0, $max=10 {
    state_builtin focused, disabled
    keys arrows, home_end          // claimable: arrows, home_end, enter, space
    Row { Label text=live($value) }
}
```

With focus on a `Stepper`, arrows and Home/End arrive at your code-behind
as `Key` events on the instance root — step `$value` there — and never
move focus. Unclaimed keys traverse as always, and Tab ALWAYS leaves (the
way off a control, never claimable). The file says which keys the control
eats — no script to read.

---

## How do I disable a control (and style it)?

Set `disabled=true` from code or markup; style the look in the shared style:

```kdl
style ToolButton background=$chrome {
    when disabled opacity=0.4
}

Button undoButton .ToolButton text="Undo", disabled=true
```

Built-in states: `held`, `hovered`, `focused`, `focus_within`, `disabled`,
`selected`.
`hovered`, `focused`, and `disabled` work on **any** node — a button
`disabled`
also stops answering input; on anything else it's a styling flag (the greyed
Label beside its greyed Slider), with `interactive=false` as the input switch
when a node must also stop responding. `held` needs a press surface (a
button, a `ListItem` row, a slider); `selected` needs a provider (a
toggle-mode button, a Radio, a `List`).
`hovered` answers a real mouse only — a finger isn't a pointer, so touch
screens never show hover styles (the emulated pointer would otherwise strand
the look wherever the finger lifted) — and a mouse press holds hover through
a mid-drag slip off the control until release. Using a state where it
can't apply is a load-time error that lists what's available:

```
panel.gogo:4: state 'held' isn't available on Rectangle
    when held background=$accent
         ^^^^ available here: hovered, focused, focus_within, disabled, selected
```

---

## How do I size things?

Bare numbers are effective pixels (density-independent). `width`/`height`
answer "how big?" three ways — exact, natural, or flexible:

```kdl stale
Button save width=200, text="Save"   // exactly 200 epx wide
Button ok   width=hug,  text="OK"    // natural size — shrink to content
Slider vol  width=fill               // take the leftover space
Panel body width=fill(2)            // a double share of the leftover
Label name min_width=220         // at least 220 — still stretches
Rectangle hairline height=1px        // device pixels, explicitly
```

`size=WxH` (or a bare `50x90`) is the exact-size shorthand for both axes;
omit a component to leave that axis alone (`size=x72`). `min-*` is the
"at least" spelling — the control keeps stretching with its container.
Heads-up on the default: children stretch across a `Column`'s width unless
you say `hug` (or give an exact size).

An exact size smaller than the content can't hold: the real box is the
combined minimum, so content wins and the build says so
(`Column height=100 but content needs 316`). Wrap it in a `Scroll` and put
the exact size there (the next recipe), let the axis `hug`, or shrink the
content. To hug *up to a limit* — a box that follows its text until a cap,
then wraps — see "How do I make a box hug text that can wrap (a toast)?".

---

## How do I make a box hug text that can wrap (a toast)?

`width=hug` on a box holding a wrapping `Label` collapses — a wrap-label's
minimum shrinks toward its longest word. The cap is the fix: `max_width=`
pairs with `hug` and means "size to the text, wrap only past this". With
`max_width=fill` the limit is the space a `width=fill` box would get — the
parent frame minus the margins — tracked live across resizes:

```kdl
Layers {
    Panel toast width=hug, max_width=fill, halign=center, valign=top,
            margin_left=12, margin_right=12, padding=12, background=#000000FF {
        Label tipText text="Saved!", wrap=true,
            horizontal_alignment=center
    }
}
```

Short text: the plate is exactly as wide as the text, centered. Long text:
the plate stops at the screen minus 24 and the text wraps. Swapping
`tipText.text` from code re-fits the plate. On a fill-capped axis the
margins bind the measuring frame (the one place a centered axis takes
margins); a number (`max_width=336`) is the fixed-limit form. Caps bind
under free parents (`Layers`, plain `Control`, slots) and never shear
content that can't shrink — hard minimums win.

Add `min_width=` for a floor and the box hugs inside a band (D135):
`width=hug, min_width=100, max_width=336` never drops below 100 however
short the text runs, and the floor beats the cap (WPF clamp order). A
raw-register `RichTextLabel` hugs and caps too since D135 — give it
`fit_content=true` (its engine minimum counts no content otherwise).

---

## How do I make a box my code sizes (an ad strip, a seated card)?

Some boxes belong to the code-behind: the page seats them in px and the
content must never talk back — an ad strip sized to the banner, a card the
code drags. Declare that on the root with `host` (D283):

```kdl stale
Panel adPlate width=host, height=host, min_width=320, min_height=50 {
    Row width=fill, height=fill {
        Label textInfo width=fill, autowrap_mode=word_smart,
                text="Free version. Everything works, but with ads."
        Button buy text="buy ad-free", min_height=44
    }
}
```

The code sets `Position` and `Size`; the file fills whatever box it gets.
A `host` axis contributes nothing to the root's minimum — only the
declared `min_width`/`min_height` floors count (they also give the lint a
touch target) — so a child's transient measure can never inflate the box:
without `host`, one frame of the wrapping label measured at a stale width
(one character per line, a rotation re-seat) raises the root's minimum,
`Size = ...` clamps up to it, and Godot never shrinks the box back — a
320x50 strip renders over the whole screen until the next re-seat.

`host` is root-only (a child's box belongs to its parent — G7004), takes
no alignment, margin or cap (the code owns placement too), and on a
`Panel` root needs the `GogoPanelView` host (G7005). Forgetting the
declaration has a net: a free-standing component that stays bigger than
the window warns at runtime, naming this fix. `width=hug,
height=hug` with no other placement keys is the softer opt-out — natural
size until the code moves it — for boxes the code positions but content
still sizes.

---

## How do I make a column fill a phone but stop at a maximum on wide screens?

The web's centered-column shape (`width:100%; max-width:N; margin:auto`),
WPF's `MaxWidth` on a stretched panel. In gogo the cap goes on a `fill`
axis, and the side is written out (D245):

```kdl
Layers {
    Column page width=fill, max_width=380, halign=center {
        Label title text="Settings"
        Button done text="Done"
    }
}
```

On a phone narrower than 380 the column fills edge to edge, exactly like
plain `fill`. On anything wider it stops at 380 and centers — by pure
width math, no per-screen-class poses. The side (`halign=center`, or
left/right) is required: when the cap binds, the leftover space has to go
somewhere, and the file says where instead of a silent default (WPF
silently centers here — a classic surprise).

The box needs a free parent (`Layers`, plain `Control`, a slot) — it
reads its parent's space and places itself in it. Under a `Column`/`Row`/
`Panel`, wrap it in a `Layers` first (the load error says so). Margins
inset the measuring space, and rigid content wider than the cap still
wins (a cap never shears — same floor-beats-cap law as the hug cap).

---

## How do I make a box scroll when its content is too tall (a menu)?

Give the exact size to a `Scroll` — a Scroll axis is allowed to be smaller
than its content (that's its whole job); everything else grows to fit. A
`hug` axis on the Scroll fits the content and turns scrolling off on that
axis, so a menu hugs its width and pans vertically:

```kdl
Scroll halign=center, valign=center, height=100, width=hug {
    Column menu spacing=12 {
        Button text="Ok"
        Button text="hello"
        Button text="hello1"
        Button text="hello2"
        Button text="hello3"
    }
}
```

The centering moved to the Scroll — it is the 100-tall box now; the Column
inside takes the viewport and scrolls past it (touch pan, fling, springy
edges — see "How do I let a list scroll under the notch?" for insets). Both
axes `hug` would leave nothing to scroll — the build warns. For selectable
rows use a `List` instead; it scrolls itself and must not sit in a `Scroll`.

Don't know how tall the menu should be? Cap the hug instead of picking a
number — the box grows with its content, then scrolls (D138):

```kdl
Scroll halign=center, valign=center, width=hug, height=hug, max_height=300 {
    Column menu spacing=12 {
        Button text="New Game"
        Button text="Continue"
        Button text="Settings"
    }
}
```

Two more words tune the axes: `allow=vertical` pins panning to one axis
outright (a fill-width page that must not pan sideways; its content is then
exactly the Scroll's width, so text with `wrap=true` wraps on screen and one
line too long to wrap is cut at the edge on its own), and `bars=never`
hides the scrollbar while the axis keeps panning — the handheld look
(`bars=reserve` keeps the bar's space so content doesn't shift). Raw
`horizontal_scroll_mode=`/`vertical_scroll_mode=` props stay the engine
escape hatch and never mix with the gogo words on one Scroll.

---

## How do I center a control?

`halign` and `valign` — and they work the same everywhere: on a free layer
(`Layers`, any plain `Control`) and inside containers (`Column`, `Row`,
`Panel`, `Scroll`):

```kdl
Layers page {
    Panel dialog halign=center, valign=center, background=$chrome,
            corner_radius=8, padding=16 {
        Label text="Centered both ways", font_size=15
    }
}
```

```kdl
Column spacing=8 {
    Label title text="brush", font_size=15
    Button okButton halign=center, text="OK", min_width=120
}
```

`halign` takes `left/center/right`, `valign` takes `top/center/bottom` —
they only say *where*; *how big* is the `width`/`height` triad in the
previous recipe (`halign=center, width=fill` is an error — fill leaves
nothing to position). And these place the *control*; `horizontal_alignment`
in the next recipe places *text* inside its box.

---

## How do I center or wrap text?

`horizontal_alignment` / `vertical_alignment` place the text inside the
Label's own box (WPF `TextAlignment`, plus the vertical centering WPF rows
give you for free):

```kdl
Label caption text="fullscreen", font_size=12, horizontal_alignment=center
Label rowLabel text="save + share", font_size=15, min_height=48, vertical_alignment=center
```

Wrapping is `autowrap_mode=word_smart` (WPF `TextWrapping="Wrap"`). A wrapping
Label needs a width to wrap against — a `min_width`, a container that
stretches it, or `max_width=fill` on a hugging box (wrap at the screen):

```kdl
Label blurb text="A longer description that wraps onto several lines",
    font_size=13, min_width=180, wrap=true
```

Under a pure hug chain wrap-text renders single-line at its full text width
(hug = natural size, D254) — a wrap that can never bind is linted (G1004)
instead of collapsing the box to a sliver.

---

## How do I add margins?

WPF `Margin` splits into three tools, by intent:

- **Space between siblings** — `spacing` on the panel: `Column spacing=8`.
- **Breathing room around content** — `padding` on a `Panel` (or a button).
  A `Panel` with `padding` and no `background` draws nothing — the
  transparent margin wrapper:

```kdl
Panel padding=12 {
    Label text="12 epx of air on every side", font_size=15
}
```

  A `Layers` (and the `View` root) takes `padding-*` directly — free
  placement is gogo's own math, so the air folds into every child's
  placement with no wrapper at all:

```kdl
Layers padding_left=10, padding_right=10 {
    slot track width=fill, height=20, valign=center { ... }
}
```

- **Air around a free child** (under `Layers` / any plain `Control`) —
  `margin=N`, or per-edge:

```kdl
Layers body {
    Column fileOps valign=bottom, margin_left=40, margin_bottom=90 {
        ...
    }
}
```

Margins bind to the edges the alignment touches: both on a filled axis, the
near edge otherwise, none when centered. Containers refuse margins — their
`spacing`/`padding`/`gap` own that job, and the error will point you back
here.

---

## How do I put extra space between two stack children?

`spacing=` is uniform — the same air between EVERY pair. For a one-off
break (a bigger gap between groups, or "push the rest to the far edge"),
drop a `gap` in:

```kdl
Column spacing=8 {
    Label title text="Brushes"
    gap 12                       // fixed: 12 epx, on top of any spacing
    Row { ... }
}
Row {
    Label caption text="Size"
    gap fill                     // elastic: takes the leftover space
    Button close text="X"
}
```

`gap` is a structure word (lowercase, like `row`/`skip`): it takes one size
word and nothing else — no name, no props, no children. Space you need to
address (style, animate, collapse from a `when`) is a layout element — use
a real node instead. Row/Column only; in a `Flow` use `spacing=`, in a
`Grid` size the tracks, under `Layers` position with margins.

---

## How do I make a button size to its content?

You don't have to — gogo buttons do it (WPF behavior; a raw Godot Button
would ignore free children in its minimum, draw the content anyway, and leave
a sliver-sized touch target):

```kdl
Button settingsRow {
    Panel padding=6 {                       // measured air — WPF Padding
        Row spacing=6 {                     // passive content never steals the press
            Image source="res://icons/settings.png", min_width=32, min_height=32
            Label text="settings", font_size=12, vertical_alignment=center
        }
    }
}
```

The button's minimum becomes the content's (6+32+6 tall here, icon+text+air
wide) and follows it live. Rules of thumb: `min_width`/`min_height` merge via
max; an exact size (`width=50`, `50x90`) clamps its axis — content never
grows it. Air works both ways (D242): `Panel padding=N` and positive
placement margins both count into the derived minimum — the empty space you
declared is space the button reserves. Negative margins mean "stick out past
the edge on purpose" and count as zero. See REFERENCE "Button content
sizing".

---

## How do I overlay controls (layers)?

Put them in a `Layers`: children stack in declaration order — later = on
top — and each child places itself with the usual `halign`/`valign`/`width`/
`margin` words:

```kdl
Layers screen {
    Image photo source="res://photo.png", width=fill, height=fill
    Rectangle scrim color=#000000B0, width=fill, height=fill   // darkens below
    Panel toast halign=center, valign=top, margin_top=20,
            background=$chrome, corner_radius=6, padding=10 {
        Label text="Saved!", font_size=15
    }
}
```

The layer host must be a `Layers` (a plain `Control`) — a `Column`/`Row`
would lay the children out in a line instead of letting them stack.

---

## How do I build a full-screen dialog?

Root children place freely (`View` is the root keyword — no wrapper, ever),
and input surfaces are DECLARED: the plate that blocks what's behind it says
`interactive=true`, and any region that must swallow stray taps gets an
invisible catcher. Nothing blocks by accident — the root and all passive
structure are input-transparent:

```kdl
View settingsSheet visibility=gone {
    // swallow taps beside the card (nothing behind may react):
    Rectangle catcher interactive=true, color=#00000000
    Panel card interactive=true, margin_left=40, margin_right=40,
            valign=center, background=#1F1F1FFF, padding=12 {
        Column spacing=8 {
            Label text="Settings", font_size=24
            Button closeButton text="Close", min_height=44
        }
    }
}
```

Skip the catcher when what's underneath SHOULD stay live (a sidebar next to
a still-tappable list); size it with margins when only a region must be
covered. The card's `interactive=true` keeps taps on the plate itself from
falling through between its controls.

---

## How do I draw custom visuals from code (fills, meters, charts)?

Declare the node in gogo — layout, color, place in the tree — and let the
code-behind subscribe to its `Draw` signal for the pixels gogo can't say.
The declared `Rectangle` is the layout truth (rect, epx re-apply); the code
owns only the drawing on top:

```kdl
Layers root interactive=true, height=40 {     // the drag surface (a slider row)
    Rectangle trackBase color=#FFFFFF33, valign=bottom, height=8,
        margin_left=16, margin_right=16, margin_bottom=10
}
```

```csharp
_track = (ColorRect)GetGogoNode("sizeSlider/trackBase");
_track.Draw += () => _track.DrawRect(
    new Rect2(0, 0, _track.Size.X * pct, _track.Size.Y), FillColor);
_track.QueueRedraw();   // whenever pct changes
```

One writer per channel: gogo owns the rect and the base color, the handler
draws the dynamic part. (This is the intended escape for WPF templates you'd
otherwise re-create node by node — see REFERENCE "No ControlTemplates".)

---

## How do I show a picture my code renders (a live preview, the canvas)?

Declare an `Image` whose `source=` names a HOST picture — any scheme but
`res://`, spelled the way your code will look it up — and let the code
register the pixels under that name. The file owns the box (size, place,
`fit=`), the code owns the picture:

```kdl
Panel previewStrip background=#000000FF {
    Image preview 240x80, halign=center, source="preview://brush", fit=distort
}
```

```rust
// once: the id the build baked for that name, and a handle on the renderer
let key = images.get("preview://brush").expect("the page names it");
let on = ui.external_image(key);
// whenever the picture changes: point the box at the new texture
ui.set_external_image(&device, on, &texture);
```

The name in the file is the whole contract: the build bakes it as the
picture's id, so a box with no `source=` (or a different spelling) is a
picture of nothing and stays empty however much the code registers —
the strip shows its background and nothing else. Until the first
registration the box is empty too. One name per picture; the canvas
draws one `Image` per tile (`canvas://tile`) the same way.

---

## How do I pin something to a screen corner or edge?

Say *where* with `halign`/`valign`, push off the edge with margins — on a
free layer these compile to Godot anchors for you:

```kdl
Layers screen {
    Button fab halign=right, valign=bottom, margin=16, text="+"       // corner
    Panel banner valign=top, width=fill, margin_left=12, margin_right=12,
            margin_top=20 {                                           // edge, inset both sides
        Label text="pinned to the top edge", font_size=15
    }
    Column hud halign=left, valign=top, margin_left=30, margin_top=20 { ... }
}
```

Margins are **signed** — positive sits inside, negative hangs past the
edge — and they carve whatever box the axis already has: an inset of a
filled/default axis (the unnamed edge stays at the parent's edge), an
offset of the pinned edge under alignment, and a pin of a natural-size box
on a `hug` axis.

```kdl
// a bar whose bottom 30 epx (its labels) hang below the screen edge:
Layers bar width=fill, valign=bottom, height=80, margin_bottom=-30
// parked fully past the bottom (off-screen until a state lifts it):
Layers tray valign=bottom, height=50, margin_bottom=-90
// two per-edge keys with no alignment pin BOTH edges — the box spans:
Rectangle rule margin_left=20, margin_right=20
```

A per-edge key that can't bind is a load-time error (the far edge of an
aligned axis; any edge of a centered axis) — the `margin=N` shorthand stays
pure breathing room and never errors. For poses that *change* at runtime
(drawer open/closed), declare an `arrangement` (two recipes down); for
transient motion, animate `translate` (next recipe).

---

## How do I keep content out of the notch (safe area)?

Give the element `safe` margins or padding — `safe` is a length value that
reads the screen's inset for that edge, host-pushed
(`gogo.set_safe_insets(left, top, right, bottom)`, pixels) and re-applied
live on rotation or cutout change:

```kdl
Layers screen {
    Image photo width=fill, height=fill      // full-bleed: under the cutout
    Layers content margin=safe {             // carved clear of every inset
        Column menu halign=left, valign=top, margin_left=30, margin_top=20 { ... }
        Button back halign=left, valign=bottom, text="back"
    }
}
```

Opt-in, per edge: name only the edges the element actually reaches
(`margin=safe` / `padding=safe` are the all-four sugar — right for a
full-screen frame like this one; a docked panel names its edges, next
recipe). Design offsets compose structurally: the safe layer supplies the
cutout inset, `margin_top=20` stays the pure design value — no arithmetic,
no code-behind re-deriving offsets. Full-bleed layers (backdrops, canvases,
scrims) simply name no safe edges. See REFERENCE "Safe-area insets". For
lists that should scroll UNDER the cutout, the padding goes on the scroll
content — next recipe.

---

## How do I let a list scroll under the notch?

Put the `safe` padding on the scroll CONTENT, not around the Scroll.
Around it the insets frame the viewport, and scrolling rows cut off at the
safe line in mid-air — looks broken. On the content they pad the scrolling
block instead: at rest the first row sits clear of the cutout, and scrolled
rows glide under it and clip at the physical screen edge:

```kdl
// a left-docked sidebar: only left and top can ever meet a cutout
Scroll gallery allow=vertical, bars=never {
    Panel rows padding_left=safe, padding_top=safe, padding_bottom=20 {
        Column list spacing=0 { ... }
    }
}
```

Name only the edges the element actually REACHES: `safe` knows the
screen's insets, not where your panel sits, so a left-docked sidebar takes
left/top — naming the right would widen it with dead space on the far side
(user-caught, D73). Design values go on the remaining edges of the same
element (`padding_bottom=20` = scroll room after the last row). WHERE the
padding sits in the tree says WHETHER the inset scrolls.

---

## How do I make a fixed-width panel's plate reach under the cutout?

The safe-inset SPACER. A right-docked 320-wide panel whose smoke should
run to the physical screen edge is "320 + the right inset" — a sum the
language deliberately can't write. Say it structurally instead: an EMPTY
panel is exactly as wide as its padding, so a box with `padding_right=safe`
IS the inset — zero on every device without a side cutout. Give it the
same background and the two plates read as one:

```kdl
Row dock width=fill, valign=bottom, spacing=0 {
    Panel push width=fill                          // the star: docks the pair right
    Panel pane width=320, background=#222222EE { ... }
    Panel edge background=#222222EE, padding_right=safe   // the `+ inset` term
}
```

Content stays in the honest 320 box, the plate reaches the edge, and no
value was computed anywhere — the WPF star trick plus a box whose width
is the inset itself.

---

## How do I slide a panel off-screen?

Pick the leg that matches what "off-screen" means:

**It's hidden when off** (a gallery, a toast): declare the VISIBLE position in
layout, start `visibility=gone`, and let show/hide play transient motion — the
slide is a visual-only offset transform, so layout never moves and the
transform disarms itself at rest:

```kdl
anim appear duration_each=0.5, easing=cubic_out {
    panel.translate.x from=-60, to=0
    and panel.opacity from=0,   to=1
}
anim disappear duration_each=0.25, easing=cubic_out {
    panel.translate.x to=-60
    and panel.opacity to=0
}

Panel panel visibility=gone, halign=left, height=fill,
        on:show=play(appear), on:hide=play(disappear) { ... }
```

Flipping the visibility runs the pair (the hide plays first, then hides and
restores the base look). Playing the opposite anim mid-flight starts from
current values.

**It stays off-screen as a standing pose** (a parked tile, a collapsed
drawer): that's an `arrangement`, not an anim — next recipe.

---

## How do I make a two-position panel (drawer, toolbar, parked tile)?

Declare the base pose in layout, declare the other pose as an `arrangement`
with a `when` block, and time the flip with a `transition`:

```kdl
arrangement expanded

transition duration=0.3, easing=cubic_out

when expanded {
    bar.margin_bottom = 0
    tile.margin_right = 0
}

Layers chrome {
    // base pose: the bar hangs 30 below the edge, the tile parks past the corner
    Layers bar width=fill, valign=bottom, height=80, margin_bottom=-30 { ... }
    Button tile 60x60, halign=right, valign=bottom,
            margin_right=-80, margin_bottom=200, text="fullscreen"
}
```

`gogo.set_arrangement(root, "expanded")` tweens both nodes there with the
matching rule; `set_arrangement(root, "base")` tweens home (going home
mirrors the entering rule). Or flip straight from markup:
`on:activate=go(expanded)`. The active pose **re-asserts itself** on rotation,
units changes and hot reload — that's what makes an arrangement the right
home for a standing pose, where an anim's end value would be stomped by the
next re-apply. C#: `SetArrangement` / `await SetArrangementAsync("expanded")`
/ `CurrentArrangement`.

Scope rules to EXACT pairs and the other flips snap declaratively — a
switch whose option poses glide into each other while entering or leaving
Off seats instantly needs no code flag:

```kdl partial
transition option1 -> option2 duration=0.5, easing=cubic_out
transition option2 -> option1 duration=0.5, easing=cubic_out
// no rules touching base — those flips snap
```

Pose tweens run current seat -> new seat, so a mid-flight re-flip glides
from wherever the node visually is (WPF To-animation feel) — never a snap
back to the full offset. When one node needs its own pace inside the same
flip — a plate sliding slower than the tiles it backs, or a toast that must
snap to its seat while only its opacity fades — give THAT node an exact
pair rule; `duration=0` means "this node snaps":

```kdl partial
Rectangle plate 60x120 {
    transition option2 -> option1 duration=0.6, easing=cubic_out
    transition option1 -> option2 duration=0.4, easing=cubic_out
}
```

---

## How do I slide a box my code moves (a floating bar, a tab underline)?

When the code picks the spot (above the tapped photo, under the picked
tab), there is no fixed pose to name. Give the properties their own
timing, then just write them:

```kdl
transition commandBar.{margin_left, margin_top, opacity} duration=0.4, easing=expo_in_out
transition underline.margin_left duration=0.4, easing=expo_in_out

View page {
    Layers area {
        Panel commandBar overlay=true, halign=left, valign=top, width=108, height=57,
                visibility=gone { … }
    }
    Rectangle underline halign=left, valign=bottom, width=53, height=3, color=$accent
}
```

```rust
// Every write slides from where the box shows now; a write mid-slide
// turns around.
rt.set_prop(NAMES.underline, PropKey::MarginLeft, Value::VNum(tab_x));

// Show the bar floating in: land the start values at once, then write
// the end values.
rt.set_prop_instant(NAMES.command_bar, PropKey::MarginTop, Value::VNum(y - 28.0));
rt.set_prop_instant(NAMES.command_bar, PropKey::Opacity, Value::VNum(0.0));
rt.set_visible(page, NAMES.command_bar, true);
rt.set_prop(NAMES.command_bar, PropKey::MarginTop, Value::VNum(y));
rt.set_prop(NAMES.command_bar, PropKey::Opacity, Value::VNum(1.0));
```

A re-layout (rotation, a new size) that re-seats the bar should use
`set_prop_instant` so it doesn't drift. Writes to a hidden box land at
once. `visibility` itself never slides — fade `opacity`.

---

## How do I lay a view out differently in landscape?

`when screen.<class>` blocks — a second pose axis the HOST drives, not a
button. Once the app names the active size class from its resize handler
(`gogo.set_screen_class("landscapeSmall")` — five fixed classes:
portraitSmall / portraitMedium / portraitLarge / landscapeSmall /
lotsOfSpace), every view's matching block applies, instantly:

```kdl partial
when screen.landscapeSmall {
    optionsPane.halign = left     // re-declares the h axis: the portrait
    optionsPane.width = 280       // base's stretch pins drop, the pose's
    optionsPane.margin_left = 60  // own keys stand alone
    hint.opacity = 0.5
}
```

Pose keys are margins, `width`/`height`, `halign`/`valign` and any style
prop. Setting an alignment RE-DECLARES that axis from the pose's keys —
that's how a stretch-with-margins portrait panel becomes a docked
fixed-width landscape one. (Arrangements can't re-anchor — they tween.)

It composes with arrangements: a dialog holding its `expanded` pose on a
rotated screen keeps BOTH — the arrangement wins any key both set, the
screen class keeps the rest, and closing the pose lands back on the
landscape seat, not on the portrait base. No transition rules here: a
rotation is the OS snapping the window, and the pose snaps with it.

When the layout question is "is the screen SIDEWAYS?" — not "how much
room is there?" — key the ORIENTATION axis instead
(`gogo.set_screen_orientation`, two fixed values):

```kdl partial
when screen.landscape {          // wider than tall, any size
    bodyStack.visible = false
    wideRow.visible = true
}
```

Don't spell "landscape" by enumerating the landscape-ish classes
(`landscapeSmall` + `lotsOfSpace`): `lotsOfSpace` also covers big
PORTRAIT screens, so an upright tablet would wear the sideways layout —
and the duplicated blocks drift apart over time. One target never takes a
class pose AND an orientation pose (G3005 — both can hold at once);
when you mean the intersection, compose them, orientation first:
`when screen.landscape screen.lotsOfSpace { ... }`.

When a pose only makes sense with an arrangement open ON a certain screen
— say the expanded panel needs a second column just in landscape — compose
the conditions in one block, arrangement first (the second slot takes a
class or an orientation):

```kdl partial
when expanded screen.landscapeSmall {
    optionsPane.margin_left = 300   // only while BOTH hold; wins any key
    hint.opacity = 0.25             // the single-axis blocks also set
}
```

Either half dropping releases the block back to whichever single-axis pose
still holds.

When landscape needs a node in a DIFFERENT PARENT — a stack that becomes a
second column, a panel that joins the app bar — declare a `seat` where it
goes and move it from the when (a pose can't reparent, but the seat
response can):

```kdl
Row {
    Column mainColumn {
        ...recents...
        Column savedStack { ... }   // rests here, below the recents
    }
    seat sideSeat                   // hugs to zero until occupied
}

when screen.landscapeSmall {
    savedStack.seat = sideSeat      // beside the recents, instantly
}
```

Dropping the condition returns the stack to its declared spot. When a
MORE specific condition needs the node back where the class alone moved
it FROM — the panel leaves the app bar again while a dialog is open —
`home` is the reserved return target:

```kdl partial
when expanded screen.landscapeSmall {
    savedStack.seat = home          // back at its declared spot while BOTH hold
}
```

See the seat card for the full rules.

A pose may also RESIZE any child in place — a `width`/`height`/
`min_width`/`min_height` written in a `when` block behaves exactly as if
the file had declared it, for as long as the condition applies (D249).
On a flow child a posed `width`/`height` takes the child out of the
leftover-space split (the star space moves to whoever still fills — how
an app bar's back column narrows to make room); on a free child it moves
the box AND the minimum it reports, so a pose can shrink a node below
its declared size:

```kdl partial
when screen.landscapeSmall {
    backColumn.width = 60           // pinned; the slot column takes the star
}
when screen.portraitSmall {
    sideRail.width = 50             // below the declared 60 — and it shows 50
}
```

A declared `min_width`/`min_height` floor still clamps every posed size
(declare `min_width=30` and a posed `width=20` renders 30 — the floor
rule), and a posed `min_*` replaces the declared floor for as long as
its block applies. Margins and alignment on container children stay load
errors — the parent seats its children; only the SIZE is poseable.

---

## How do I make one button expand AND collapse a panel?

`toggle(pose)` — the disclosure button. The press goes to the pose when
base is active and comes home when the pose is: `go(active == pose ? base
: pose)` in one word. Style the button THROUGH the pose (the `when` block
reaches it like any node — numbers, booleans, colors and the motion words),
so its look can never disagree with the panel:

```kdl
arrangement expanded

transition duration=0.2, easing=cubic_out

when expanded {
    bar.height = 120
    moreButton.rotate = 180   // the chevron points back up
}

Layers stage width=240, height=200 {
    Rectangle bar color=accent, width=200, height=50, halign=center,
        valign=top
    Button moreButton text="v", min_width=88, halign=center,
        valign=bottom, on:activate=toggle(expanded)
}
```

Keep the button **momentary** — don't add `toggle=true`. A toggle-MODE
button holds its own selected state next to the pose's, and the two drift
apart the moment anything else flips the pose (a second button, code,
`go(base)` anywhere); gogo lints the combination. The pose IS the state.

---

## How do I slide content open smoothly (animated expand)?

`reveal_y=` — the node's layout SPACE as a 0..1 fraction. `visible` is a
bool and bools snap; a fraction is a float, and floats tween under the
`transition` rule — so the label below slides open and the panel border
grows WITH it, no magic heights to maintain:

```kdl
arrangement expanded

transition duration=0.2, easing=cubic_out

when expanded {
    hello.reveal_y = 1     // grant the label its space — the slide
    hello.opacity = 1      // and fade it in on the way
}

Panel border_color=red, border_thickness=1, padding=5, corner_radius=9 {
    Column {
        Button more text="v", 50x50, on:activate=toggle(expanded)
        Label hello text="Hello, Gogo", reveal_y=0, opacity=0
    }
}
```

Collapsed, the label earns no height (clipped past its top edge, taps
included) but stays alive; expanding tweens the fraction to 1 and every
sibling reflows each frame. The resting value on the node's line is what
DECLARES the channel — a `when` or anim driving `hello.reveal_y` without it
is a load-time error. `reveal_x=` slides horizontally the same way. It
needs a container parent (`Column`/`Row`/`Grid`/`Panel`/...): under
`Layers`/free parents nothing reflows around minimums — overlay reveals
there are `opacity`/`translate` territory.

---

## How do I make something fill the leftover space?

`width=fill`. In a container:

```kdl
Row spacing=8 {
    Label label text="Size", font_size=15, valign=center
    Slider sizeSlider width=fill, min=1, max=100   // WPF "*"
}
```

On a `Layers` free layer, the same words — the filler fills, a margin skips
past the fixed part:

```kdl
Layers row min_height=48 {
    Image icon source="res://icon.png", size=32x32, valign=center, margin_left=8
    Label title width=fill, height=fill, margin_left=48,
        font_size=15, vertical_alignment=center
}
```

Filling siblings split the space by weight — `width=fill(2)` takes twice a
plain `fill`'s share (WPF `2*`). Weights divide the *leftover*, not the
parent: fixed and `hug` siblings take their sizes first.

---

## How do I line up a form (label / control / value columns)?

Stacks can't share tracks across rows — that's `Grid`. Declare the columns
once, then each `row { }` is one line of the form; placement IS the
structure, no coordinates:

```kdl
Grid cols=[hug, fill, 60] {        // label hugs, control stretches, value 60
    row { Label a text="Size";     Slider s;  Label sv }
    row { Label b text="Hardness"; Slider h;  Label hv }
    row height=fill {
        Layers preview col_span=3      // one cell spanning all three tracks
    }
}
```

Moving a form row is cut/pasting one block. An empty cell is `skip`; a row
may end early (trailing tracks stay empty). A cell spanning DOWN
(`row_span=2`) owns the covered tracks — write `skip` under it, the error
will walk you through. The keypad shape shows everything at once:

```kdl
Grid cols=[hug, hug, hug, hug] {
    row { Button k7; Button k8; Button k9; Button plus row_span=2 }
    row { Button k4; Button k5; Button k6; skip }
    row { Button k1; Button k2; Button k3; Button enter row_span=2 }
    row { Button k0 col_span=2; Button dot; skip }
}
```

For a page shell, place by NAME instead — the areas form (one string per
row, `.` for holes, a REQUIRED `: height` riding each row):

```kdl
Grid cols=[hug, fill],
     areas=["header header : 60",
            "nav    body   : fill",
            "footer footer : 40"] {
    Panel header { ... }              // binds to the area named "header"
    Column nav { ... }
    Rectangle footer color=$chrome
}
```

One Grid uses one form, never both. WPF habits that route with an error:
whole-grid overlay spans (`RowSpan=99` scrims) are a `Layers` sibling here;
`Grid.Row="2"` coordinates don't exist; `auto` is spelled `hug`; weights
live on tracks (`cols=[fill(2), fill]`), not on cells.

---

## How do I make a tile mosaic (photo gallery, launcher grid)?

`Tiles` — equal columns, cells flow in order and wrap. Nothing is addressed:
add a tile, everything after it re-packs. Give each tile's Image
`aspect=natural` and the bitmap's ratio sizes the tile
(square photos make square tiles); headers sit in the same list with
`full_row=true`, and a featured tile takes `span=2`:

```kdl
Scroll {
    Tiles gallery cols=2 {
        Label t1 text="top picks", full_row=true, font_size=24
        Panel p1 { Image source="res://a.jpg", aspect=natural }
        Panel featured span=2 {
            Image source="res://wide.jpg", aspect=natural
        }
        Panel p2 { Image source="res://b.jpg", aspect=natural }
        Label t2 text="more", full_row=true, font_size=24
        Panel p3 { Image source="res://c.jpg", aspect=natural }
    }
}
```

A span that can't fit the row's remainder wraps whole to the next row (the
tail stays empty — first-fit, no reshuffling). `cols=` is live: set it from
code (`tiles.Set("cols", 3)`) and the mosaic re-packs — an icon_size toggle
is that one write. Wider air between tiles is padding inside the tile
(wrap the content in a `Panel padding_left=6, padding_top=6`), so the gap
scales with nothing and the hit rect stays the visible tile. A caption Label
on a tile needs `clip_text=true` (plus `width=fill` to keep it stretched):
without it, a long name's minimum makes the tile OVERLAP its neighbors —
cells never shrink content, and Tiles warns when one outgrows its cell.

**A names-only view of the same mosaic.** Give each tile a second look — its
name in a bordered `Panel` — hidden under pictures, and let one arrangement
hide the picture parts, show the names and pack the cells at their own
width. The list is written once; nothing is duplicated:

```kdl
arrangement names
style PictureOnly { when names visibility=gone }
widget NameChip $text="" {
    Panel box border_color=#FFFFFFFF, border_thickness=1.5, padding_left=10,
            padding_right=10, min_height=40, visibility=gone {
        when names visibility=visible
        Label text=$text, wrap=true, vertical_alignment=center   // a long name folds, never clips
    }
}
when names { gallery.cells = natural }      // chips pack at their own width

Tiles gallery cols=2, cells=lattice {
    Label t1 .PictureOnly text="top picks", full_row=true, font_size=24
    Panel p1 { Image .PictureOnly source="res://a.jpg", aspect=natural; NameChip text="Autumn" }
    Panel p2 { Image .PictureOnly source="res://b.jpg", aspect=natural; NameChip text="Beach" }
}
```

Code flips it with `rt.go(Arrangement::Names)` and back with
`rt.go(Arrangement::Base)`. The effects catalog (`effect_select.gogo`) is
this recipe, reached as the Icon Size button's fourth stop; its chips also
carry the entry's picture as a 40x40 thumbnail under the name's start.

---

## How do I open an image-heavy page fast (lazy images)?

By default every `texture=` decodes before the first frame that shows it —
fine for a few icons, a frozen first frame for a catalog of 70 photos (a
fifth of a second on an old phone). `loading=lazy` moves the decode to a
background thread: the page shows at once, never waiting for a picture, and
each image fills in the moment its pixels are ready, in declaration order —
the top of the page first:

```kdl
Scroll {
    Tiles catalog cols=2 {
        Panel p1 { Image a source="res://a.jpg", loading=lazy,
            aspect=natural }
        Panel p2 { Image b source="res://b.jpg", loading=lazy,
            aspect=natural }
    }
}
```

- The file's existence is still checked at build time — a typo'd path fails
  the build loudly, lazy or not. Only the decode moves off the main thread.
- A lazy image draws EMPTY until its pixels land, usually a few frames
  later. Its size is read from the file header before the page first
  shows, so `aspect=natural` tiles have their final size from the start —
  nothing moves when the pixels arrive. Code that needs the pixels
  themselves (a screenshot, a reveal gate) waits for `gogo.lazy_pending()`
  to read 0 first.
- Reserve it for the image-heavy pages. `eager` stays the default: a
  toolbar icon that pops in a frame late looks broken; a catalog thumbnail
  that streams in behind the page slide looks fast.
- The biggest win pairs it with a KEPT page: build the view once, hide it
  instead of freeing on close, and every reopen is instant with the scroll
  position physically still set — lazy loading makes that first build
  cheap, keeping the instance makes it the only one.

---

## How do I show many small pictures from one file (an atlas)?

A sticker picker shows 54 animals. Written as 54 `Image` lines over 54
PNG files, the page decodes 54 files — 25 MB of pixels for thumbnails
the size of a fingertip — and every one stays in memory. Artists solve
this with a **sheet**: one picture with all 54 thumbnails laid out in
a grid. An `atlas` line tells gogo how to cut that picture, and each
Image shows one piece of it:

```kdl partial
// 480x720 picture, 80x80 squares: 6 across, 9 down, cells 0 to 53.
atlas animals source="res://assets/stickers/animals.jpg", cell=80x80

View v {
    Flow sheet {
        Image a 57x57, source=animals[0], loading=lazy
        Image b 57x57, source=animals[1], loading=lazy
        Image c 57x57, source=animals[2], loading=lazy
    }
}
```

That one line does three things for you:

- **One decode.** The sheet decodes once; every cell draws from it.
- **Numbered pieces.** `animals[12]` is row 2, first column. gogo counts
  the cells from the picture's real size, so a sheet redrawn with one
  more row needs no edit.
- **Build-time checks.** A cell past the end, or squares that do not
  tile the picture, fail the build:

```text
ui/stickers.gogo:5: error G2016: `animals` has 54 cells — `animals[0]` to `animals[53]`
```

When code picks the cell (a picker filled from a table), write the
tiles once and let the page's code set each one:
`rt.set_prop(tile, PropKey::Source, Value::VImage(atlas::ANIMALS[i]))`.
Every atlas is a typed constant in the build's `atlas` module.

- Pieces of different sizes? Name them instead of numbering them:
  `atlas toolbar source="…" { slice play x=0, y=0, size=48x48 }`, shown
  as `source=toolbar.play`.
- A smoothed, scaled slice reads one pixel past its edge: leave space
  between slices (a contact sheet's own margins are enough).

---

## How do I stretch a picture without blurring its corners (9-slice)?

A speech-bubble picture is 64x32 with rounded corners. Stretch it to
fit a long caption and the corners stretch too — fat, blurry, lopsided.
A **9-slice** keeps the corners at their own size: `edge=` says how many
picture pixels on each side must not stretch.

```kdl partial
atlas bubble source="res://assets/ui/bubble.png", edge=10

View v {
    Image caption width=fill, height=80, source=bubble
}
```

gogo cuts the picture into nine parts along those bands:

- **Corners** draw at their own size, one epx per picture pixel.
- **Edges** stretch one way — the top and bottom edges get wider, the
  left and right edges get taller.
- **The middle** stretches both ways.

Sides can differ — the bubble's tail sits at the bottom:
`edge=10, edge_bottom=18` (the later word wins, like `padding`). If
the Image gets smaller than two corners, all bands shrink together
instead of overlapping. A 9-slice always fills its Image, so `fit=` on
it is an error (G2018). In an atlas of named slices, `edge=` goes on
each `slice` line that stretches.

---

## How do I use a TexturePacker or Aseprite export?

Point `from=` at the exported JSON ("JSON (Hash)" or "JSON (Array)" —
Free Texture Packer and ShoeBox write the same). The JSON names its own
picture; every frame becomes a named slice:

```kdl partial
atlas ui from="res://assets/ui/ui.json" {
    slice panel edge=16     // make the imported `panel` a 9-slice
}

View v {
    Image play 32x32, source=ui.buttons_play   // "buttons/play.png"
    Image back 200x120, source=ui.panel
}
```

- **Names**: the file name without its extension, lowercased, every
  other character turned into `_` — `buttons/Play-Hover.png` becomes
  `ui.buttons_play_hover`; a leading digit gets a `_` (`01.png` →
  `ui._01`).
- **Trimmed sprites** keep their original size for layout, so turning
  trimming on in the packer never moves your page. They cannot take
  `edge=` — turn trimming off for a sprite you 9-slice.
- **Rotated sprites** are refused: turn rotation off in the packer.
- **A stale export** — the picture's size no longer matching the JSON —
  fails the build with "export the atlas again".

---

## How do I paint a gradient (fade a color)?

`Gradient` with a `stops=` list — colors in order, evenly spaced unless you
pin them with `@position`:

```kdl
Gradient veil  fill=radial, stops=[accent, accent/0]      // fade to clear
Gradient sky   angle=90, stops=[#0F1B2B, #4A6FA5]         // aimed linear (90 = to right)
Gradient wheel fill=conic, stops=[red, yellow, green, blue, red]
Gradient flash fill=radial,
    stops=[#FFFFFF6B@0, #FFFFFF6B@0.94, #FFFFFF6B/0@1]    // hold, then fade
```

- Positions are all-or-none, never decrease (equal = a hard step), and
  under `repeat=mirror` stop at 0.5. `/opacity` is allowed on any entry.
- Fade out with `C/0` (first example): the RGB under alpha 0 steers the
  blend even though it never shows — `accent/0` fades clean, `#00000000`
  fades through dark gray. Bare `transparent` is refused in `stops=`
  because it's #00000000 without saying so.
- A **solid color is a `Rectangle`**, not a one-stop gradient — the build
  says so (G2003) and offers the rewrite. The reverse trap: two stacked
  `Rectangle`s do NOT make a fade — there is no interpolation between
  siblings; a fade is one `Gradient` with two stops (the first example).
- Stops stay live: `flash.stop_1_color.a from=0, to=0.42` animates a stop
  like any property. Numbered from 1 in declaration order — `stop_1` is
  the first stop in `stops=[...]`, not an array index.

---

## How do I draw a circle or a ring?

`Circle` — size the box, get the disc; the radius derives from the short
side. A ring is a transparent fill with a stroked rim:

```kdl
Circle dot 24x24, color=$accent                     // a filled disc
Circle ring width=50, height=50,                    // the classic icon ring
    border_color=$accent, border_thickness=1.5
```

Like every gogo panel, a bare `Circle` draws nothing until `color=` or a
border says so. All three properties are anim/when-drivable
(`when hovered color=#FF0000FF`).

---

## How do I make an on/off switch (toggle)?

`Toggle` is gogo's switch — drawn by gogo (there is no native Godot
switch), so it restyles like any control:

```kdl
Toggle mute value=1, on:change=OnMuteChange
```

One number is the truth: `value` runs 0..1, the thumb follows it, and
`selected` flips as it crosses the middle — live while a finger drags
(state is presentation). Taps
glide to the other end; drags settle on the side they land — `change(bool)`
fires on settle, and only when the settled side differs from where the
gesture began (wiggling across the middle is zero changes). Code-behind
reads/writes the `selected` bool; `change(bool)` auto-wires as
`On<Name>Change`.

```kdl stale
style BigSwitch for=Toggle, slot.track.corner_radius=4 {
    when selected slot.track.background=green
}
Toggle wifi .BigSwitch
```

For an independent checkbox look use `Check`; for one-of-N use `choice`
with `Radio`s (below).

---

## How do I make radio buttons (one-of-N)?

Wrap `Radio`s in a `choice`. The choice is structure (erased at build — the
radios parent flat where the choice sits), and every Radio inside joins one
exclusive group, whatever layout nests between:

```kdl
choice size value=medium {
    Radio small text="Small"
    Radio medium text="Medium"
    Radio large text="Large"
}
```

The same set in two columns:

```kdl
choice size value=medium {
    Row {
        Column { Radio small text="Small";  Radio medium text="Medium" }
        Column { Radio large text="Large" }
    }
}
```

- `value=` pre-presses one Radio by name; omit it for none-selected.
- A named choice reports changes by convention:
  `choice paintMode ...` → `OnPaintModeChange(radio_name)` on the host —
  or explicitly, `on:change=Handler` / `on:change=emit(HostSignal)` (the
  radio's name is passed as the argument). The event lives on the GROUP —
  items refuse `on:change` (a Radio's rows expose `when selected`).
- Radios need names — the name IS the reported value. An independent
  toggle (no group) is `Check`:

```kdl
Check newStroke text="new stroke", on:change=OnNewStrokeChange
```

---

## How do I make a radio that doesn't look like a checkbox?

Give the `Radio` a body. A bodied Radio draws NONE of the native radio
visuals — the body is the whole look (the same blank overlay canvas as
Button content), while exclusivity, `value=` seating and the change
report still come from the `choice`. `selected` is how the body shows the
selection:

```kdl
choice paintMode value=color {
    Radio color 52x50 {
        Circle colorRing 35x35, halign=center, valign=top,
            border_color=#FFFFFFFF, border_thickness=1.5
        Image 32x32, halign=center, valign=top,
            source="res://assets/icons/color.png"
        Label text="color", font_size=11, valign=top, margin_top=36,
            width=fill, horizontal_alignment=center
        when selected colorRing.color=$accent
    }
    Radio rainbow 52x50 { ... }
}
```

- The body replaces ALL native visuals, Button chrome included (D144):
  the plates come blank — no style needed for a plateless radio, and the
  focus ring stays so keyboard focus indicates. `background=`/`when`
  responses recolor the blank slate when you DO want a plate.
- `text=` on a bodied Radio errors — letters live in the body's own
  Labels.
- Re-tapping the current pick keeps it selected and does NOT re-report:
  changed means CHANGED. A programmatic `button_pressed = true` reports
  like a tap (one report per genuine transition); `set_pressed_no_signal`
  seats silently and the choice never sees it — but the `when selected`
  restyle then arrives a sweep-beat late, with a warning; seat silently
  through `gogo.set_selected` instead for an instant restyle.
- An independent toggle takes the same body spelling: `Check name { ... }`
  is the bodied Radio's twin — same blank canvas and `selected` responses,
  checkbox reader role, no group, `change(bool)`/`on:change=` wiring
  unchanged.

---

## How do I share one radio look across many radios?

Factor the body into a **host widget** (D276): `for=Radio` announces that
the widget fills a Radio's body, and a `when selected` at WIDGET scope —
beside the body root, not inside it — responds on the hosting Radio:

```kdl
widget OptionLook for=Radio, $label="" {
    Row width=fill {
        Circle dot 10x10, color=$accent, opacity=0
        Label text=$label
    }
    when selected dot.opacity=1     // the HOSTING Radio's selected
}
choice saveLocation {
    Radio btnDefault { OptionLook label="Default album" }
    Radio btnCustom  { OptionLook label="Custom location" }
}
```

- The `when` must sit at widget scope. Written INSIDE the body (say, inside
  the `Row`) it would answer the Row's own never-raised `selected` — that
  spelling is a build error in widget bodies (the dead-when trap: the app
  builds, the dots just never fill).
- A `for=` widget only instantiates as the direct body of its host type
  (`Radio` here; `Check` and `Button` work the same way).
- The host can still add its own `when selected` inline; for the same state
  the host's response replaces the widget's (nearer scope wins).

---

## How do I hide something without collapsing the layout?

`visibility=gone` releases the space — siblings reflow over it. The
keeps-space form is `visibility=space_only` (WPF `Hidden`): invisible,
hit-inert, unfocusable, but the node still holds its layout slot, so
toggling it never moves a neighbor:

```kdl
Layers strip width=hug, height=hug {
    Label note text="saved", halign=left, valign=top
    // away right now — the strip still hugs this seat, so the note
    // doesn't jump when the banner returns
    Rectangle banner color=#E51400FF, width=240, height=40,
        visibility=space_only
}
```

`space_only` holds space under FREE parents (Layers / plain Control /
slot), where gogo owns the measure; an engine container skips hidden
children in its own sorting and refuses the spelling (v1). Inside a
`Column`/`Row`, hold the seat the old way — zero the opacity and take the
node off the input map so the ghost doesn't catch taps:

```kdl
Column spacing=8 {
    Button ghostButton text="soon", opacity=0, interactive=false
    Button saveButton  text="save"
}
```

---

## How do I split my UI into files?

One view per file; shared themes/styles/widgets in their own files, imported with
`uses` (paths relative to the current file):

```kdl partial
uses "styles/theme.gogo"
uses "widgets/cards.gogo"
```

---

## How do I translate my UI (and switch language at runtime)?

Suffix any displayed string with a translation key — the literal stays the
English text, the key names the term:

```kdl
Label tip text="new stroke"/newStroke
Button ok   text="keep changes"/keepChanges
```

Feed locale tables from the host and flip — every keyed string on every live
view updates in place, instantly:

```gdscript
gogo.set_translations("de", {"newStroke": "neuer Strich", "keepChanges": "Änderungen behalten"})
gogo.set_locale("de")   # ...and back: gogo.set_locale("")
```

- No table / missing key → the literal shows, so an untranslated app is just
  the English one. Your `.gogo` files ARE the English strings file.
- Reuse a key by repeating it with the SAME literal; a different literal
  under a known key is a build error (G8001) — that's the net that catches
  "edited one copy of the string, forgot the other".
- Dynamic strings from code use the same rule:
  `gogo.translate("PaintingSaved", "Your painting has been saved")`.
- Make sure nothing ships untranslated (Rust app): turn on
  `CompileOptions::require_keys` and every displayed string with no key
  fails the build (G8002). Mark a string that must stay English with `/_`
  (`text="Leon Shaw"/_`), and a whole dev or release-notes file with an
  `allow untranslated` line at its top.

---

## How do I debug a tap that misses?

The model first: **interactive controls answer input by default, everything
else is transparent, and `interactive=true/false` overrides per control**
(REFERENCE "Input"). Then check, in order:

1. **Is the surface declared?** A drag row, a blocking plate, a catcher —
   anything passive that should respond needs `interactive=true`. In debug
   builds a press nobody claims logs a one-time hint naming the topmost node
   under the finger and this exact fix.
2. **Look at the map.** `gogo.set_input_overlay(root, true)` (or the editor
   preview's "Input" toggle) tints every region that currently claims input —
   a dead surface shows no tint.
3. **Is something eating it?** An `interactive=true` catcher (or another
   button) drawn ABOVE the target claims the tap first — later (higher)
   layers hit-test first.
4. **Is the hit rect as big as the visuals?** Canvas items don't clip —
   content can DRAW outside its control's rect while taps land by rect. gogo
   buttons derive their minimum from their content (REFERENCE "Button
   content sizing"), but a plain container can still be smaller than what it
   shows: check the control's rect, not the pixels. `gogo.explain(node)`
   prints both the rect and the input provenance.

---

## How do I find out what's wrong?

Save the file. Errors report the file, the line, the source with a caret, and
usually the fix:

```
toolbar.gogo:9: unknown widget type 'Buttn'
    Buttn optionsButton .ToolButton text="Brush"
    ^^^^^ did you mean 'Button'?
```

Widget types, properties, style names, states, easings, and `$values` are all known
sets — most typos come back with a suggestion. While a file has an error, the last
good version of the UI stays on screen.

---

# Coming from XAML

The migration questions — the same answers as the recipes above, entered
from the WPF/XAML instinct you already have. Each subsection is one
question; the dock's table of contents lists them as their own group.

## Where does my wrapping text get its width?

WPF's measure pass hands *available size* down to every element — even a
size-to-content one. A left-aligned `CheckBox` in a 640-wide panel is
measured with 640 available, its `TextBlock TextWrapping="Wrap"` wraps
there, and the checkbox still hugs the wrapped result. Godot has no such
pass: a control's minimum is answered with **no context**, so wrap-text
only wraps where a .gogo width **binds** it (D254). Porting XAML text means
deciding, per shape, where that bound comes from:

| XAML shape | gogo spelling |
|---|---|
| `<TextBlock TextWrapping="Wrap"/>` stretching in a panel | `Label autowrap_mode=word_smart, width=fill` |
| Left-aligned content control holding wrap text (`<CheckBox HorizontalAlignment="Left">`) | bind the chain: `width=fill` on the control, the row AND the label — a hug chain has no width to hand down, and a `fill` label inside one gets the leftover of a content-sized row: its own one-char raw minimum (linted, G1004) |
| `MaxWidth` hug — size to text, wrap at the screen (a toast) | `width=hug, max_width=fill` on the box (a number caps at a fixed width) |
| Wrap text inside a floored button (`MinWidth` guarantees the line) | `min_width` on the BUTTON + `width=fill` down to the label — the floor is the width the wrap binds at, and G1004 credits it |
| `TextTrimming` / no wrapping | plain `Label` — no `autowrap_mode`, no width decision needed |

The trade the left-aligned row makes: with the chain bound `fill`, the
control's hit area spans the row instead of hugging the text. If the hug
tap target matters, keep the control hugging and cap the label instead
(`max_width=` a number) — a *fixed* cap needs no parent frame, so it works
under a hug chain where `max_width=fill` has nothing to read.

If a page ever *grows every frame* after a port (a code-behind "fit to my
panel" write feeding back into layout), the runaway-growth tripwire prints
`gogo: runaway layout growth` naming the root — see G1004's sibling notes
in `docs/errors/` and the `overlay=true` cut.
