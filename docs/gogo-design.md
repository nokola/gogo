# gogo-ui crate design (G1, 2026-08-29)

> This file was written while gogo lived inside the FanRust application
> (2026-08 to 2026-11), and it keeps that record: dated decisions, ids
> such as GPAR61 and D97 from FanRust's own logs, and comparisons with the
> earlier Godot addon ("the addon", "shipped"). "The first host" is
> FanRust: where a rule came from one of its pages, the text says what
> the page needed. The crates named here are the ones in this repository:
> `gogo-ui` (`crates/gogo-ui`), `gogo-text` and `gogo-wgpu`. To use them,
> read REFERENCE.md, "Host API".

The `gogo-ui` crate is pure Rust: no wgpu, no winit, no OS. Input: `.gogo`
text (at build time) + touches + the current theme/arrangement. Output: a
draw list. Same language and semantics as the Godot gogo (`gogo-spec.md` is
the grammar); what changes is the machinery underneath.

Godot gogo → Rust gogo, one line each:

| Godot | Rust |
|---|---|
| parse `.gogo` at load | parse at build time (`build_pages` in `build.rs`); the same compiler also runs at run time (`compile`, hot reload) |
| one engine node per widget | one plain `Copy` struct per node in a flat `Vec` |
| engine layout, min-size passes, ratchets | one pure `layout()` pass, nothing written back |
| engine draws each node | one draw list, one batched GPU pass |
| signals + auto-wired `On<Name>` host methods | build-time `enum` of handler names per page, app matches on it |

## 1. Arena

```rust
pub struct Page {
    pub nodes: Vec<Node>,          // index 0 = root; a node's children are one contiguous run
    pub strings: Vec<String>,      // every name and text, by StrId; id 0 = the empty string
    pub props: Vec<Prop>,          // every node's (key, value) pairs; a node owns a range
    pub whens: Vec<When>,          // `when <state>` responses; a node owns a range
    pub handlers: Vec<OnHandler>,  // on:<event> = action; a node owns a range
    pub theme: Vec<ThemeEntry>,    // $values per variant
    pub arrangements: Vec<StrId>,
    pub transitions: Vec<Transition>,
    pub anims: Vec<AnimTrack>,
    // … and the smaller tables: values, kind_defaults, prop_transitions,
    // anim_heads, choices, repeats, behaviors, slices (page.rs)
}

pub struct Node {
    pub kind: Kind,                // View, Panel, Column, Row, Flow, Layers, Scroll, Grid, Tiles,
                                   // Label, Button, Image, Rectangle, Circle, Arc, Gradient,
                                   // Slider, Input, Check, Toggle, Radio, Choice, List, ListItem,
                                   // Repeat, Gap, GridRow, Skip, Slot, Seat, Empty, Custom
    pub name: StrId,               // NO_STR (id 0) = unnamed
    pub parent: NodeId,            // NO_NODE = the root
    pub children: (u32, u32),      // first child, count; contiguous; a SUBTREE is not a slice (children-first order) — Page::is_under
    pub placement: Placement,      // halign, valign, margins, padding, width, height, min_*, max_*
    pub props: (u32, u32),         // first, count in Page::props (style merge done at build time)
    pub whens: (u32, u32),         // first, count in Page::whens
    pub handlers: (u32, u32),      // first, count in Page::handlers
    pub initial_states: u8,        // the selected / disabled bits the file starts with
    // … type_name, widget, subtree_end, gap, auto, choice, focusable, line
}
```

- `NodeId`, `StrId`, `KeyId`, `ArrangementId` and `ImageKey` are newtypes
  over `u32` (`page.rs`, `id_newtype!`, 2026-09-05): each indexes `Vec<T>`
  / `[T]` directly (`page.nodes[node]`, `rects[node]`,
  `page.strings[text]`, `image_sizes[image]`), so a node id handed where a
  string id belongs is a rustc error; the raw number is private to the
  `gogo-ui` crate. An image is `ImageKey`, one id space for the whole build
  (`IMAGES` is dense by it): a draw and the renderer pass the id, never
  the path, and a host picture the pages never named interns into the
  same `Images` table (D13). Props are `(PropKey, Value)`
  pairs in one page-wide `Vec<Prop>`; a node owns a range. `Value` is an enum: `VBool`,
  `VNum`, `VColor`, `VColorPair`, `VText { text, key }`, `VImage(ImageKey)`,
  `VRef { name, opacity }`, `VIdent(StrId)`, `VSize`, `VCall`, `VList`, `VStop`, `VItem`.
- Styles (`.Style`) and `uses` are merged at build time — the runtime never
  sees a style. `$theme` refs stay refs: variants switch at runtime.
- A `repeat`'s template is a typed slot: the build emits `<Repeat>Slot`
  (`TEMPLATE`, `at(slot_root)` over `NodeId::rebase`), so a code-behind
  over a pooled row binds its widgets by field; a widget placed only inside
  a template still gets its `<Widget>Names` (D14).
- A hostless widget may carry `when screen.<class> { node.prop = value }`
  blocks posing its OWN body — one copy per instance, ranked as a default
  under any page pose on the same instance (D14). Any other state word at
  widget scope still needs `for=`.
- A `behavior slider` reads a live `min=` / `max=` / `step=` write on its
  root over the contract's baked numbers (a row reused for another
  setting takes that setting's range); `Runtime::clear_prop` takes a
  code write back.
- Translation keys (`text="x"/key`) stay keys, one id space for the whole
  build (`KEYS`, `enum Key` — `/ok` in two files is one key). `BuildOptions::
  runtime_keys` joins keys code looks up by name (a host's own table of
  terms) after the pages; a page's English wins where both name a key. The
  runtime's `Locale` is read at layout and draw time through one function
  (`locale::display`, one index per read, nothing allocated), so a language
  switch is one relayout and a label is measured in the language it is
  drawn; a key the table lacks shows its English. Tables come from the host:
  it builds a `Locale` (`Locale::build`) and hands it to `Runtime::set_locale`.
- A `repeat` holds one hidden template. Its rows are clones of it,
  appended to `nodes` the first time they are needed
  (`Runtime::set_items(page, repeat, &items)` for a keyed repeat,
  `Runtime::set_count` for a counted one); a row no longer shown keeps
  its nodes and the next row reuses them. This is the only way the arena
  changes at run time: it grows, and no node is ever removed.
- Static form: `build.rs` emits the same structs as `&'static` slices per
  page; boot copies them into the `Vec`s (memcpy-sized). Hot reload parses
  text into the same `Page`. One parser, one `Page` type.
- Strings are owned (`Vec<String>`): `Page::from_static` copies a built
  page's strings in, and a page compiled at run time owns its own.

## 2. State overlay

`when` responses, arrangements, screen classes and transitions are the
runtime part. Per node the active values are:

```
base props → single poses (arrangement / screen class / orientation,
             later line wins) → composed pairs → focus_within → focused
           → hovered → selected → held → disabled   (spec priority order)
```

`Runtime` (per page) holds: active arrangement, per-node state bits, running
tweens (`Vec<Tween { node, key, from, to, elapsed, duration, easing }>`).
`resolve(page, runtime) -> Resolved` builds the current look from scratch;
a frame takes `Runtime::resolve_retained(page)`, which keeps the last look
and works out again only the nodes a change reaches. `Resolved` holds each
node's placement and the values that differ from the file (`overrides`) —
the layout and draw passes ask it (`Resolved::prop(page, node, key)`),
never `Page` props directly. Tweens retarget from the current value (spec:
mid-flight re-flip).

Anims (`anim <name>`) run on their own list (`Runtime::players`); what
they write lands on top of the `when` responses.

## 3. Layout pass

```rust
pub fn layout(page: &Page, res: &Resolved, viewport: Rect, text: &dyn TextMeasure) -> Vec<Rect>
```

- Output is one `Rect` per node (epx units), indexed by `NodeId`. Pure:
  same inputs, same output; no field of `Page` or `Runtime` is written.
- Measure and arrange, two questions with two names (2026-08-30, after
  two layout bugs in the first host showed one function answering
  both):
  - `size_at(node, width)` — MEASURE: the box the node wants given the
    width its parent can offer (wrapped text and Flow rows need it).
    Hugging parents, cross axes and "the height at the width I got" ask
    this; it is memoized per pass, so a parent asking twice costs once.
  - `main_min(node, axis)` — how much a child gets when its parent was
    told its size and is splitting it. In plain words: A parent is either TOLD its size (`width=300`, `width=fill`, the
    screen) or ASKS its children (`width=hug`, or an aligned axis with no
    width word).
    - Parent that asks its children ("how wide do you need to be?"): a
      `fill` child answers with its content, like any other child. There is
      nothing to share yet, so "fill" has nothing to say here.
    - Parent that was told its size ("I'm 300 wide, split it"): fixed
      children take their size, `fill` children share what's left. A fill
      child never ends up narrower than its content or its `min_width`,
      whichever is more; when that does not fit, the children run past
      the parent's edge.
    A `wrap=true` label with `width=fill` has no width of its own
    (Godot's autowrap minimum) — the text bends to the box.
  - `star_split` — ARRANGE along an axis, the star rule: fixed slots take
    their size, fills share what is left by weight, a fill whose share is
    under its floor takes the floor and the rest re-share (Godot
    BoxContainer, WPF star). So `fill(2)` / `fill(3)` is 2:5 of the room
    whatever the children hold, and two `width=fill` columns around a
    fixed cluster are equal (in a 360-wide row: 100 | 160 | 100 epx).
  Then `place` lays each child into its slot top-down. Every kind's
  measure/place is a `match` on `Kind` in `layout.rs`; no per-kind trait
  objects. Two roads give the same rects: `layout` lays the whole page
  out from scratch, and the frame road (`Runtime::layout_retained`,
  `layout::LayoutCache`) keeps the last answer and lays out again only
  under the boxes a change can reach. There is no ratchet to guard
  (Godot's min-size-only model was what produced D255/D283).
- Semantics per kind copy the spec: `fill`/`hug`/exact, min floors
  clamp posed values, View children place freely, Panel is the styled box
  with padding + border stacking, Grid/Tiles per the D82/D94 sections,
  Scroll clips and offsets by the runtime scroll position.
- A hug box is as wide as its content; margins are gaps around it and
  never stretch it: `width=hug, margin=8` sits 8 in from the side
  `halign` picks (the near edge when no `halign` is written). Two named
  edges span the box — it reaches between them — only where no size word
  is written on that axis (GPAR39, 2026-08-31).
- Text measure asks a `TextMeasure` trait (`fn measure(&self, text, font_size,
  max_width, font) -> (f32, f32)`) supplied by the caller; the crate ships a
  fixed-advance fake for tests (`FixedAdvance`) and `crates/gogo-text` plugs
  `cosmic-text` in (`EpxMeasure` over a `TextEngine`), which caches each
  measure by (text, font, size, width) so a warm frame does no shaping.
- Cost target: ~500 nodes in tens of µs (est.). `Runtime::dirty` is set by
  anything that changes what layout or draw would give (a resolved value,
  an item set, a code write) and cleared by `frame_retained`; a host that
  skips the frame while it is clear makes an idle page cost nothing.
- Fractions stay in the rects; the renderer rounds each rect's origin to
  a device pixel at draw time (decided 2026-08-30, Q7) — crisp glyph runs
  under a `height=205.7` image, no per-node compounding in the layout.
- `reveal_x` / `reveal_y` (a fraction 0..1) on any node, any parent
  (2026-09-05, user; the addon demanded a container parent): under a
  container the node takes only its shown fraction of space and the
  siblings close up (a disclosure collapse); a hug axis shows the
  fraction of its own measure; a `fill` axis shows the fraction of what
  it was granted, from its alignment edge — a Rectangle `reveal_x=0.3`
  in a 100-wide Layers is 30 wide at the left, the slider fill bar with
  no wrapper Row. Not a clip mask: the box itself shrinks, so a Label
  re-lays out in the smaller box.
- Debug: the rects are plain numbers a host can print — `kept.rects()[node]`
  for a `node` from `Page::find("name")` — the numbers behind a layout diff.

## 4. Draw list

```rust
// Rgba below stands for Color_Straight_Srgb_4xU8_RGBA, the one color type.
pub enum Draw {
    Quad { rect: Rect, color: Rgba, corner: f32, border: f32, border_color: Rgba },
    Image { rect: Rect, image: ImageKey, tint: Rgba, fit: Fit, window: Option<[f32; 4]>, sampling: Sampling },
    Gradient { rect: Rect, kind: GradientKind, angle: f32, corner: f32, period: (f32, f32), stops: u32 },
    GradientStop { pos: f32, color: Rgba },                  // the `stops` records right after their Gradient
    Circle { rect: Rect, color: Rgba, border: f32, border_color: Rgba },
    Text { rect: Rect, text: StrId, key: KeyId, font_size: f32, color: Rgba, halign: TextAlign, valign: TextAlign, wrap: bool, font: StrId },
    ClipPush(Rect), ClipPop,
    // … QuadCorners, Arc, TextStretch, Caret, Selection, SelHandles,
    // WedgePush/WedgePop, RotatePush/RotatePop, SwingPush/SwingPop
}
pub fn draw(page: &Page, f: Frame<'_>) -> Vec<Draw>   // a Frame = one frame's look (`Resolved`) and its rects
```

- A walk down the tree = paint order (parents before children,
  siblings in file order; `Layers` overlays later). It is not the
  arena's index order: a node's children sit after its later siblings.
  Opacity multiplies down the walk; a `disabled` control dims its
  subtree to 0.3 unless the file says otherwise.
- Default look (2026-08-30): the native kinds draw the same numbers on
  every engine, so a bare `Button text="OK"` is one picture everywhere
  (`draw.rs` consts, spec "Default look"): Button plate gray 0.1 at 0.6,
  no plate under a native mark; Check = 20-epx box, 1.5 white outline,
  accent fill + white 10-epx square when selected; Radio = 20-epx ring,
  accent 10-epx dot; Toggle = 44×22 pill, `gray` off / `accent` on, white
  18-epx knob; mark then 8 epx then the text; Slider = 4-epx `#5A5A5A`
  track, white 0.6 fill, white 16-epx knob; a colorless Rectangle is
  white (Godot's ColorRect), a colorless Circle draws nothing; scroll
  grabbers `#737373` in a 4-epx lane on either overflowing axis. A bare
  Slider strip is 32 epx tall and a bare Input 36 (`layout.rs`
  `SLIDER_HEIGHT` / `INPUT_HEIGHT` — Rust's own numbers; the addon's
  controls took their heights from their control templates, GPAR26). A
  bare Toggle's box is at least 44 epx tall — a finger target — with the
  22-epx pill centered in it (`TOGGLE_TOUCH_EPX`, GPAR92).
- Recorded draw/layout differences from the addon, kept on purpose:
  gradients render per-pixel — the addon baked a 128×128 texture, which
  smeared hard color steps about 5 px on a 720-wide screen (GPAR44); and
  `spacing=` keeps fractional epx — the addon truncated to whole pixels,
  so gaps drifted at non-integer scale factors (GPAR46).
- THE COLOR-MIXING LAW (user call 2026-09-02): gogo mixes AUTHORED
  colors in premultiplied Oklab — gradient stops on the GPU (ui.wgsl),
  `transition` tweens and `anim` color lanes on the CPU
  (`gogo_ui::color::mix_rgba`, ONE implementation the shader mirrors).
  Premultiplying keeps fades to transparent clean (no dark fringe from a
  transparent color's rgb); Oklab keeps lightness even across hues
  (red→green passes through a real midtone, not muddy brown). CSS
  `color-mix()` made the same default. DELIBERATE DIVERGENCE from the
  addon's per-byte sRGB lerp — a host that samples a gradient in code
  (a gradient editor) calls the same function, so a seeded stop sits ON
  the rendered line. Scope: mixes between two authored colors only; the
  shader's 1-px antialias coverage ramps are not color design and stay
  as-is.
- Palette: the bare color words are one table for every engine,
  `page::PALETTE` = the Godot resolver's (CSS-named: `red` tomato,
  `blue`/`accent` cornflower, `gray` slate …). A host's theme file
  overrides what it needs (`theme { red = #E51400 }`); no host's own
  set is the language's.
- `crates/gogo-wgpu` turns the list into instanced draws of "UI quads" (a
  quad type per command: solid/rounded/image/glyph/circle/gradient/arc, one
  shader that branches on the type), one draw per run of quads that share
  a clip and a texture; clip = scissor. `gogo-ui` never sees a device
  pixel: the renderer multiplies every rect by `epx` as it builds the quads.
- A `Draw::Text` carries the string's id, the font and the size; the
  renderer shapes it through `crates/gogo-text` (`cosmic-text`), whose
  `TextEngine` keeps the shaped runs in a cache. A glyph is rasterized
  into the atlas the first time it is shaped; a host that wants none of
  that in its first frame calls `TextEngine::warm` at start.

## 5. Touch and events

```rust
pub fn hit(page: &Page, f: Frame<'_>, x: f32, y: f32, any: bool) -> Option<NodeId>   // last drawn wins
impl Input { pub fn touch(&mut self, page: &Page, rt: &mut Runtime, f: Frame<'_>, measure: &dyn TextMeasure, t: Touch) -> Vec<Event> }
```

- `hit` walks the paint order backwards (top-most painted first), skipping
  non-interactive and hidden nodes, honoring clip rects of `Scroll`
  ancestors. Hit-testing is Godot gogo's D103 ONE input model (adopted
  2026-08-30, user): input-taking KINDS answer by default, everything
  passive is transparent, always — `interactive=true` declares a surface,
  `false` hides one, and a wired handler alone never claims input (a
  full-screen Panel with just `on:show=` must not eat taps; an earlier
  handlers-claim-input clause was removed).
- `touch` drives the spec's input vocabulary: `held` on press, slide-off
  clears, slide-back re-arms, `activate` on a completed tap; Slider `edit`
  per changed value and `change` on release; Toggle `selected` flips at the
  midpoint, `change` on settle; Scroll pans and flings (the code-built pan
  strip of the Godot port becomes the only Scroll). Returned `Event { node,
  kind: Press | Release | Activate | Change(Value) | Edit(Value) |
  Pointer | Key | Signal, action: Option<Action> }`: `action` is what the
  file's `on:` line names, `None` = the node's own name is the handler.
  `on:show` and `on:hide` play anims and send no event. The model has
  only pointers: a mouse is one pointer — the host feeds it as one fixed
  finger id (`MOUSE_ID`), so every event fires for mouse and touch alike.
- `on:pointer=name` (renamed from `on:touch` 2026-08-30, user — the stack
  is touch-first but the reader's understanding is not; WinRT and the web
  both call the unified mouse/touch/pen stream "pointer") is the raw
  register for code-drawn controls (GradientEdit, the color wheel): every
  phase as `EventKind::Pointer { phase, x, y }` in the node's own epx
  space, and the node keeps the pointer — no `held`, no scroll steal, the
  interpreted vocabulary never fires there. Lints make the contract
  visible: `press`/`release`/`activate` wire only on press surfaces and
  `pointer` never does (G0072 — the shadowing conflict cannot be
  written), and `on:pointer` without `interactive=true` on the same node
  is a dead wire (G0073, the D103 rule: the wire alone claims nothing).
- `tap_pulse=true` on a press surface (decided 2026-09-05, user: the
  word stays, the motion is a TURNSTILE): a completed tap swings the box
  like a door about its left edge — out to 25° in 0.05 s (quad out), back
  flat in 0.12 s (back out 0.5; the shipped `Fx.TapPulse` timings on an
  angle instead of a uniform scale). Drawn in 3D (user call 2026-09-06:
  WP7's turnstile has perspective): the angle is the node's `swing`
  motion channel, the draw walk wraps the node in a `SwingPush` and the
  renderer projects it like Silverlight's PlaneProjection — a camera on
  the box's center, 57° field of view, as far in front as makes the box
  fill it flat (`swing_focal`), so the far side of the door shrinks
  toward the middle the same way on a tile and on a page. A swing is a
  transform only while it runs (the D10 disarm law), a tap mid-swing
  restarts it. Built into `Runtime::tap_pulse` — no host code, every
  host gets it; an anim can drive the same lane (`x.swing from=0 to=90`).
- `Handler` is a per-page `enum` emitted by `build.rs` from the file's
  `on:` values and the auto-wiring convention (`Button save` → `Save`).
  The app turns an event's handler name into one (`Handler::from_name`)
  and matches on it; a variant the `match` leaves out and a handler in
  the app with no node are both compile errors — the Godot load-time
  G-codes become rustc errors.
- `Key` is the build-wide `enum` of translation keys, emitted the same way
  (`Key::PickColorHint`); `Runtime::set_key` takes it and nothing else, so a
  key no page declares does not compile. `set_text` is the other write — a
  literal, never translated.
- No accessibility tree (G10 LATER). The arena has everything one needs
  (kinds, names, rects, text) — it is a walk, not a redesign.

## 6. Diagnostics

The Godot G-codes (G2003 one-stop gradient, G8001 translation drift,
G9008 dotted refs, duplicate prop, …) become `build.rs` errors with the
same codes and messages, pointing at file:line — a bad `.gogo` fails
`cargo build`. A compile at run time returns the same `Diagnostic` list
(`Compiled::diags`; `reload::recompile` for a hot reload) and the host
shows it: `examples/window` prints it.

## 7. Language changes (decided 2026-08-29, breaking is fine)

Removed — engine-born, no reason left:
- `width=host` / `height=host`, host-class rules (G7002/G7005). Existed
  because Godot ignored script minimums or wiped children (the min-size
  ratchet: a transient minimum baked into the offsets never shrank back).
  A pure layout has no ratchet to guard.
- (Reversed 2026-08-29, user call:) `overlay=true` and the `signals` flag
  STAY — `overlay` is layout semantics (a box that sits on its parent
  without sizing it: touch-slop plates, badges, the held tip), not an
  engine fence; `signals` names which node inside a widget body the
  instance's `on:` wiring binds to (the Slider inside LabeledSlider).
- The raw register: raw class names (`TextureRect`, `Control`), raw signals
  (`on:mouse_entered`), engine enum values (`autowrap_mode=word_smart`). One
  vocabulary, all gogo words. Added 2026-08-29: the gogo event `press`
  (finger-down on a press surface; `activate` stays the completed tap) —
  the undo/redo buttons' `on:button_down` became `on:press`, same feel.
  Added 2026-08-30 (user): `release` — the end of a press gesture, fired
  exactly once however it ends (lift inside or out, cancel, a Scroll
  steal, disable/hide under the finger). The pair is `press`/`release`,
  not `down`/`up`: the D181 discussion ("press vs pressed") only
  demanded that state and event never share a spelling
  (`held` vs `press`), and press/release are the words as spoken.
  The ported spelling for Godot's raw `Control` (a box that claims NO
  minimum from its children): `Layers` with `overlay=true` on each child
  that must not ask for room (GPAR66, decided 2026-08-30: clip and
  measure stay separate).
- Retired-spelling redirects (`Border`→`Panel`, `pressed`→`held`, …).
  Unknown word = build error with a suggestion, no alias table.
- (Reversed 2026-08-29, user call:) `interactive=` STAYS beside `disabled=`
  — they are two switches: `interactive=true` claims input for a passive
  node (the catcher plate under a dialog), `false` makes a control
  hit-invisible without restyling; `disabled` greys, blocks and cascades.
- `$name` vs bare `name` in theme blocks. One form: `$name`.

Changed:
- Names become code: `build.rs` emits a struct per page (`NAMES.save_button`)
  and the `Handler` enum. No node lookup by string; typos are rustc errors.
  A widget instance is a nested entry (`NAMES.slider_size.track`,
  `NAMES.slider_size.root`) typed by one `<Widget>Names` struct per widget
  for the whole build, so a widget's code-behind takes `&SliderPhoneNames`
  on any page. Each `<Widget>Names` also has `bind(page, "instance")` for a
  page compiled at run time — a pin places the widget once in a host page it
  compiles itself; no wrapper page lives in the build for that (D115). The
  page's `arrangement` and `anim` names are enums too
  (`Runtime::go(Arrangement::Wide)`, `Runtime::play(page, Anim::Appear, …)`,
  `Runtime::arrangement_is(…)`); the `*_id` forms serve runtime-compiled
  pages (2026-09-05, D11).
- Code scrolls a row into view through one door, `Runtime::scroll_into_view(scroll,
  item, rects, margin)`: the least move that puts the item inside the
  viewport, nothing when it already is (D116). Pages no longer spell their
  own edge rules.
- Root: `View` (screen, free placement) and `Panel` (component) stay; the
  host-class rules go.
- `Scroll`: one kind, finger pan built in.
- Input routing: innermost interactive node gets the touch. Past the
  16-epx deadzone the drag's dominant direction decides once: a Slider
  keeps a sideways drag, a Toggle keeps one it already started, otherwise
  the innermost Scroll/List that scrolls that way and has room takes the
  gesture and the press is cancelled; nothing scrolls that way = the
  child keeps the finger (REFERENCE "Scroll"). No filter words.
- Assets checked at build: `Image source="…"`, fonts — a
  missing file is a build error. A `source=` is `Value::VImage(ImageKey)`,
  never text: a path is never translated, and the build's `IMAGES` table
  holds every path a page names (bytes for a `res://` asset, `None` for a
  picture the host shows, `preview://brush`).
- Units: epx only; device px never appear in a `.gogo`.

Code-behind values (decided 2026-08-29; NOT built — as of 2026-11 the
compiler has no `export` word, and code writes a value with
`Runtime::set_prop`):
`export $barWidth = 480` at page scope; reads are plain `$barWidth`; the
build emits a typed setter per exported value; no wrapper is needed
for it (a `$ref` is dereferenced at resolve, every frame). A code-owned AXIS
no longer exists — code hands the layout a value, the layout stays the one
writer. Example (toolbar): portrait `appBar width=fill`; `when
screen.landscape { appBar.width = $barWidth; appBar.halign = center }`
with `export $barWidth` computed by code from min(w, h).

A control's value (decided 2026-11-10): inside a `control` body a bare
`$value` follows the control's value on every property
(`Label text=$value`, `Rectangle opacity=$value`), also when it is
passed along to a widget or a control placed in the body, and
`round($value, step)` is written by itself, with no `live()` around it.
`live()` is retired (G0081 names the bare spelling). Until that day `live($value)` followed on `text=`
only and did nothing anywhere else, and a bare `text=$value` built
clean and kept the number the control started with. Why no wrapper:
`$value` is the one `$name` that changes while the page runs, and the
compiler can tell it by its name; a read that does not follow has no
use (it is the number written on the instance line, stale after the
first drag); and `by $value in $min..$max` already followed with no
word around it. A property that follows costs one row in a list that is
visited only when that control's value is written
(`Page::value_follows`). A `when` response is one fixed value, so
reading `$value` there is refused (G0083).

Actions and anims (decided 2026-11-11). A file wires three actions by
itself, with no code in the host: `go(x)`, `toggle(x)` and `play(x)`; a
tap on `Button on:activate=play(pulse)` plays the anim. `on:show` and
`on:hide` are hooks with one job, `play(…)`: a node shows and hides
inside the runtime, where no event leaves for a host, so any other
action there is refused (G0084). An anim line on a size, a margin or a
padding moves the box, as a `transition` on it does (one list of words,
two doors); the box is laid out again on every frame of the play, and
`scale`, `translate` and `reveal` are the words that need no layout. A
rule borrowed for the way back gives its timing and never its `anim=`.
`each .Style` runs its wearers in file order. A written `from=` starts
its step there, also on a later step, and two steps that pin one
property are refused only when they run at the same time. `offset=`
shifts any step that starts something. Every word on an anim line is
known or refused (G0066), and a `transition` rule that could never time
or play anything is refused (G0034, G4003, G4004).

Who wins (decided 2026-11-11). Between `when`s of one rank a node's own
beats one its style carries, in whatever file the style sits, and
between two styles the one worn later on the node's line wins: the
order a reader sees on that line, and the rule plain props follow. The
compiler stores each node's `when`s in the winning order and the runtime
applies them as stored; it used to collect and sort them on every
resolve, by line numbers that came from different files. A theme default
written later replaces an earlier one, the page's over an included
file's. The page's text color is `theme { font_color = … }`: `color` in
that block (decided 2026-08-30) did nothing and is refused, so text
color has one word everywhere. A style on a `widget` line is refused:
the line is not a node, and the two places that are (the first node of
the body, the instance line) already say both things. `theme { accent =
… }` reaches the marks gogo draws itself. `Button padding=0` is 0: the
12/6 default is given at build to a text-only Button that wrote no
padding. A `disabled` or `selected` written from code sets the state.

One reader (decided 2026-11-11). A placement word is read by one
function, `Placement::write`, wherever it is written: a node's line at
build, and a view-level `when`, a code write and an anim line while the
page runs. The runtime used to carry its own, smaller copy, in which
`min_size` was missing, `max_width = fill` was a cap of 0, `fill(n)` was
no size, `safe` was 0 and a number written over a `safe` side kept the
inset. The compiler turns what the reader cannot read into the build
error for that word; a code write that cannot be read keeps the reader's
fallback. A code write of `max_length` and of a List's `select` is read
where the file's word is read: the last write first, the file after. A
List's selection follows a written `select` at the next `tick`, the
frame door that holds the page: `off` holds nothing, `one` the first
selected row. A seat written from code is checked where its name is
looked up, since `set_prop` holds no page: a debug build stops with a
message, a release build leaves the node at home.

Lists and items (decided 2026-11-11). An item's field may feed any
property; the build cannot know an item's types, so `set_items` checks
them, in the pass that already visits every bound field, and refuses the
whole call. The compiler and `set_items` read one table of property
types (`PropKey::is_bool`, `PropKey::closed_words`). A row follows its
key: the same key keeps its row and is bound again when its data
changed. The matching reads each row once when the order is kept and
sorts once when it is not, and the duplicate-key check sorts once: 1000
unchanged rows took 1.26 s in a debug build and take under a
millisecond. A row with no handler of its own reports its List's. A
List holds its bar's lane as a Scroll does, from one function. A repeat
nobody could fill (no name, or inside another repeat's row) and an
`empty { }` nothing could hide are build errors.

Kept: `arrangement` (user's call — a `state` unification was proposed and
declined), snake_case keys, `#RRGGBBAA`, `text="x"/key`, `when` /
`transition` / `anim`, widgets + `$params`, `uses`, styles, Grid/Tiles,
screen classes, `visibility=gone|space_only` (`hidden` until 2026-08-31).

Accepted Godot forms (round 3, so the copied files build): a node line may
break right after its name when the next line opens with a size or a
`key=` member; `res://` paths (`res://` = the folder the host names in
`BuildOptions::res_root`, assets checked and embedded by the build);
`slot`/`seat`/`control`/`automatic=` from the
control-customization spec; `choice` erased into its parent;
`layout <Kind>` inside a List; `color.a` as an opacity-like key
(`modulate.a` is refused, G0075: write `opacity`). Godot's render facts
the layout copies: font px = `int(epx × size)`,
face height = the file's ascent and descent each scaled and rounded UP to
whole px (FreeType's `FT_Size_Metrics`: 24 px Roboto = 23 + 6 = 29, not
round(28.13) = 28 — one px per list row otherwise; read from the font at
load, `gogo_text::FaceMetrics`), line pitch = face height + the line gap
(1 epx as whole device px, `gogo_text::TextPx`; Godot's 3 px at every
scale was the rule until 0.3.0), a both-axes-
sized Image centers in a stack, an auto scroll bar keeps a 4-epx lane on
each overflowing axis, Input plate = (0.1, 0.1, 0.1) at 0.6. (The color
behind a page is the host's: gogo clears nothing.)

Start states (2026-08-30): `selected=true`, `disabled=true` and a
`choice`'s `value=<name>` are build-time state bits (`Node.initial_states`),
the Runtime starts from them; `selected=` on a Radio inside a `choice` is
G0068 (one keyword per fact: the group's `value=` picks), an unknown
`value=` name is G0069, `wrap=true` on a hug-width label with no
`max_width` is the G1004 warning (never wraps, D254). A property word the
compiler accepts and no pass reads is a test failure unless
`page::UNREAD_BY_DESIGN` says why (`every_prop_key_is_read`) — three
silent drops in one round earned the guard.

Refuse what cannot be honored (E4, 2026-08-31): where the file says
something the runtime used to drop, the build fails naming the rule —
one hex form (`#RRGGBB`/`#RRGGBBAA`; `#F00` is G0005 with the long
spelling in hand), `/opacity` and `@pos` inside 0..1 (G0007/G0008),
malformed numbers (`1.2.3`, `.5` — G0009, never a silent 0), a list only
on `cols=`/`areas=`/`stops=`/`border_dash=`/`dash=`/`window=` and `@pos`
only in `stops=` (G0040), a number key given a word (`opacity=red` —
G0058), the Gradient rules (`stops=` required, linear|radial|conic,
`angle=` linear-only, non-empty color list, `@` all-or-none and never
decreasing — G2005; one stop = G2003 with the `Rectangle` rewrite; bare
`transparent` in stops = G2006), a Radio outside `choice` (G0069), one
arrow and one endpoint per side of a `transition` and at most two `when`
conditions (G0034/G0038), `repeat`/`slot` clauses on their own lines in
their fixed order and typed (G9002–G9004, G5107; `by` optional on
`repeat` — the `capacity=N` form keys nothing, GP27),
duplicate `$params` (G0018), no `.` in a `$name` declaration (G9008), no
`/key` on `uses` or asset paths (G0023), `Check true` / bare `interactive`
typo nets (G0046; `base`/`any` as a node name is the same code —
reserved everywhere, E5 2026-08-31), `state`/`state_builtin`/`signal` as NAME LISTS that
live in a `control` body (G0076/G0077), a `theme dark { }` block inside a
node (G3006), and `empty { }` holding one child (G9007, "wrap them in a
Column or Layers"). Honored instead of refused: `transition base -> X` on
a node (`base` = the rest state of a built-in flip) and going home on a
view-level `base -> X` rule (mirrored), GPAR9.

`/o` replaces the alpha (E6, 2026-08-31, GPAR31): the suffix written at
the slot is the alpha the slot gets, whatever the value carried — a theme
value with its own alpha, a variant, a `$a = $b/o` hop, a widget param, a
pasted 8-digit hex (`#FFFFFF6B/0`). Rust applied the INNER suffix on a
chain (the theme's number drew, under a clean build); `runtime::deref`
now takes the first `/` it meets, the slot's. Refusing a second `/` was
built and taken out the same day: the radial flash gradients fade a
translucent disc with a `/0` stop, and refusing made the RGB get written
twice — change the hue, forget the edge stop. A code write
(`Runtime::set_prop`) replaces the whole value; a `/o` on a `$value`
read parses in neither engine today — when it does, the same rule holds
and a debug warning names the code-written alpha it replaces.

Changed 2026-08-31 (`capacity=` + the P2 GPAR round):

- `max=` on a repeat is RETIRED (G0035 points at the new word):
  `capacity=` is a reservation the rows grow past, never a ceiling — a
  max that grows would lie, a max that refused would truncate rows on
  screen (user design 2026-08-31). Both repeat forms (counted and keyed)
  clone ONE hidden template into row SLOTS at first touch; freed slots
  re-bind in place, so the arena stops growing at the high-water row
  count and a steady `set_items` allocates NOTHING (the slot reuse is
  pinned in `crates/gogo-ui/tests/godot_repeat_list.rs`). Slots take
  per-slot names (`marks/2/knob`) and per-slot copies of the template's
  anims.
- Input takes input (GPAR112): tap-to-focus, `Input::key`, `edit`
  streams changed values, Enter/blur commit once per genuine change,
  `Draw::Caret` shaped by the renderer. The host feeds the keys
  (`Input::key`) and an IME's text (`Input::ime_replace`);
  `examples/window` wires the mouse only.
- The finger meets the picture: hit-testing follows motion (GPAR50),
  a caught glide keeps the pull (GPAR5), a Toggle release always
  settles (GPAR8), Slider `edit` fires per changed value (GPAR6).
- Timing: a watched child/sibling response is timed by the DECLARING
  scope (GPAR10/83), node pose-pair rules time their own share of a
  flip (GPAR72), tweens seed from the renderer's resting look
  (GPAR71), `show()` gates on a real flip (GPAR53), instance-line
  `when`/`transition` blocks land on the instance root (GPAR68), the
  one-line top-level `when` reads its props (GPAR70).
- Fences: seats validate (G1015/G1016, with three recorded Rust flips —
  sized sockets, free hosts/homes, E2 cross-vocabulary ranking),
  `play(nope)` is G4004, G2004 invisible pigment, G3005 both-vocabulary
  poses, G5105/G3002 automatic-slot fences, theme-entry shape fences +
  G6005 circular palette chains, and every unknown-name error now
  carries a did-you-mean (GPAR87).
- Layout: a capped hug measures naturally then binds (GPAR76,
  `size_final` split from room-measure), a fill wrap label claims its
  longest word + the G1004 leftover lint (GPAR77), `areas=` errors point
  at their row string's line (GPAR99).

Changed 2026-08-31 (P1 GPAR round — GPAR4/12/30/61/66/69/108):

- A glide is stopped only by a touch that lands ON the gliding Scroll or
  its content — chrome drawn over it presses while the list keeps
  rolling underneath (D130, GPAR4).
- Translations are the host's to ship and to pick: it builds one
  `Locale` per language (the first host embeds its tables and reads the
  phone's system language) and hands the chosen one to
  `Runtime::set_locale` (GPAR12).
- `font_color=` and `$` theme values are color positions: `$dim = gray`
  is the COLOR gray, `$scrim = black/0.5` keeps its alpha (GPAR30).
- The G-codes are renumbered to the published registry —
  `diag::REGISTRY` + `RETIRED` + `SHARED_WITH_ADDON`, held by a
  both-ways pin; unknown property = G2001, wrong kind stays G2002
  (GPAR61; teaching pages still owed).
- A widget cycle is a named build error (G5003), never a compiler stack
  overflow (GPAR69). Raw `Control` ports as `Layers` + `overlay=true`
  (GPAR66, the §7 dialect line).
- The keyed `repeat … by $item.<field>` (no `max=`) is the data path: a
  hidden template, `Runtime::set_items` reconciling by key with the
  shipped refusal codes, `$item.<field>` bound per clone,
  `Runtime::item_key` naming a tapped row (GPAR108).

Changed 2026-08-31 (GPAR37/GP8/GP20/GP32 close-out):

- A written size or cap is a hard edge for the FINGER too: `hit` honors
  every clipping ancestor (`draw::clips` — one predicate for draw and
  hit), so a button's clipped half takes no taps (GPAR37).
- `Runtime::take_finished()`: anims that end on their own reach the host;
  `stop()` never reports (GP20).
- Widget/control bodies take scoped `style` blocks (they shadow page
  styles and die with the body; instance-line `style=` never sees them);
  a `for=` widget's `transition` rules time the HOST's flips; a
  `control`'s endpoint-less `transition` is its default feel; a control's
  own anims drive `slot.<name>.<motion word>` lanes (GP32).

Changed 2026-08-31 (areas/for= port round — GPAR74/81/90/99, GP39 closed):
- Grid `areas=` is ported: expanded at compile into the same `row { }`
  structure the row form builds (`Compiler::grid_areas`), with the full
  Godot fence battery (G1002 structure, G1003 areas, G1010 dead-space
  lint) and the Tiles fences. Layout reads `row_span=`/`row_height=` and
  spreads spanned-cell deficits (the shipped `_base_sizes`).
- `for=` on styles is checked: first on the line, a control kind, and the
  wearer must BE that kind; `extends=` chains fold parent props/whens
  under the child's with cycle/unknown/`for=`-matrix errors.
- The `for=` widget-host contract (D276) is in: hosts are Button/Radio/
  Check, the five contract breaks refuse, widget-line props bake onto the
  body root, and a host's own inline `when` REPLACES the widget's
  response per state (the shipped `merge_defs` overwrite).
- choice fences: Check inside a choice, an unnamed Radio, `text=` on a
  bodied Radio/Check, `on:change` on a Radio, `widget <built-in name>`.
- Word-valued props are CLOSED sets (G0057: the visibility triad, fit,
  allow, bars, select, touch/keyboard_target, gradient fill) and bool
  keys type-check (G0078). `%` is refused at compile.
- `visibility=space_only` is the keep-the-slot word (was the invented
  `hidden`). DEVIATION KEPT: no free-parent fence — this layout is pure
  and holds the slot under any parent; Godot's fence is an engine limit.
- Overlay contradictions refuse: `overlay=true` with `space_only`, and an
  overlay `fill` axis under a hug parent no non-overlay child sizes.

Changed 2026-08-31 (E8 — user decisions; the table is §8):
- An unknown property is an ERROR (G2001; was G0042 until GPAR61), never a warning, and a real
  word on the wrong kind is G2002 — every spelling passes one table
  (`page::kind_row`): node lines, styles, theme kind defaults, `when`
  responses aimed at the node or at a child, code controls. The
  `Kind::Custom` exemption is gone.
- `color=` on a box that shows text is refused: text is `font_color=`,
  the plate is `background=` (GPAR15). The draw fallback that read
  `color=` for text is gone with it.
- Any box has a plate (GPAR36): `background=` / `corner_radius=` /
  `border_*` draw on a Label, Image, Layers, stack or code control —
  behind the content, covering the padding. Godot refused these off
  Panels; one sentence beats a list of exceptions.
- `theme font=` / `theme font_size=` / `theme font_color=` inside a block
  are the subtree's text defaults and INHERIT (`Resolved::inherited`) —
  the shipped addon's theme propagation. Found by the table: the page
  roots' `theme font_size=$fontNormal` landed on the root alone, so every
  Label under three of the first host's pages drew at 15. Any
  other word on a block's `theme` line is G3006.
- `for=` on a style names a kind (checked, G3006) and is not a property
  of the nodes wearing it; the wearer must be that kind (G3006 too,
  built in the areas/for= port round above).
- `areas=` on a Grid was refused in this round, naming `row { }` (it
  had been a warning and an empty grid); the areas/for= port round
  above built it, and it compiles today.
- `icon_size=` joins `icon=` as an accepted-unread Button word.

Changed 2026-09-01 (E10 — user decisions, the Godot addon now a stepping
stone on its way out; semantics may diverge from it for ergonomics):
- THE CAP LAW: a box never reports more than its size cap — any box,
  under any parent — and `max_*=fill` caps at the parent's inner space.
  This replaces the shipped D144 special case ("a hug cap under a
  container only on a Layers"), which promoted a Godot engine limit (a
  Container can't shrink a child's minimum) to a language rule and forced
  a meaningless Layers wrapper. One refusal remains, spelled where it
  bites: a cap that reads the parent's space on a stack's OWN stacking
  axis (`max_*=fill`, or any cap on a `fill` slot) is circular — the
  slot IS the frame; cap the stack itself. `max_*=fill` inside a Flow is
  refused too (a packed line is not a readable frame, v1). A Scroll
  scrolls past its cap, a List hugs then scrolls — unchanged.
- Stack minimums stay FRACTIONAL: Godot's whole-pixel truncation in
  BoxContainer minimums is not ported. It only mirrors Godot's integer
  text metrics (its Label ceils every measure), where it is a no-op;
  over fractional text measures it cut below what text needs and made
  wrap labels wrap a line early.

Changed 2026-09-02 (E12 — GP40 contract round step 2, user decisions):
- The native-backend gate family is CUT (customization spec: the
  `render=gogo` exits, the D112 Button carve-out): it polices
  Godot's native-vs-gogo backend split, and this engine has ONE
  renderer (D1). A `for=` style applies with no ceremony, and the
  `render=` word is REMOVED with the family (user 2026-09-02): one
  renderer needs no renderer word — it refuses as an unknown property,
  no alias, no teaching error.
- `for=` style `when` states check at the DECLARATION against the
  target's contract; a PLAIN style's check where it is applied — a
  control instance or a built-in (a plain box has no contract, its wear
  stays unchecked). A plain style whose custom state lands on exactly
  one control type draws the D2 advisory to add `for=`.
- Per-stop live props (step 4): a Gradient's declared stops stay live
  as numbered properties — `stop_1_color`, `stop_2_offset` (1-based,
  `stops=` declaration order), carried as PARAMETRIC prop keys
  (`PropKey::StopColor(n)` — no cap; the real check is "this gradient
  declares 2 stops — stop 3 names nothing"). A `when` response or a
  code write drives one stop; the declared look has ONE spelling
  (`stops=` — a static `stop_N_color=` errs toward it); a driven offset
  clamps non-decreasing (the declared `@` order stays law, G2005). The
  anim LANE (`g.stop_1_color.a to=…`) landed 2026-09-02 with the
  color/non-motion lanes (below).
- `pivot=` (2026-09-02, the Godot 4.7 offset-transforms comparison):
  a 0..1 box point — the fixed spot `scale` and `rotate` transform
  about; unset = the center (the old behavior verbatim), `0x0` holds
  the top-left, `1x1` swings on the bottom-right. One point serves the
  node's own rect, its subtree's shift, the rotation center and the
  hit walk's moved rects, so look and touch agree by construction. A
  look word on every box (not a motion channel, not an anim lane);
  G2005 holds it to a full 0..1 pair. Note from the same comparison:
  gogo already HAD offset transforms as a founding design (motion is
  transformed, layout declared) — Godot 4.7 caught up to what the
  addon armed via the engine's experimental flag (D10). Hit-testing
  follows EVERY channel: translate/scale/pivot through the moved
  rects (GPAR50), and `rotate` by un-rotating the FINGER about the
  nearest rotating ancestor-or-self (Godot's picking law, adopted
  same day) — the renderer applies the innermost rotate bracket per
  quad, so nearest-wins is exactly the picture; clips and wedges cut
  in screen space and keep the raw point. Zero cost while nothing
  rotates.
- Color/non-motion anim lanes (2026-09-02, user-approved design): any
  word a `transition` rule can move, `play(anim)` can drive — ONE list,
  two doors (`runtime::animatable`, the transition list plus the live
  stop family). `anim warn { b.background to=#FF3B30FF, duration=0.2 }`
  with everything a track already has (`and`/`then` chains, `from=`/
  `from=current`, `autoreverse`, `repeat=forever`, `speed=`, easings);
  `.a` on any color word moves alpha alone while the hue keeps following
  its ladder (`background.a`, `stop_1_color.a` — generalizing `color.a`).
  ONE ownership law with the motion channels: a playing anim owns its
  (node, property) lane — `when` and code writes yield, a `transition`
  on an owned lane spawns no tween (anim beats transition, user call), a
  newer player takes the lane over (WPF SnapshotAndReplace), `stop`
  restores the pre-play value, a finished anim holds its end pose, and a
  held pose yields to a `when` that writes the key. Inside: `AnimTrack`
  endpoints widened to `Value`, prop lanes tick into a layer that
  `compose_anim` lands as the LAST resolve override. Fences: non-
  animatable word, `.a` on a non-color, the kind-words table, `key`
  children on a color lane (numbers only — chain `then` steps), both
  spellings of one property in one anim. Pins: `anim_lanes.rs`,
  `stops_are_animatable` un-ignored.
- Theme entries take a `.Style` (step 3): the app-wide default look for
  every instance of the kind, worn FIRST — the weakest writer; instance
  styles and lines out-rank it, and the entry line's own props out-rank
  its style. Declared `control`s are legal entries (`theme { Fader
  .Chunky }`) — styles only, since a control's prop defaults are its
  `$param=` line; plain widgets keep refusing (no contract). Variant
  entries hold what can switch with the variant TODAY: base-look props
  riding the variant-gated kind defaults. A variant entry's `when`s and
  user-control restyles need variant-STAMPED baked writes — deferred
  LATER, IF NEEDED (user 2026-09-02, GP47).

Changed 2026-09-02 (GP40 leftovers, after the contract round):

- Enum words fold ASCII case (the addon's `_enum_fold`): `halign=Center`,
  `width=FILL`, `visibility=SPACE_ONLY` mean their lowercase words; the
  compile fence stores the CANONICAL spelling, so every later reader
  stays an exact compare. DEVIATION: the addon also stripped punctuation
  (`spaceonly` matched `space_only`) to match Godot editor labels
  ("Word (Smart)") — a surface Rust doesn't have; underscores are part
  of the word here. Call names (`fill(2)`, `hug(n)`) stay exact.
- A Circle in a non-square box draws a CENTERED DISC — bounding square
  min(width, height), centered both axes (gogo_circle.gd `_draw`) —
  never a pill stretched to the box.

Changed 2026-09-02 (GP26 step 1, user decision — Tab AND arrows):

- Keyboard focus: everything that answers a tap answers the keyboard —
  press surfaces (Button/Check/Toggle/Radio/ListItem), Slider, Input,
  and a `control` whose contract declares `state_builtin focused`;
  `keyboard_target=off` opts out (baked as `Node::focusable` at
  compile; disabled/hidden checked at traversal). Tab/Shift-Tab walk
  FILE order, wrapping; an arrow moves to the nearest reachable box in
  its direction (aligned neighbors beat nearer diagonal ones) — unless
  the focused control claims the key (an Input's caret claims
  Left/Right; §5.5 value steps come next). The FOCUSED bit is D129
  VISIBLE focus from the start: a key grant shows the ring, a tap parks
  focus with no ring — an Input's caret always warrants one. Focus is
  one owner on the input machine (`Input::focused()`); leaving an Input
  by ANY door (Tab, arrow, tap) commits through the one blur path.

Changed 2026-09-02 (GP26 step 2 — focus_within, modality, the ring):

- `focus_within` lights the owner and every ancestor (the ONE state
  that looks up), under the ring's own visible-focus warrant — a
  settings row lights while its inner control is key-focused, a tap
  into a Button parks silently, a tapped Input (caret) lights its row.
  The spelling stays the documented non-participle (D181: it names a
  relation, not a condition of the node). Priority unchanged — the
  ladder was already in `State::priority`, focus_within lowest.
- The D129 modality edges: ANY key press means keyboard driving — a
  parked (ringless) owner gets its ring and chain in place, even when
  the key itself did nothing; a pointer press turns the indicator off
  (re-tapping a focused box clears a ring earlier keys put there).
- The Button family (Button/Check/Toggle/Radio) ships an injected
  default `when focused` ring — `BUTTON_FOCUS_RING` (white @ 0x55,
  2 epx border), the addon's retired engine ring re-owned; authoring
  `when focused` replaces it (D122 per-state doctrine); `Input` is
  untouched (the caret is its focus look).
- MISPORT FIXED: custom states now sit between `held` and `disabled`
  (Custom=7 < StateDisabled=8) per the addon's §5.3 law — "`disabled`
  beats everything, always; a behavior can't out-rank the input
  blocker". The old Rust order had Custom on top, citing the addon's
  100+ prio while missing its disabled=1000 cap; pinned now.

Changed 2026-09-02 (GP26 step 3 — the §5.5 router, wheel, List cursor):

- The focused control claims its logical actions BEFORE traversal, one
  router (the addon's `_key_to_action`): Enter/Space = `activate`,
  running the same completions a finished tap runs (`change` first,
  then `activate` — one law for key and tap); a Slider claims ALL the
  arrows (right/up +`step=`, left/down −, default 1, quantized from
  `min` and clamped) plus Home/End; a Toggle claims arrows as its
  sides (right/up on, left/down off). A step commits `change`
  directly, NO `edit` (spec: a step and a programmatic write are
  indistinguishable at the seam). Tab is the way off a claiming
  control. Check/Radio claim only Enter/Space — their arrows traverse.
- A wheel tick rides the same pair: up/down = increment/decrement on
  the steppable control under the POINTER (Slider, Toggle), which also
  takes focus — parked, no ring (pointer driving). Wheel PANNING stays
  out (GP42's desktop round).
- The List keyboard cursor is plain focus (the shipped reading: the
  cursor IS engine focus): rows are reachable, arrows walk them,
  Enter selects (`change` on the LIST) then fires the row's
  `activate`. follow_focus (GPAR113): a KEY grant nudges every
  Scroll/List above the new owner just enough that its box is in view,
  clamped to the pannable range — a tap is already in view.
- User `control`s claim key GROUPS by contract (GP26, 2026-09-02):
  `keys arrows, home_end, enter, space` — a list line like
  `state_builtin`. A claimed key reaches the code-behind as a `Key`
  event on the focused instance root and never traverses; an
  unclaimed key traverses as always; Tab is never claimable — the way
  off a control. This is the built-ins' static-claim law made
  declarable (the shipped behaviors' `key_action` was a runtime
  claim, like Godot's `_gui_input` — invisible from the file); group
  words stay semantic so remapping never touches claims. A disabled
  instance claims nothing.
- User `control`s name their input machine by contract (GP52,
  2026-09-03): `behavior slider by $value in $min..$max, step=$step,
  key_unit=5%` — one line, next to `signal`/`keys`. The machine is the
  stock slider law (press anywhere on the instance jumps the value over
  bare x/track-width, `edit` streams, `change` commits on release,
  cancel restores, arrows/Home/End/wheel step; `key_unit=5%` is
  SliderPhone's shipped unit for continuous sliders, `step=` beats it;
  declared `drag_started`/`drag_ended` signals ride the gesture's
  edges — cancel included, so an armed host always hears the end).
  This is the addon's behavior-SCRIPT seam made declarative — the third
  of its kind (`automatic=` replaced geometry writes, `keys` replaced
  runtime key claims, `behavior` replaces pointer scripts). The
  vocabulary is CLOSED (`slider` today); an unknown name refuses with
  the list, and custom input stays the code-behind's (`on:pointer=` +
  `keys`, the GradientEdit pattern). One owner per key group: the
  machine owns arrows/Home/End, so a `keys` claim on them refuses
  (G0082) — never a silent precedence. Tier 3 (a host-REGISTERED
  behavior trait, the gogo user's own machines) is deliberately parked
  until RP6 pages expose the wiring cost — GP53.

Changed 2026-09-06 (GP53 — input behaviors are the gogo user's, user
call: option C over the closed enum):

- The RP6 evidence came in: the first host's 20 `on:pointer` sites (color
  wheel, small picker, gradient bar and marks, crop, object page) are
  2D surfaces with their own models, none value-shaped; the one
  value-shaped control beside `slider` was a two-state switch, whose
  `script "toggle_behavior.gd"` line the compiler DROPPED silently (the
  switch was deaf). What those code-behinds did share was the by-hand
  reframe of a pointer into a sibling's space — `Event::pointer_in(rects,
  node)` now.
- `behavior <name>` names a behavior the RUNTIME routes to: the addon's
  `GogoControlScript` reborn as `gogo_ui::Behavior` — `began(cx, x, y) ->
  Claim`, `moved`, `ended`, `canceled`, `key(cx, action) -> bool` —
  with `Cx` handing the value triad and the slots as CONTROL-LOCAL rects
  and taking ONE write back (`set_value`/`set_fraction`, snapped and
  clamped). A behavior never sees nodes, never raises a state or a
  signal: the runtime owns the law around it — `held`, focus, the
  second finger, `edit` per changed write, `change` on a changed
  release (a number, or the SIDE for a bool payload), the restore on
  cancel/reclaim, the declared drag edges (started before the press's
  first write, ended after the commit and on cancel), `selected`
  derived from the value when the contract declares it, the live-label
  follow. Gesture scratch lives in the behavior (one gesture at a time).
- gogo's own `slider` and `toggle` ARE implementations of the trait,
  inline in `Input` (no allocation for the stock pages); a host's
  behaviors are `Box<dyn Behavior>` per page `Input`
  (`Input::register`, one box each, once), found by NAME once per
  gesture — never per sample. The compiler checks a `behavior` line
  against a `BehaviorSpec` — the name, the slots the behavior reads
  (`cx.slot(i)` in that order; each must be a `slot` in the body,
  found by ancestry, the children-first arena trap), the key groups it
  OWNS (a `keys` claim on them refuses), the `change` payload — from
  `BUILTIN_BEHAVIORS` and `CompileOptions::behaviors`. Up to
  `BEHAVIOR_SLOTS` (3) rects per behavior.
- `toggle` (the addon's toggle_behavior.gd): a tap glides the value to
  the other end, a drag past 6 epx puts the thumb under the finger
  (thumb-width compensated over the track) and settles nearest on
  release; Enter/Space flip, Right/Up/End on, Left/Down/Home off; it
  owns all four key groups.
- The FEEL rule the addon had and Rust lacked: a driven slot's write
  while the root is HELD lands DIRECT (a glide in flight is dropped at
  the value); with `held` off — the release's settle, a key step, a
  code write — it glides by the control's `transition`. Without it the
  switch's 0.3 s feel trailed the finger by a glide per sample.
- Two rules a host's pooled rows forced the same day: a pooled repeat
  row is a CLONE and `Page::clone_subtree` carries the automatic-slot
  drivers, the `$value` readers and the control's `transition`
  feel (the clone had behaviors only — its fill never followed its
  value); and the frame loop asks `Runtime::is_moving` AFTER the layout
  pass, where glides are armed — the addon's `gogo_anims` group, kept
  current by every pump — so a host that sleeps until the next event
  never freezes a settle mid-slide.
- A `script` line in a control is a G0082 error naming `behavior` —
  refused, never dropped (the bug #4 class). The addon's three docs
  (gogo-spec.md, REFERENCE.md, COOKBOOK.md + the keyword cards) are the
  Rust gogo's end-user docs now (user 2026-09-06) — their control
  sections speak `behavior` and the Rust trait.

Changed 2026-09-02 (GP26 step 4 — hover, the chain reading):

- `hovered` answers a REAL mouse only (D75 — the host decides the
  source: it calls `Input::hover` for mouse motion and never for a
  touch, so no sticky hover survives a finger). The box under the
  cursor holds it — and so does EVERY
  ancestor. DEVIATION, user call 2026-09-02: the shipped addon lights
  only the control whose own `mouse_entered` fired; the chain reading
  is deliberate — gogo also runs game UIs where "the settings page
  lights, and the current slider inside it" is the staple. `hovered`
  now looks up exactly like `focus_within` (one shared chain walker).
- The capture bracket (the shipped hold): a mouse press FREEZES hover —
  slipping off a held button keeps its hovered look and nothing new
  hovers; the release re-checks the position (back over keeps it,
  anywhere else clears). Motion never changes modality (D129).
- Platform note: winit may synthesize mouse events from touch on some
  desktops; if a touchscreen ever shows sticky hover, gate by winit
  device id. Android sends no cursor events — unaffected.

Changed 2026-09-02 (GP26 step 5 — the soft keyboard, verified on an
Android phone in the first host):

- The keyboard follows the caret's EDGES: the host raises its soft
  keyboard when `Runtime::caret` appears or moves to another field and
  drops it when the caret goes away — every door (tap, Tab, wheel, blur,
  reset) is covered by that one check. Edges, NOT the level: syncing
  the level re-raised a keyboard the user had dismissed with Back,
  because the caret was still set — a Back dismiss stands until the
  field is tapped again (that tap re-raises).
- The IME speaks WHOLE text states, not keys (autocorrect and swipe
  rewrite spans): the host hands each state to `Input::ime_replace`,
  which writes it into the focused field — max_length applies like
  typing, `edit` streams once per genuine change, and the commit law
  holds unchanged (the host sends the IME's Done key as `Key::Enter`,
  the one Enter door). `Input::ime_seed` gives the text and the
  selection to start the IME from.
- A host with several pages hands a tap that hits nothing on any page
  to the page holding focus: that tap is the focus-loss commit door,
  and dropping it leaves the field uncommitted and the keyboard up.

Changed 2026-09-02 (GP24, the user's call): unstyled text stays PURE
WHITE — Labels and every Button state. A decided deviation from Godot's
default theme (Label 0.875 = #DFDFDF; Button 0.875 / hover 0.95 /
pressed 1.0 / disabled 0.875@0.5): the Rust chrome look is the design,
so parity captures differ on unstyled glyphs by design.

Changed 2026-09-02 (GP48 — Input selection + copy/paste, steps 1-3,
emulator-verified):

- ONE anchor + the moving caret is the whole selection model: Shift
  moves extend/shrink through the same arm, a plain arrow collapses to
  its side without moving, typing and both erasers act on the span with
  one `edit`, Ctrl+A selects all. A press places the caret where it
  LANDS (decided behavior change — it used to jump to the end); a MOUSE
  drag selects while a finger's drag still pans (pointer kinds never
  blur into each other); a double click selects the word.
- The clipboard never enters gogo: the host owns Ctrl+C/X/V and the
  clipboard itself (a desktop crate, Android's ClipboardManager);
  gogo only hands text out (`selected_text`) and takes text in
  (`paste`, `delete_selection` — `max_length` caps the RESULT).
- The touch door is a long-press: 0.5 s still in a field selects the
  word under the finger and raises the edit bar (Cut/Copy/Paste/All) —
  the built-in `edit_bar.gogo` PAGE (module `gogo_edit_bar`, compiled
  into every app by `build_pages`): the labels are translation keys,
  the look is plain gogo (restyle the file, not Rust), and the host
  composes it topmost, docks it with `edit_bar_anchor`, packs the
  buttons by state (Cut/Copy need a selection, All needs text) and
  answers the taps through the same doors as the shortcuts. A press
  reaching the page's own machine is off-bar by construction and
  dismisses; All keeps the bar up. `keyboard_target=off` keeps the
  transient chrome out of Tab traversal.
- A touch-born selection (long-press, double-tap, the bar's All) hangs
  two drag circles below the field — `sel_handle_rect` is ONE geometry
  for the renderer and the grab check: dragging moves THAT end and
  never collapses past one char; the bar stays up (adjusting is not
  dismissing). Mouse and keyboard selections show no circles — they
  have drag-select and Shift+arrows. `selection_color=` (an Input's
  own word) tints the highlight (unset = white at 25%) and, made
  opaque, the circles.
- The IME re-seed law (found on device): an IME pushes whole text
  states on its own schedule, so a LOCAL change it never saw (the
  long-press select, the bar's All, a paste) gets REVERTED by its next
  push. A host re-seeds (`Input::ime_seed`) whenever the field's state
  differs from what the IME last saw; ordinary typing updates that
  memory instead, so composing (autocorrect spans) is never restarted.
- The bar's four labels are the translation keys `cut`, `copy`, `paste`
  and `selectAll`: a host whose tables lack them shows the English
  literals. Emoji addendum 2026-09-02: a host converts IME spans
  between UTF-16 units and chars at its own seam (`Input::ime_replace`
  counts chars), so backspace deletes a whole emoji.

### Added 2026-09-05 (user): `border_dash=`, gradient tiling, code images

- **`border_dash=[4, 1]`** on any plate (Panel, Rectangle, a Label's or
  Image's plate, Buttons): the dash and gap lengths in `border_thickness`
  units, repeated along the edge — WPF's StrokeDashArray, so shipped's
  `"4 1"` ports as written; an odd list repeats to even out. Drawn as
  crisp axis-aligned quads walking the perimeter clockwise from the
  top-left with the pattern continuing across the corners; never
  antialiased line ends, which feather into a 1-epx gap and erase it
  (D257). The stroke sits inside the box like the solid border; a dashed
  plate draws its fill and no solid border; corners stay square while
  dashed. Bad spellings are G2009.
  Why: the crop UI's aspect glyphs were pixel-rendered code images because
  the word did not exist — a declaration is the gogo way.
  - **On a Circle** (added 2026-09-06, user): the ring dashes, the lengths
    measured along the ring's centerline, the pattern starting at the top
    and running clockwise, the last dash cut where the ring closes (WPF's
    Ellipse StrokeDashArray). The disc keeps its fill and drops the solid
    ring. Why: the mold page's brush cursor — a gray ring that turns dashed
    and rotates for the repair brush — was a per-frame model the host drew;
    with the word it is a declared Circle and three `anim`s.
  - **`dash=[4, 3]` on an Arc** (same day, user): the same pattern along
    the arc's own stroke, in `thickness` units (an Arc has no border).
  - How a dashed ring renders (same day, user): ONE quad per dash/gap pair
    with the mask in the shader — the arc quad's free uv slots carry
    (offset, dash, period) as angles, and each pixel keeps or drops by
    where it falls in the period, with the ring's one-pixel ramp on the
    dash ends. Never a draw per dash: 48 full-disc arc quads for the
    cursor's repair ring were ~50x the fill of one masked quad, on a phone
    GPU at the moment it is busiest (the mold apply). Straight dashes stay
    crisp per-dash quads (D257): each is its own box, no overdraw.
- **Gradient tiling**: `repeat=repeat|mirror` together with a `from=`/`to=`
  span makes that span ONE PERIOD and the ramp repeats across the whole
  box (CSS repeating gradients); `mirror` folds within the period as it
  folds within the box today. The draw carries `period` (start, length in
  run units) and the renderer tiles by `fract`. Linear fills only; without
  a span nothing changes. Why: the progress shimmer — one tilted, moving
  comb of stripes — was 16 tiles and lost shipped's 3-degree tilt at the
  seams; one tiled gradient has the tilt back.
- **Code images, one registry**: a host names the `res://` pictures its
  CODE shows in `BuildOptions::code_images` (the crate ships no macro for
  it: the first host wraps each path in a macro of its own,
  `gogo_image!("res://assets/x.png")`, and its build.rs scans
  `src/**/*.rs` for the literal); they join the SAME image table as every
  `source=` (one `ImageKey` space, one embed, one decode) and the
  generated `code_image(path)` gives the site its key. A path built at
  runtime is not embedded, and the runtime table is SEALED for
  `res://` names: interning one the build did not embed is the
  **`unembedded_image` lint**, a debug panic naming the path (release
  interns a key that shows nothing). Host pictures (`face://3`,
  `preset://default`) are not files and stay welcome. Why: the clone page
  had declared a hidden plate of four icons only to get them embedded —
  a second registry by the back door.

### Added 2026-09-06 (user): a gradient draws ONCE (D21)

- The draw list carries one `Gradient` entry per gradient followed by
  its `GradientStop` records (positions non-decreasing along the run,
  colors as authored with the node's opacity applied). It used to carry
  one full-box entry per stop pair, which the renderer painted as one
  full-box quad each — every pixel of a sixty-stop gradient bar was
  visited fifty-nine times and thrown away fifty-eight of them.
- The renderer keeps two tables, a row per gradient on screen: an INDEX
  MAP (bytes: which stop pair covers each slot along the run, 1024 slots
  per row, a gradient uses as many as its box has device pixels along
  its longer side, times 1.5 for the diagonal, at most 1024) and the
  STOPS (two texels per stop: the premultiplied Oklab color as 16-bit
  fixed point over -1..1, then the position as two 16-bit halves of a
  32-bit fraction). A pixel does one map read and four stop reads, no
  loop, no search, then the same Oklab mix and decode as before. Rows are
  cached by content hash across frames and refilled only when a stop
  changes; uploads go as one dirty row range per table per frame. Both
  tables are INTEGER textures read by exact texel (`textureLoad`), never
  filtered — an interpolated pair index is not a pair, and a float32
  table bound unfiltered was an incomplete texture on the OnePlus's GLES
  3.0 driver (opaque black); integers fetch on every driver (D21).
- The slot rule is written ONCE for both sides: slot = floor(run × n);
  the CPU fills slot s with the pair that contains s / n. A stop inside a
  slot puts the slot's first pixels on the neighbouring pair with t
  clamped, an error under one device pixel at the density above.
- Mirror (`repeat=mirror`) is the stop list folded into 0..0.5 with its
  mirror appended; a lone stop becomes the same color at both ends; two
  stops at one position are a zero-width pair the fill never selects — a
  hard edge, as before.

### Added 2026-09-06 (user): `window=`, `sampling=`, `%` anim endpoints

- **`window=[left, top, width, height]` on an Image**: draw only this
  rectangle of the texture, in the texture's own pixels; unset = the
  whole texture. `fit=` maps the window, not the texture, into the box.
  Why: the painting sits on the GPU as tiles, and a tile's texture
  carries a border of pixels copied from its neighbours so smoothing at
  a tile edge blends into the right colours and no seam shows; the
  border must never reach the eye. Shipped showed each tile through an
  AtlasTexture window (CanvasView.cs Setup); the word is that window.
  Four numbers, corner at or past 0, positive size — else G2010.
- **`sampling=smooth|pixel` on an Image**: `smooth` (the default) blends
  between texels; `pixel` shows each texel as a hard block. The renderer
  keeps two samplers and batches by the choice. Why: zoomed past 21 px
  per painting pixel the canvas flips to blocky sampling so the user sees
  crisp pixels, not blur (shipped DisableImageFilteringLimitEpx).
- **A `%` endpoint on a `translate.x` / `translate.y` anim lane**
  (`from=-100%`, `to=100%`): a fraction of the node's OWN laid-out box on
  that axis — its width for x, its height for y — resolved when the anim
  starts, from the last `glide`; a node with no box yet (hidden at play,
  the usual case for a sheet sliding in) resolves at the next `glide`,
  and its first laid-out frame already shows the from pose. A lane on any
  other channel refuses it (G0064). Why: the text page's two settings
  sheets slide in by their own height and push each other sideways by
  the screen width, numbers no file can write; with the word, all four
  slides are declared and the code only names which one to play.

### Added 2026-09-06 (user): `text_fit=stretch`, `text_flip=`, the 3D turnstile

- **`text_fit=stretch` on a Label**: the text's ONE glyph, its ink box
  pulled over the Label's box — each axis its own stretch, so a tall
  narrow box gets a tall narrow glyph. **`text_flip=horizontal|vertical|
  both`** mirrors it. The draw list carries a `TextStretch` entry; the
  renderer rasterizes the glyph near the box's larger side (at most 1024
  device px — a page-sized box stretches that) and maps the ink box onto
  the rect, flipping the uv ends. Why: a speech bubble on the text page is
  one character of an ornament font stretched over the callout
  (Callout.xaml.cs's stretched TextMesh); with the word, a callout is a
  widget of two Labels and its bake is the same file laid out at the
  painting's scale (D22). `text_fit=none` (the default) draws text as
  text; `text_flip=` without the stretch changes nothing; the words are a
  Label's alone (a Button's text stays text) and closed sets (G0057).
- **The `swing` motion channel** (`Motion::swing`, an anim's
  `box.swing from=0, to=90` lane): degrees the node turns about its LEFT
  edge in 3D. The draw walk wraps a swinging node in a `SwingPush`
  (hinge x, the camera on the box's center, `swing_focal(height)`), the
  renderer projects it like Silverlight's PlaneProjection. `tap_pulse`
  rides this lane (D23); hits read the door's projected span.
- **A font key may end in a style suffix** (`gogo_ui::font_style`:
  `"Angelina#bold"`, `#italic`, `#bold-italic`) — faux bold and italic
  through the one `font=` word; the text engine reads the suffix.

### Added 2026-09-27 (user): `atlas` — grids, named slices, JSON imports, 9-slices (D97)

- **`atlas <name> source=… | from=…`** at a file's top level cuts one
  picture into slices; `source=` shows one as `name[12]` (grid,
  `cell=WxH`), `name.play` (a `{ slice … }` block or a `from=` packer
  JSON) or `name` (the whole picture, usually with `edge=`). `edge=` makes
  a 9-slice. Language rules: gogo-spec.md "Atlases"; usage: REFERENCE.md
  Directives → Atlases (D97).
- **A slice is an `ImageKey` with the top bit set**, indexing the build's
  `pub static SLICES`; `draw` turns it into its picture plus windows, so
  hosts only see whole pictures and `Value`/`Draw` keep their size.
  `Page::atlas_slice` / `Page::picture_of` resolve one; `CompileOptions::assets`
  reads the picture's header and the JSON at build time.

### Added 2026-09-28 (user): `release=unused`, host layers, `height=$value` (D98)

- **`release=unused`** on an `atlas` line or an Image naming a file: the
  picture may leave the GPU while no live page names it (the sticker
  sheets). Every other built-in picture stays for the run; scrolling and
  hiding never free. The build emits `RELEASE_UNUSED`; the host sweeps
  after a page is freed or when page code asks. Rules: gogo-spec.md
  "Atlases"; usage: REFERENCE.md Image + Atlases; error G2020.
- **Host layers:** an Image whose `source=` the host opened with
  `UiRenderer::host_layer` plays the host's recorded drawing at that point
  of the UI's own pass — between what the page drew before and after it.
- **`width=` / `height=` take a numeric `$value`** like `min_width=` and
  padding already did (default entry only); `Runtime::theme_number` reads
  one from code.
- **A hugging Flow is as wide as its widest wrapped row** (user: "do C",
  2026-09-28): offered a width, `flow_min` packs the rows into it and
  reports the widest packed row, not the widest single item — so `Flow
  halign=center` is a centred block whose short last row starts at its
  left (the sticker sheet; the old answer was a one-item column). The
  minimum a Flow asks for is still its widest item. Rejected: a new
  `line_align=` word — centring each line also centres the short last row,
  and WP's look needed a second word. Pin: `godot_placement.rs`
  `a_hugging_flow_is_as_wide_as_its_widest_packed_row`.

## 8. Property words per kind (E8, 2026-08-31)

One table in `page.rs` (`kind_row`) says which words each kind takes. The
compiler refuses any other word as an error — a word gogo never had gets
G2001 with the closest legal word ("did you mean `text=`?") or its gogo
spelling (`foreground=` → `font_color=`); a real gogo word on the wrong kind
gets G2002 naming whose word it is and what this kind writes instead
(`color=` on a Label → "text is `font_color=`, the plate is `background=`").
The same check runs on every path a word can enter: node lines, styles,
theme kind defaults, `when` responses aimed at the node or at a child
(`when held flash.opacity=0` checks `flash`'s kind once the tree is built),
and code controls (which take the shared row: box, plate, padding).

Any box has a plate: `background=`, `corner_radius=`, `border_*=` are legal
and drawn on a Label, an Image, a Layers or a stack, behind the content and
covering the padding, the same as on a Panel — a chip is `Label text="x",
background=red, padding=4`, no wrapping Panel. A box with none of the plate
words costs no draw command. (Godot gogo refused the plate off Panels; this
runtime draws it — the rule is one sentence instead of a list of
exceptions.)

The block between the markers is generated from the table by the
`kind_words_doc_is_current` test — on a mismatch it prints the block to
paste.

<!-- kind-words:start -->
Every box takes: `width`, `height`, `min_width`, `min_height`, `max_width`, `max_height`, `size`, `min_size`, `halign`, `valign`, `margin`, `margin_left`, `margin_top`, `margin_right`, `margin_bottom`, `aspect`, `opacity`, `visibility`, `overlay`, `interactive`, `font`, `scale`, `scale.x`, `scale.y`, `translate.x`, `translate.y`, `rotate`, `pivot`, `seat`, `disabled`, `touch_target`, `reveal_x`, `reveal_y`, `accessibility_name`, `keyboard_target`.
A Grid cell adds `col_span`, `row_span`; a Tiles cell adds `span`, `full_row`.

| kind | own words |
|---|---|
| `View` | `background`, `corner_radius`, `corner_radius_top_left`, `corner_radius_top_right`, `corner_radius_bottom_right`, `corner_radius_bottom_left`, `border_thickness`, `border_color`, `border_dash`, `clip_contents`, `padding`, `padding_left`, `padding_top`, `padding_right`, `padding_bottom`, `disabled_opacity` |
| `Panel` | `background`, `corner_radius`, `corner_radius_top_left`, `corner_radius_top_right`, `corner_radius_bottom_right`, `corner_radius_bottom_left`, `border_thickness`, `border_color`, `border_dash`, `clip_contents`, `padding`, `padding_left`, `padding_top`, `padding_right`, `padding_bottom` |
| `Column` | `background`, `corner_radius`, `corner_radius_top_left`, `corner_radius_top_right`, `corner_radius_bottom_right`, `corner_radius_bottom_left`, `border_thickness`, `border_color`, `border_dash`, `clip_contents`, `padding`, `padding_left`, `padding_top`, `padding_right`, `padding_bottom`, `spacing` |
| `Row` | `background`, `corner_radius`, `corner_radius_top_left`, `corner_radius_top_right`, `corner_radius_bottom_right`, `corner_radius_bottom_left`, `border_thickness`, `border_color`, `border_dash`, `clip_contents`, `padding`, `padding_left`, `padding_top`, `padding_right`, `padding_bottom`, `spacing` |
| `Flow` | `background`, `corner_radius`, `corner_radius_top_left`, `corner_radius_top_right`, `corner_radius_bottom_right`, `corner_radius_bottom_left`, `border_thickness`, `border_color`, `border_dash`, `clip_contents`, `padding`, `padding_left`, `padding_top`, `padding_right`, `padding_bottom`, `spacing`, `orientation` |
| `Layers` | `background`, `corner_radius`, `corner_radius_top_left`, `corner_radius_top_right`, `corner_radius_bottom_right`, `corner_radius_bottom_left`, `border_thickness`, `border_color`, `border_dash`, `clip_contents`, `padding`, `padding_left`, `padding_top`, `padding_right`, `padding_bottom` |
| `Scroll` | `background`, `corner_radius`, `corner_radius_top_left`, `corner_radius_top_right`, `corner_radius_bottom_right`, `corner_radius_bottom_left`, `border_thickness`, `border_color`, `border_dash`, `clip_contents`, `padding`, `padding_left`, `padding_top`, `padding_right`, `padding_bottom`, `allow`, `bars` |
| `Grid` | `background`, `corner_radius`, `corner_radius_top_left`, `corner_radius_top_right`, `corner_radius_bottom_right`, `corner_radius_bottom_left`, `border_thickness`, `border_color`, `border_dash`, `clip_contents`, `padding`, `padding_left`, `padding_top`, `padding_right`, `padding_bottom`, `cols`, `areas`, `row_height`, `spacing` |
| `Tiles` | `background`, `corner_radius`, `corner_radius_top_left`, `corner_radius_top_right`, `corner_radius_bottom_right`, `corner_radius_bottom_left`, `border_thickness`, `border_color`, `border_dash`, `clip_contents`, `padding`, `padding_left`, `padding_top`, `padding_right`, `padding_bottom`, `cols`, `cells`, `spacing` |
| `Label` | `background`, `corner_radius`, `corner_radius_top_left`, `corner_radius_top_right`, `corner_radius_bottom_right`, `corner_radius_bottom_left`, `border_thickness`, `border_color`, `border_dash`, `clip_contents`, `padding`, `padding_left`, `padding_top`, `padding_right`, `padding_bottom`, `text`, `font_size`, `font_color`, `wrap`, `clip_text`, `horizontal_alignment`, `vertical_alignment`, `text_fit`, `text_flip` |
| `Button` | `background`, `corner_radius`, `corner_radius_top_left`, `corner_radius_top_right`, `corner_radius_bottom_right`, `corner_radius_bottom_left`, `border_thickness`, `border_color`, `border_dash`, `clip_contents`, `padding`, `padding_left`, `padding_top`, `padding_right`, `padding_bottom`, `text`, `font_size`, `font_color`, `wrap`, `clip_text`, `horizontal_alignment`, `vertical_alignment`, `tap_pulse`, `toggle`, `selected`, `icon`, `icon_size` |
| `Image` | `background`, `corner_radius`, `corner_radius_top_left`, `corner_radius_top_right`, `corner_radius_bottom_right`, `corner_radius_bottom_left`, `border_thickness`, `border_color`, `border_dash`, `clip_contents`, `source`, `fit`, `loading`, `release`, `window`, `sampling` |
| `Rectangle` | `color`, `color.a`, `corner_radius`, `corner_radius_top_left`, `corner_radius_top_right`, `corner_radius_bottom_right`, `corner_radius_bottom_left`, `border_thickness`, `border_color`, `border_dash` |
| `Circle` | `color`, `color.a`, `border_thickness`, `border_color`, `border_dash` |
| `Arc` | `color`, `color.a`, `from`, `sweep`, `thickness`, `dash` |
| `Gradient` | `stops`, `fill`, `angle`, `repeat`, `from`, `to`, `corner_radius` |
| `Slider` | `background`, `corner_radius`, `color`, `value`, `min`, `max`, `step` |
| `Input` | `background`, `corner_radius`, `corner_radius_top_left`, `corner_radius_top_right`, `corner_radius_bottom_right`, `corner_radius_bottom_left`, `border_thickness`, `border_color`, `border_dash`, `clip_contents`, `padding`, `padding_left`, `padding_top`, `padding_right`, `padding_bottom`, `text`, `font_size`, `font_color`, `wrap`, `clip_text`, `horizontal_alignment`, `vertical_alignment`, `value`, `placeholder_text`, `max_length`, `selection_color` |
| `Check` | `background`, `corner_radius`, `corner_radius_top_left`, `corner_radius_top_right`, `corner_radius_bottom_right`, `corner_radius_bottom_left`, `border_thickness`, `border_color`, `border_dash`, `clip_contents`, `padding`, `padding_left`, `padding_top`, `padding_right`, `padding_bottom`, `text`, `font_size`, `font_color`, `wrap`, `clip_text`, `horizontal_alignment`, `vertical_alignment`, `tap_pulse`, `selected` |
| `Toggle` | `background`, `corner_radius`, `corner_radius_top_left`, `corner_radius_top_right`, `corner_radius_bottom_right`, `corner_radius_bottom_left`, `border_thickness`, `border_color`, `border_dash`, `clip_contents`, `padding`, `padding_left`, `padding_top`, `padding_right`, `padding_bottom`, `text`, `font_size`, `font_color`, `wrap`, `clip_text`, `horizontal_alignment`, `vertical_alignment`, `tap_pulse`, `selected`, `value` |
| `Radio` | `background`, `corner_radius`, `corner_radius_top_left`, `corner_radius_top_right`, `corner_radius_bottom_right`, `corner_radius_bottom_left`, `border_thickness`, `border_color`, `border_dash`, `clip_contents`, `padding`, `padding_left`, `padding_top`, `padding_right`, `padding_bottom`, `text`, `font_size`, `font_color`, `wrap`, `clip_text`, `horizontal_alignment`, `vertical_alignment`, `tap_pulse`, `selected` |
| `choice` | `value`, `spacing` |
| `List` | `background`, `corner_radius`, `corner_radius_top_left`, `corner_radius_top_right`, `corner_radius_bottom_right`, `corner_radius_bottom_left`, `border_thickness`, `border_color`, `border_dash`, `clip_contents`, `padding`, `padding_left`, `padding_top`, `padding_right`, `padding_bottom`, `select`, `spacing`, `bars` |
| `ListItem` | `background`, `corner_radius`, `corner_radius_top_left`, `corner_radius_top_right`, `corner_radius_bottom_right`, `corner_radius_bottom_left`, `border_thickness`, `border_color`, `border_dash`, `clip_contents`, `padding`, `padding_left`, `padding_top`, `padding_right`, `padding_bottom`, `tap_pulse`, `selected` |
| `repeat` | (none — the line takes clauses, not properties) |
| `gap` | (none — the line takes clauses, not properties) |
| `row` | (none — the line takes clauses, not properties) |
| `skip` | (none — the line takes clauses, not properties) |
| `slot` | `background`, `corner_radius`, `corner_radius_top_left`, `corner_radius_top_right`, `corner_radius_bottom_right`, `corner_radius_bottom_left`, `border_thickness`, `border_color`, `border_dash`, `clip_contents`, `padding`, `padding_left`, `padding_top`, `padding_right`, `padding_bottom`, `automatic` |
| `seat` | (none — the line takes clauses, not properties) |
| `empty` | (none — the line takes clauses, not properties) |
| `<code control>` | `background`, `corner_radius`, `corner_radius_top_left`, `corner_radius_top_right`, `corner_radius_bottom_right`, `corner_radius_bottom_left`, `border_thickness`, `border_color`, `border_dash`, `clip_contents`, `padding`, `padding_left`, `padding_top`, `padding_right`, `padding_bottom` |
<!-- kind-words:end -->

## Checks for G1's successors

- G2: every example in `gogo-spec.md` and the Godot `test/` fixtures parses
  to the expected arena (kind, name, children ranges, prop count).
- G4: rect pins per kind, taken from the Godot suites where they exist
  (same file, same numbers) — layout parity before render parity.
- G5: screenshot vs the Godot addon per page, max channel diff.
