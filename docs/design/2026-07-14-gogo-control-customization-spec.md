# Gogo Spec: Control Customization & User-Defined Controls

Status: Design settled, ready for implementation
Scope: the customization ladder (theme vars → styles → slot fills → user controls), the `for=` style contract, `slot` unification, `state` / `state_builtin`, the `control` construct, automatic geometry, the built-in backend opt-in (`render=gogo`, §5.7), theme-level type defaults (§5.8), and all associated checking, diagnostics, and tooling behavior.

This spec assumes existing Gogo syntax and semantics as shipped: `Control optional_name prop=value, ... { children }`, `uses "file.gogo"`, `$name` (params / theme vars / data), `.StyleName` application, `when <state> prop=value`, `widget`, `signals` keyword, `emit(...)`, `arrangement`, standalone `transition duration=..., easing=...` rules, `"text"/i18nKey`, `WxH` size shorthand, `gap`. Nothing in this spec changes existing behavior except where explicitly stated (§2.3: plain-style `when` names move their check to the application site, and `disabled` becomes a universal state — shipped gogo errors it off-button; §3.4: named slots, and default content on the bare `slot`, extend the existing `slot`; §5.3: `state` is un-retired inside `control` blocks and the docs' umbrella word migrates condition → state; §5.3 again: hover follows capture on **every** control — shipped hover clears on pointer-exit even mid-drag, all backends adopt follow-capture in this round). *Build status: the three shipped-behavior deltas — universal `disabled`, hover follows capture, `when`-name checking at the application site — are BUILT (D106, 2026-07-14, CC1). The condition→state vocabulary migration rides CC3 with the `control` construct.*

---

## 0. Vocabulary (user-facing, mandatory)

All user-facing strings — errors, warnings, fix-it text, hover/completion text, explain-pane output — use this vocabulary and no other:

| Say | Never say |
|---|---|
| slot | part, role |
| state | flag, visual state, condition (the pre-spec docs word — migrated per §5.3) |
| automatic (as in "position is automatic") | compiler-owned, coupled, driven |
| built-in state / built-in control | system state; *native* as a control-kind word |
| "catches typos while you type" | "checked against the contract" |

The words *contract*, *coupling*, *ownership* are internal/spec vocabulary only. Every user-facing string is written from the user's goal inward ("Restyling a whole control?"), not from the mechanism outward ("Validates the style against..."). The two state classes of §2.3 are spec-internally *universal* vs *contract* states; user-facing surfaces say **universal state** / **control state** (*contract* stays internal, per this fence). *Native* is reserved for the **backend** (`render=native`, native pass-through — §5.7) and is never a synonym for built-in controls or states.

---

## 1. Overview: the customization ladder

Four tiers, strictly escalating in power. Each tier has exactly one spelling. Diagnostics with fix-its route users between tiers (§7).

- **Tier 0 — theme variables.** `$accent = #FF6B4AFF`. Already exists. No changes.
- **Tier 1 — styles.** Property bundles. `when` accepts any state name in any style. New: `for=` unlocks a control's slots (`slot.` paths) and turns on write-time checking of state names. On built-in instances, contract features ride the Gogo backend — `render=gogo` per instance (§5.7) or a theme-level default (§5.8).
- **Tier 2 — slot fills.** `slot name { content }` inside a control's braces puts author content inside a control-owned slot. New for built-ins (Gogo backend, §5.7/§5.8); extends the existing widget `slot` with names and defaults.
- **Tier 3 — user-defined controls.** The `control` construct: a widget plus a behavior script, declared states, and slots with automatic geometry. Produces controls that are first-class citizens of tiers 0–2 (stylable with `for=`, fillable with `slot`, introspectable in the explain pane).

A key invariant across all tiers: **value-driven geometry is automatic.** Styles and fills can never cause a control's visuals to disagree with its value (e.g. a slider thumb drifting from its fill). This is enforced by ownership errors (§4.3), not convention.

---

## 2. Tier 1: `for=` styles *(D109 — built 2026-07-14, CC4: write-time checking, instance slot paths, the D1/D2/D3/D4/D10/D11/D14/D16 family; D15 completed 2026-07-14 with CC6's `render=`)*

### 2.1 Syntax

*Pinned for CC4 (D108 addendum): a `for=Knob` style requires the declaring file to reach Knob via `uses` — a declaration-site error otherwise (the target is checked where the style is written, not first at application).*

```gogo
style ThinSlider for=Slider, slot.track.height=2, slot.thumb.size=12 {
    when dragging slot.thumb.scale=1.2
    when focus    slot.thumb.background=$accent
}
```

- `for=` is an ordinary-looking property in the style header, but `for` is a **contextual keyword** reserved in style headers only. It can never be a settable style property.
- **`for=` must be the first property when present.** Violation is an error with a fix-it that reorders. Rationale: the reader and the editor assist know the target before any slot path appears.
- The target is the control's **type name exactly as spelled in trees**: `for=Slider`, `for=Check` (built-ins), `for=Knob` (user-defined, §5). One spelling per type — gogo's capitalization law makes every addressable type Capitalized, and a second lowercase spelling would split every grep (the D83 rule that retired the raw Godot aliases). Built-in vs user-defined needs no case mark: the name already carries it, and the compiler knows.
- `for=` does **not** take a list. Cross-control style sharing is expressed by contract relationships declared on the controls themselves (e.g. Progress's contract is defined as Slider's minus `thumb`; a `for=Slider` style is then applicable to Progress by rule). Contract relationships — built-in pairs and user-declared alike — are deferred to §9: empty until their members exist (§5.7's v1 set has no Progress).
- **`extends=` compatibility** (style single-inheritance is shipped): a style may extend a style whose requirements it can satisfy. Plain extends plain — as shipped. `for=Slider` extends plain — fine: the base's ordinary properties and universal states are valid under any target, and author-node dotted paths in the base resolve at application exactly as when the plain style is applied directly (same late binding, same D14-class outcome if nothing matches). `for=Slider` extends `for=Slider` — fine, fully checked. The two errors: `for=Slider` extends `for=Button` (the base's slot paths mean another control's slots — meaningless; revisit only if contract relationships ever ship, §9), and plain extends `for=X` (the derived style would inherit slot paths it has no contract to check or even address — D1 territory by inheritance). Diagnostic wording follows the D1/D10 family.

### 2.2 What `for=` unlocks

Inside a `for=X` style (header and body):

1. **`slot.` paths**: `slot.<slotName>.<property>` sets a property on the named slot of the target control. `slot` is a contextual keyword in path-root position within `for=` styles; it cannot collide with author node names (author nodes are addressed by bare paths as today: `flash.visible=true`).
2. **Write-time checking of `when` states**: every state name in the style is verified against the target's states — the universal set plus its declared contract states (§2.3, §5.3). Note that `when` itself is available in all styles (§2.3); `for=` is what moves its check to *write time*.

Both are **write-time checked** against the target's contract:
- Unknown slot name → error listing the control's slots.
- Unknown state name → error listing the control's states (D4). This includes **contract states the control didn't opt into**: `when pressed` in a `for=Knob` style where Knob lacks `state_builtin pressed` is an error — for user controls the fix-it offers adding `state_builtin pressed` to the control declaration (or removing the clause); for built-ins, whose state sets ship fixed, only the remove/rename fix-its apply. Universal states (`hover`, `focus`, `disabled`) never error — an undeclared one draws advisory lint D16 instead (§5.3). (This kills the string-typo bug class; `when draging` is a compile error with a rename fix-it.)
- Property not applicable to the slot → normal property checking.
- Assignment to an automatic property → ownership error (§4.3).

### 2.3 Plain styles (no `for=`)

- Plain styles may use `when` on **any state name**; contract-state names are checked where the style is **applied**. State names come in **two classes** — the split shipped gogo already has (`hover`/`focus` work anywhere, `pressed` needs a button), named instead of flattened:
  - **Universal states** (`hover`, `focus`, `disabled`): facts about any control, maintained by the runtime everywhere — built-ins and user controls alike. Styleable in any style, plain or `for=`; never a D4/D14 trigger. (`disabled` joins the universal set — shipped gogo errors it off-button; its enforcement is systemic and it is meaningful on any control. Behavior change, noted in the preamble.)
  - **Contract states** (`pressed`, `checked`, and all custom names like `dragging`): capabilities of *some* controls — inherent on the shipped button family, declared via `state_builtin`/`state` on user controls (§5.3). Legal in any style at declaration, checked where the answer is known: at application (D14) or at write time under `for=` (D4) — with lint D2 suggesting `for=` so the name is checked as you type. D2 is **evidence-scoped**: a style applied **cleanly** to two or more distinct control types is deliberately generic by evidence, and the lint stands down — an application that errors (D14, D15) establishes nothing (no suppression mechanism exists, and none is added; the application sites D2 already reads for target inference are the evidence). Existing code like `when pressed flash.visible=true` remains legal as-is (buttons have the capability).
- **Application-time check (D14, contract states only):** applying a style whose `when` names a contract state the control does not have is a load-time **error** at the application site (``Button has no state 'dragging' — states here: hover, focus, disabled, pressed, checked``) — the same dead-and-loud rule as shipped `pressed`-needs-a-button; `for=` moves the same catch to write time. **Behavior change vs shipped:** today an unknown `when` name errors at the style *declaration*; this spec legalizes the declaration (whether a control declares the state is only knowable at the application site) and keeps the error, moved to where the answer is known.
- `slot.` paths in a plain style are errors carrying the fix-it that inserts `for=<control>` in first position (§7, D1). Exception: if a bare dotted path's root resolves to an author-named node reachable at the application site, it is an ordinary node path, not a slot path — D1 only fires when `slot.` is written explicitly or when no such node exists.

The asymmetry between slots and states is principled: a `slot.` path needs a contract to *mean* anything (which control's thumb?), so `for=` is required; a state name is meaningful per application site, so `for=` is optional and only adds write-time checking. Deliberately generic shared styles (one `when dragging` style applied across Slider, scroll bars, and Knob) are therefore expressible without `for=` — and applicable exactly to the controls that declare every state the style names (anything else is the D14 error).

### 2.4 Animation and transition scoping *(D108 — built 2026-07-14, CC3: declaration default feel + node-block override + fraction glide; the style tier joins with `for=`, CC4)*

No per-state animation syntax exists or is added. `transition duration=..., easing=...` rules time every `when` flip in scope; leaving a state tweens back to base ("base is the other keyframe"). A style that flips `slot.track.background` on `when checked` plus a transition rule produces a smooth crossfade with zero new machinery.

**The scoping model, normative** (mostly shipped, restated once so no passage implies its own model):

- **Placements and what each times:** top level = arrangement flips (shipped); style body = state flips on that style's wearers (shipped); node block = state flips on that node (shipped); **a `control` declaration body** = the control's **default feel** — the one new placement, riding the same scoped-directive grant as its anims (§5.2). The declaration is the default visuals; default feel belongs in the same place.
- **Across scopes, nearest wins:** node block > applied style > control declaration default > nothing = snap. Within one scope, shipped specificity is unchanged (exact pair > entering > leaving > bare).
- **Fraction glide** (§4.5) is timed by the **bare (endpoint-less) rule of the nearest control-scope placement** — the same chain. State-pair rules (`any -> dragging`) never address fraction. This is why an unstyled `render=gogo` Toggle still glides: Toggle's declaration ships a bare rule; styles and node blocks override it by the chain above.

---

## 3. Tier 2: slot fills *(D107 — designed 2026-07-14, built 2026-07-14 for widgets; fills on built-ins live with §5.7's backend since CC6 — Slider today, Button/Toggle with their rounds)*

### 3.1 Fill syntax (use site)

```gogo
Slider volume render=gogo, value=$vol, min=0, max=100, step=5 {   // slot fills need the Gogo backend (§5.7)
    slot thumb { Image texture="res://assets/icons/grip.png", 12x12 }
    slot label { Label text="Volume", font-size=$fontAppBar }   // fills are static author content —
}                                                               // the LIVE value label is the control's
                                                                // default content (live($value), §5.2)
```

- `slot <name> { content }` inside a control's or widget's braces fills the named slot with author content.
- The `slot` keyword is **mandatory** for named fills. Bare `thumb { ... }` is not a fill (it would parse as nothing valid in tree position; the D4 diagnostic suggests the `slot` form when the identifier matches a slot name).
- **Bare children fill the default slot.** `Card { Label ... }` (existing code) and `Button { Image ...  Text ... }` continue to work unchanged — plain children route to the default slot. This makes content-taking built-ins (Button) and widgets one rule, not special cases. A control that declares **no** default slot refuses bare children with the slot list as the fix (no guessing fix-it; the list is the fix-it): ``Knob has no default slot — content goes in a named slot: `slot track { … }`, `slot thumb { … }` ``.
- Filled content is authored freely (any tree), clipped to the slot's bounds. The control retains ownership of the slot's position and automatic geometry (§4); the fill only supplies what's *inside*.
- Filling a slot replaces that slot's **default content** entirely (no merging) — and *only* the default content: `slot` declarations nested inside it are **contract structure, not content**, and always survive a fill. Filling a slider's `track` deletes the track's default background and inserts your tick marks — `thumb` and `progress` remain, and automatic geometry keeps its targets (the tick-marks-in-a-track use case is the whole reason slots default to fillable). Ordering is deterministic: fill content renders **below** nested slots (the thumb draws over your tick marks — value display is never buried by a fill).
- Unknown slot name at a fill site → error listing the control's slots (D5).

### 3.2 Fillability

Slots are **fillable — all of them, always**. There is no closure mechanism (rejected, §12): tick marks belong in a track, textures in a progress bar, and nothing a fill can do threatens the contract — automatic ownership (§4.3/§4.5) protects driven properties, nested slots survive fills and draw above them (§3.1), and behaviors never touch nodes (§5.5), so no function depends on a slot's content.

### 3.3 One noun, two verbs

`slot` is the single noun for a control's pieces. Dotted path = restyle (`slot.thumb.size=12`, Tier 1, requires a contract: a `for=` style or a contract-bearing instance line); block form = fill (`slot thumb { }`, Tier 2, works anywhere). There is deliberately **no** separate "part" concept. `slot.` style paths on plain widgets are not permitted (widgets are styled via author-node paths as today); `slot.` paths require a contract to check against. This asymmetry is a rule, not a gap: filling is well-defined everywhere, restyling-by-slot is defined where a contract exists.

Slot paths are also legal directly in a contract-bearing **instance's prop list** — `Slider volume render=gogo, slot.thumb.size=10, value=$vol` — because a one-off tweak shouldn't demand a named style. (A theme type entry, being an instance-line prototype, is the third carrier — §5.8.) The instance knows its own control type, so checking is even more direct than `for=`; instance-over-style precedence follows the shipped rule (local properties override style properties); D3 and D15 apply unchanged. Fill blocks take **no** props — a fill is content, and letting `slot thumb size=10 { … }` set slot properties would be a second spelling of slot-property assignment. One spelling per operation: the dotted path assigns slot properties (style or instance line), the block form fills content, never mixed.

### 3.4 Named slots on widgets (extension of existing `slot`)

```gogo
widget Card {
    Panel background=panel, corner-radius=6, padding=12 {
        slot header { Label text="Untitled", font-size=14 }   // named, with default content
        slot                                                   // unnamed = default slot (existing form)
    }
}

Card {
    slot header { Label text="Run stats", font-size=14 }
    Label text="I fill the default slot", font-size=14
}
```

- Bare `slot` (existing, shipping) remains the unnamed default slot — and may now also carry default content in braces (`slot { <default content> }`), exactly like a named slot: replaced entirely when the instance provides bare children, symmetric with the named form.
- `slot <name> { default content }` declares a named slot; the braces hold its default content (may be empty: `slot actions`).
- At most one default slot per widget; duplicate named slots are an error.
- **Slot-line props** (applies to control slots too, §5.4): a slot is a **Panel-like region**. Its declaration line takes the placement vocabulary — `width`/`height`/`min-*`/`halign`/`valign`/margins: where the region sits (`slot header height=24, width=fill`) — and the box family — `background`/`corner-radius`/`border-*`/`padding`: the region's base look (`slot track height=4, background=$surface2` needs no content at all). Box props ride the region and **survive fills** (a fill replaces content, §3.1). Content props (`font-size`, `text`, …) belong to content, not the region — **restyling content IS filling**: bring your own node (`slot label { Label font-size=12 }`), and the routed error says so (``font-size styles the label's content, not the slot — fill it: `slot label { Label font-size=12 }` ``). When a slot declares a radial kind, its line also takes that kind's aperture parameters (`from=`/`sweep=`, §4.6). The **restylable set** (`slot.` paths) is this set **plus the motion vocabulary** — `scale`, `translate.x/.y`, `rotate`, `opacity` — everything except the driven property (D3) and content props. Motion words on slots follow the shipped fence: they are `when`-response vocabulary (`when dragging slot.thumb.scale=1.2`); static base-look motion stays an error.
- Fill-site checking: `slot heder { }` on Card → error listing Card's slots (`header`, default) with a rename fix-it.

---

## 4. Automatic geometry *(D108 — linear kinds + presence built 2026-07-14, CC3; arc/angle ride the Arc round. D134 2026-07-18: the coupling is fully spelled — `automatic=<kind>[(config)] by <driver> in <range>` — and `phase` is REMOVED: if the geometry displays data it's `automatic=`, if it's presentational motion it's an anim (`repeat=forever`); a timer is code writing a `$param`)*

### 4.1 Concept

Each control's contract marks certain slots as having **automatic** geometry: a logical property of that slot is driven by the control's value at runtime. Styles and fills cannot set the driven property; everything else about the slot is theirs.

The driving value is the control's **fraction**: `(value − min) / (max − min)`, normalized 0→1, animated through transitions (e.g. a toggle's fraction glides 0→1 on flip; mid-drag it holds intermediate values). Fraction is **not exposed to styles** — Gogo currently has no continuous state-driven properties. Fraction exists only as the input to automatic geometry (and in the debug overlay, §8.4). When continuous drivers land in a future version, fraction is already in the contract, ready to expose; nothing here needs redesign.

### 4.2 The automatic kinds (closed set)

| Kind | Meaning | Property owned | Examples |
|---|---|---|---|
| `length` | main-axis extent follows fraction | main-axis size (width or height per orientation) | progress fill |
| `position` | main-axis placement follows fraction | main-axis position | slider/toggle thumb |
| `arc` | the slot region's angular aperture (a wedge) follows fraction | the wedge — content-independent (§4.6) | radial progress |
| `angle` | the slot region's angular seat about its host's center follows fraction | the seat (placement, not rotation — §4.6) | knob thumb |
| `presence` | slot content exists iff a driving state is true; enter/exit transitions honored | tree presence | checkbox mark |

Kinds are **logical**, not property names: `length` maps to `width` on a horizontal slider and `height` on a vertical one, so one style serves both orientations without mentioning either.

**The coupling is fully spelled (D134)** — the kind (config in its parens for the radial kinds, §4.6), the driver after `by`, the range after `in`; every piece required, nothing defaulting invisibly:

```kdl
slot fill  automatic=length by $value in $min..$max
slot thumb automatic=position by $value in $min..$max
slot mark  automatic=presence by checked
slot wedge automatic=arc(from=-225, sweep=270) by $value in $min..$max
slot dot   automatic=angle(from=-225, sweep=270) by $value in $min..$max
```

The fraction kinds drive `by $param in <lo>..<hi>` — v1 accepts exactly `$value in $min..$max` (multi-value controls and custom ranges arrive with their first consumer, the range slider; the grammar already carries the names). `presence` drives `by <state>` — a bool has no range. `by`/`in` are keyword-anchored clauses (repeat's clause grammar), comma-free, `by` before `in`; a long coupling wraps at a trailing comma like any line.

`arc`/`angle` — and the `Arc` primitive their default content wants — ship in v1, specified in §4.6; the Knob (§10 E) is their acceptance case. There is no clock kind and no clock driver (D134, `phase` removed unbuilt): a spinner, a caret blink, an indeterminate marquee display no data — they are `repeat=forever` anims (the shipped WP indeterminate is literally a forever storyboard gated by a state); geometry that loops AND means something is a value your code writes on a timer.

The set is **closed and compiler-known**. This is what makes styles checkable: the compiler knows exactly which properties each kind owns and what "displays the value" means. The set is extensible only by future language versions. Advanced Gogo documentation states this as policy, with "propose a kind" as the path and future continuous drivers as the general relief valve.

### 4.3 Ownership enforcement

Assigning an owned property is a load-time error with a delete fix-it, worded from the user's chair (D3):

```
error: slot.thumb.x moves by itself — the slider drives it from the value
  --> styles/audio.gogo:3
   = you style the thumb's look (size, background, radius…); the slider handles where it sits
   = fix-it: remove `slot.thumb.x=...`
```

Checked wherever the property is set: `for=` style declarations (write time), instance-site slot-path assignments (§3.3), and inline `when` clauses.

### 4.4 Semantic lint — retired

D12 ("style 'Foo' never displays the slider's value") is **retired before shipping** (2026-07-14): §4.5 made it uncomputable by construction. Styles cannot restructure (D8) and cannot touch driven properties (D3), so a control's automatic slots display the value *no matter what a style does* — the only way left to blind a control is semantic (a transparent progress color), which contract data cannot detect and which is an author's explicit act needing no lint. Tombstoned like D6; the ID is not reused.

### 4.5 Channel assignment for automatic geometry

**The channel: layout.** Automatic geometry writes the **layout channel**. The driven property (thumb main-axis position for `automatic=position`, progress main-axis extent for `automatic=length`, subtree presence for `automatic=presence`) is the value's spatial *truth*, not decoration: hit testing must find the thumb where it appears, the behavior interface's per-slot input routing ("drag began on thumb") must test against the fraction position, and clipping/overlap must agree with what's on screen. The motion channel is visual-only by shipped doctrine; putting truth there would either break that doctrine (hit testing following offsets) or produce a thumb whose hit rect sits at base while its pixels sit at fraction. So: layout. Consequence for the player: the automatic driver never writes offset transforms, and the anim player never writes a driven layout property — the player's ownership of the motion channel, arm/disarm included, is untouched by this feature.

**ONE WRITER, per stage.** The driven property has exactly one writer — the runtime's automatic driver — with the fence enforced per writer kind: styles statically (D3); fills and inline props statically (D3's fill-site clause); an anim track or arrangement targeting a driven property is a load-time error (new trigger under D3, same wording family: "moves by itself — the slider drives it"; a control's own scoped anims are the one place anim tracks can address slots at all, §5.2); behavior scripts by construction (the interface hands the script no node access, §5.5). The full pipeline is one writer per stage: the behavior writes the value → the nearest control-scope **bare** `transition` rule (§2.4's chain: node block > applied style > control declaration default) interpolates **fraction** → the driver maps fraction to the driven layout property. Fraction glide (a toggle's 0→1, a drag settle) is tweened upstream in fraction space: the transition machinery writes fraction, never layout; the driver remains the sole layout writer. No rule anywhere in the chain → fraction snaps. (This also answers "who times fraction" — one timing mechanism in the language.)

**Coexistence with `when dragging slot.thumb.scale=1.2`.** Composes with zero shared writers, because the two effects ride different channels: the driver sets the thumb's layout position (*where it is*); the `when` flip's scale tween rides the motion channel exactly as any `when` flip does today (player-owned, arm/disarm intact), scaling about the thumb's pivot at its layout position. Layout says where; motion decorates there. A style may freely animate any non-driven property of an automatic slot — scale, background, radius — through the normal `when` + `transition` path; only the driven property is fenced.

**Pinned at build (D108, 2026-07-14):** (a) a mid-gesture value write lands on the driven geometry DIRECTLY (the pointer is the truth — no glide); every discrete change (code write, key step) glides through the bare-rule chain. (b) The driver re-asserts from the held fraction when the host or a position slot resizes — anchors carry the fraction itself, so parent resizes re-derive for free; only the seat offsets (which embed the slot's own width) need the re-assert.

**Driver arm/disarm (mirror of the player's rule, one stated asymmetry).** The driver is armed only while fraction is changing (a drag in progress, or a transition glide mid-flight) and disarmed at rest — zero per-frame cost for idle controls, mirroring the player's economics. The asymmetry to state explicitly: player disarm zeroes offsets; **driver disarm holds** — the driven layout property keeps the last written value, because it is truth, not displacement. At rest, the thumb's layout position simply *is* the fraction position.

**Validity constraint: driven properties must be relayout-local.** Writing layout per frame during a drag is acceptable only if the write cannot ripple into parent measurement. Contract-validity rule, checked when the control declaration loads: a slot with `automatic=position` or `automatic=length` must sit in a host where the driven property does not feed parent measurement (free-placement hosts qualify; size-to-children containers on the driven axis do not). Violation is a load-time error on the control declaration naming the offending host ("thumb's position is automatic — it can't live in a host that measures children on that axis"). The v1 built-in composites satisfy this by construction; the rule exists so user controls can't declare a contract whose driver would thrash layout.

**Per-kind channel table** (completes §4.2):

| Kind | Channel | Driven property | Fenced writers |
|---|---|---|---|
| `length` | layout | main-axis extent of the slot | styles, fills, tracks, arrangements |
| `position` | layout | main-axis position of the slot | same |
| `presence` | layout (tree) | subtree presence of the slot's content (its `visible`-equivalent; a style writing `visible` on a presence slot is D3) | same |
| `arc` | layout | angular aperture (wedge) of the slot region — clips drawing AND hits (§4.6) | same |
| `angle` | layout | angular seat of the slot region about its host's center (§4.6) | same |

### 4.6 Radial kinds and the `Arc` primitive *(D110 — built 2026-07-14, CC5: wedge fan + CLIP_CHILDREN_ONLY, seat writes, slot_aperture, Arc; knob.gd's math is the live fixture)*

The radial kinds ship in v1; their acceptance case is the Knob (§10 E). They have no built-in consumer — they ship with the `control` construct, for user controls (§5.7 "Interactions").

**Aperture as kind config (D134).** `arc` and `angle` take their angular aperture as REQUIRED named arguments in the kind's parens: `from=` (degrees, start angle; Godot's screen convention — 0° = right, clockwise positive) and `sweep=` (degrees of travel). `slot progress automatic=arc(from=-225, sweep=270) by $value in $min..$max`. There are no defaults (nothing about a coupling defaults invisibly), and the aperture cannot be misplaced — only the radial kinds take config, and `from=`/`sweep=` as slot-line props error with the parens form spelled. The behavior reads the same aperture via the `slot_aperture()` value query (§5.5) — input mapping and visuals share the **declaration** as their single source of truth; a behavior never carries its own sweep constants.

**`arc` drives a wedge, not a node.** The slot *region* is clipped to the angular wedge `[from, from + fraction × sweep]`; whatever content fills the slot shows through it. This keeps automatic geometry content-independent — exactly as `length` drives the region's extent, never the content's width — so fills stay safe: fill the progress slot with a Gradient ring and the wedge still reveals it by value. Drawing and hits both clip to the wedge (§4.5's layout-truth rule: the hit area is what's visible).

**`angle` drives the region's seat.** The slot region is placed on the circle centered in its host at angle `from + fraction × sweep` — angular *position*, not rotation (the thumb keeps its own orientation; `rotate=` stays a motion word).

**Relayout-local by construction:** wedge and seat act inside the host region's granted rect and never feed parent measurement, so §4.5's validity constraint is satisfied inherently.

**The `Arc` type** (new primitive, gogo_arc.gd — capitalized: addressable, stylable, animatable): draws a ring segment. Props: `color=`, `thickness=` (stroke, epx), `from=`/`sweep=` (degrees; defaults 0/360 — a full ring). Radius self-derives from the box, Circle's rule. Spelling note: `thickness=`, **not** `width=` — `width` is the sizing key and stays one spelling of one concept (an earlier draft's `Arc width=4` was that collision). The default content of an arc slot is typically a bare full ring: the wedge does the revealing, so the aperture is written once, on the slot line, never duplicated into content.

**Cost stated:** the wedge is not a Godot rect clip — slot content renders through an angular mask (draw-based or shader, implementer's pick; the pinned behavior is that pixels and hits agree with the wedge either way).

---

## 5. Tier 3: the `control` construct *(D108 — §5.1–§5.6 built 2026-07-14, CC3, incl. the condition→state migration and the §8.4 contract overlay; §5.7/§5.8 built 2026-07-14, CC6 — Slider first)*

### 5.1 Purpose and boundary

`control` = `widget` + behavior script + declared states + slots with automatic geometry. Use `control` when input must be turned into a value/state machine; use `widget` for pure composition. The boundary is enforced and taught by D7: **`script` is mandatory in `control`.**

### 5.2 Syntax

```gogo
control Knob $value=0, $min=0, $max=1, $step=0 {
    script "res://controls/knob.gd"
    state_builtin hover, focus, disabled
    state dragging
    signal drag_started, drag_ended
    transition duration=0.15, easing=cubic_out  // default FEEL: times flips + fraction glide (§2.4)

    Circle color=$surface2 {
        slot track 48x48 {
            slot progress automatic=arc(from=-225, sweep=270),
                    by $value in $min..$max {
                Arc thickness=4, color=$accent  // a full ring — the wedge reveals it (§4.6)
            }
            slot thumb automatic=angle(from=-225, sweep=270),
                    by $value in $min..$max {
                Circle 8x8, color=$text0
            }
        }
        slot label { Label text=live($value) }  // live read of a contract prop
    }
}
```

- `control <CapitalizedName> <$params with defaults> { ... }`. Capitalized like every addressable type (the capitalization law). Use sites, `for=Knob`, and fills work exactly as for built-ins.
- Header `$param=default` declarations define the control's public props — the **same grammar as `widget` lines** (one spelling of "declare a parameter"; a control is a widget plus behavior, and its declaration line reads like one). Bare `k=v` on the line keeps its widget-line meaning (bakes a property onto the root) and `.Style` applies to the root. Use sites stay unmarked either way: `Knob gain value=$gain` — instance props matching declared params are params, exactly as for widgets.
- The **body tree is the default visuals**. There is no separate "default style" mechanism; the declaration is the default. Tier-1's scaffold fix-it (D8) materializes a `for=` style from it.
- `slot <name> [size/props] [automatic=<kind>[(config)] by <driver> [in <range>]] { default content }` inside the body declares the control's slots, their defaults, and their automatic couplings, in one place (D134 — the coupling fully spelled, §4.2).
- Controls may declare **scoped `anim`/`style`/`transition` directives** alongside the body tree — `anim`/`style` exactly as widgets do (they see the per-instance names, invisible outside), and a scoped `transition` rule is the control's **default feel** (§2.4: it times state flips and fraction glide when no style or node block overrides — an unstyled composite still eases). Inside a control's own scoped anims, track paths additionally accept **`slot.` targets** on the motion channel — `slot.thumb.translate.y from=-4, to=0` (a release-bounce flourish) — the scope knows the contract, so the addressing is checked like any slot path. This is the **only** place anims address slots: from outside, a control's internals stay invisible (the widget rule), slot addressing is scoped-or-nothing. Driven properties stay fenced (§4.5's D3 trigger — this grant is what makes that fence reachable).
- Declaration-order rule: `script`, `state_builtin`, `state`, `signal` lines precede the tree. (Convention; the parser accepts any order among the four lines.)

#### Live reads: `live($param)`

Default content shows a contract prop by saying so: `Label text=live($value)` re-resolves on every prop change (drag, code set) — it is how a `label` slot's default content shows the value. (Fraction glide animates *geometry*, not the value — the value steps discretely, so a `live($value)` label shows the new value immediately while the thumb glides, §4.5.) `live` joins the closed call-form set, legal in exactly one position: prop values inside a control's default tree.

The scoping is the callee's checkable contract (D17):

- A **bare** `$param` read in a control's default tree is a load-time **error** with the wrap fix-it — contract props are runtime-mutable by nature, so a pasted read is a latent frozen-value bug, and `live()` strictly subsumes static (a prop that never changes simply never fires; the §4.5 armed-only-while-changing economics).
- Bare theme refs (`color=$accent`) stay static and legal as shipped.
- `live($accent)` on a non-param errors listing the params; `live()` outside a control's default tree errors and routes here (fill-site binding: §9 if it ever itches).
- `bind` sits in the redirect hints (``did you mean `live()`?``) for WPF muscle memory.

The widget asymmetry is deliberate, stated once: in a `widget`, `$x` is a construction argument (static, no error); in a `control`, `$x` is a live prop — and the spelling makes you say so. Remaining fences: `live()` targets only non-driven properties (D3 owns the driven ones); a binding is a **writer**, so a scoped `when` driving the same property is a two-writers load-time error (the shipped ONE WRITER doctrine); and this is data binding by author-owned nodes, not the deliberately-absent continuous *style* drivers (§4.1) — trees have bindings, styles have property bundles.

### 5.3 States: `state` and `state_builtin`

Two keywords, one mark, pointing at the shelf (author-owned is the unmarked case throughout Gogo):

- **`state <names...>`** — custom states. Semantics defined by the control's script, which raises/lowers them. Free-form names.
- **`state_builtin <names...>`** — opt-in to runtime-maintained states from the closed built-in set. The runtime maintains them (with all the hard edges handled once, correctly: hover on touch devices, hover-during-capture, focus order, input blocking); the script may **read** but never write them.

Built-in state set (v1): `hover`, `focus`, `focus-within`, `pressed`, `disabled`, `checked` — the shipped condition set, spelled exactly as shipped, in the two classes of §2.3: **universal** (`hover`, `focus`, `focus-within`, `disabled` — runtime-maintained on every control, always styleable, no declaration needed) and **contract** (`pressed`, `checked` — capabilities, opt-in via `state_builtin`). *(`focus-within` joined 2026-07-17, D129 — see the focus bullet below.)*
- `hover` (universal): pointer-over, with policy **hover follows capture** — while a drag has captured a slot, that slot remains hovered even if the pointer strays (prevents mid-drag flicker). Global policy, **and a behavior change for natives** (preamble): shipped hover clears on `mouse_exited` unconditionally, mid-drag included (gogo_states.gd wires the exit signal bare) — every backend adopts follow-capture in this round, so the two backends can never style-flicker differently mid-drag (§5.7 parity, styling side). No hover on touch.
- `focus` (universal): styleable everywhere; for a user control, *declaring* it additionally hooks the control into the focus system (tab order, `focus_mode`, focus visuals — the §5.5 participation hooks). A control that never takes focus simply never enters the state. **Semantics since D129 (2026-07-17): VISIBLE-FOCUS** — the state holds while the control is focused AND an indicator is warranted (focus arrived by key/gamepad, a key pressed while focused upgrades, a pointer press downgrades; caret controls always indicate). One spelling with the correct-for-styling meaning — the CSS :focus-visible model given the short name, the same move `hover` made in D75 (the raw engine signal was wrong for styling; the short name kept the corrected semantics). Raw focus is data, not a style hook: code-behind reads `has_focus()`; no `focus(any)` spelling ships until a real consumer appears. Behaviors' `key_action` routing stays on ENGINE focus — keyboard reaches a focused control whether or not the indicator shows.
- `focus-within` (universal, D129): holds while the focus is on the node or any descendant (CSS `:focus-within`, WPF `IsKeyboardFocusWithin` — both parent traditions agree on "within", and the assist one-liner teaches the direction at first contact). Visible-focus flavored like `focus`. The container-highlight state for key/gamepad UIs: a settings row lights while its inner slider holds focus; the active pane brightens. Ranks below every other state on the ladder (ambience under input feedback).
- `pressed` (contract): pointer-down-on-me with capture semantics (distinct from a custom `dragging`, which typically adds movement).
- `disabled` (universal): set via the `disabled` prop by users; enforcement is systemic — input is blocked **before** the script sees it, and it's reported to accessibility. Meaningful on any control (shipped gogo errored it off-button — behavior change, preamble). Universal means the **`disabled` prop exists on every node** — that prop-surface expansion is part of this feature's cost, stated here. On a node that takes no input (a Label), the prop's only effect is styling — `when disabled` responses fire, nothing needs blocking (the dim-the-caption-with-its-control use case).
- `checked` (contract): the persistent on/off condition — shipped today on the button family (`Button toggle=true`, `Check`, `Radio`; polled, so code writes restyle). A user control opting in gets the standard flip (a completed press toggles it; code may set it) — the opt-in brings the press/capture tracking it needs **internally**; `pressed` itself stays undeclared, unstyleable and unreadable unless listed in its own right. Collection-grade exclusivity (`selected`) is future work (§9.1).

Rules:
- **Built-in state names are reserved across both keywords.** `state hover` → error, fix-it moves it to `state_builtin` (D9). This holds under future growth of the built-in set: an upgrade that adds `selected` to the set makes any existing custom `state selected` a loud error with a rename-or-migrate fix-it — a forced, explicit versioning conversation, never a silent re-semanticization.
- Opt-in is required for **contract states**, and for **script read-access** to universal ones (no automatic grant): the declaration list plus the universal set is exactly what the explain pane and `for=` completion show, and some declarations carry obligations (focus participation, §5.5). Declaring a universal state does exactly two things: grants the script read-access, and lists the state in the contract (explain pane, completion). **Universal states are always styleable; script read-access still requires the `state_builtin` grant** — visual response and behavioral response are different privileges, and the grant is the one with obligations attached (a fence on purpose, not a bug). A `for=X` style responding to a universal state X doesn't declare draws advisory lint D16 (add the declaration so the contract lists what the style responds to), never an error. Lint: if the script reads raw pointer-over ad hoc, suggest `state_builtin hover`.
- From the style side, custom and built-in states are indistinguishable: same `when` syntax, same checking, same completion.
- **Priority when several states hold** (extends the shipped chain): arrangements < focus < hover < checked < pressed < **custom states** < disabled. Custom states are more specific facts than the generic capabilities — `dragging` implies pressed-plus-movement, so it beats `pressed` when both hold (a thumb mid-drag showing its pressed color instead of its drag color would read as a bug). Among multiple customs, the `state` line's declaration order IS the priority order, later wins — author-owned placement, zero new syntax. `disabled` beats everything, always, customs included: a behavior can't out-rank the input-blocker.
- **Shipped-collision note (the deliberate rename):** shipped gogo's umbrella word is *condition* (`when <condition>`; ``unknown condition`` errors), and `state` is a retired top-level directive whose error teaches `arrangement`. This spec adopts **state** as the one user-facing word: docs and error strings migrate condition → state in the same round, and the retirement error becomes position-scoped — top-level `state` still redirects to `arrangement`, while `state` inside a `control` block is this declaration. One word for the concept, migrated loudly; the two words never coexist.

### 5.4 Slots in `control` declarations

- Every `slot` in the body is part of the control's contract: restylable via `slot.` paths in `for=` styles, fillable at use sites — always (slot closure was rejected, §12).
- `automatic=` on a slot declares its coupling, fully spelled (§4.2, D134): the kind (radial config in its parens, §4.6), `by` the driver (`$value` for the fraction kinds, a state for `presence`), `in $min..$max` the range (fraction kinds only). Every piece required; old spellings (`presence(checked)`, slot-line `from=`/`sweep=`, bare `automatic=length`) error with the full form spelled.
- Slot-line props follow §3.4's rule: placement vocabulary plus the box family; content props route to content.
- Slot names must be unique within a control. Author nodes inside slot default content may freely use any names, including slot names — no collision is possible, because slot addressing is always keyword-anchored (`slot.thumb.` in styles, `slot thumb {` in fills).
- A control author who wants Button-like plain-children ergonomics declares a bare `slot` in the default tree, exactly as widgets do — the capability is spelled, never implied (a control without one refuses bare children, §3.1).

### 5.5 `script` and the behavior interface

- `script "<path>.gd"` — **mandatory** (D7). Points at a GDScript file implementing the behavior interface. The keyword names the artifact (Godot's own word); "behavior" remains prose vocabulary for the concept. Note for future spec work: inside `control`, `script` specifically means "implements this control's behavior contract" — if widget code-behind attachment sugar is ever added, decide explicitly whether it shares this keyword. v1 behaviors are **GDScript-only**. Only the behavior author writes GDScript — app code talks to controls through props, bindings, states, and signals, all reachable from C# like any node surface; since v1's contract-bearing controls are the built-in composites gogo itself ships (§5.7), an app port likely writes zero `.gd` behavior code. C# behaviors are a stated future (§9), not a discovered gap.
- No convention-based lookup (no implied `knob.gd`): every relationship in Gogo is spelled, and the explicit path is what makes the explain pane, scaffolding fix-its, and hot-reload file watching cheap.
- No inline script bodies: declarative surface in `.gogo`, imperative logic in `.gd`, connected by explicit reference (matches the existing relay-vs-code-behind doctrine).

**Behavior interface (runtime API — spec the following surface):**
- Receives routed input, pre-hit-tested per slot (the script learns "drag began on thumb", not raw coordinates against unknown trees). Routed press/drag events carry the **control-local position and the control's size**, and the behavior may query **slot geometry as values**: `slot_rect(name)` (Rect2, control-local — a knob's pointer-angle needs the track's *center*, which control size alone can't locate) and `slot_aperture(name)` (the slot line's `from`/`sweep`, §4.6 — input mapping and visual aperture share the declaration as their single source). Values, never nodes — the invariant below holds exactly. Input to a `disabled` control is blocked before reaching the script. This routing is the beginning of the gogo gesture API and is bound by the D103 round's recorded obligation: gestures attach only via the gogo API, only to interactive nodes — violation is a first-run error naming the node and the fix.
- Reads props; calls `set_fraction()` / value setters (step snapping location: behavior; `step=0` means **continuous** — no snapping, the Godot Range convention the examples use); raises/lowers **custom** states; reads (never writes) built-in states; `emit()`s only the names the control's `signal` line declares (§5.2) — an undeclared `emit` is a first-run error (the D103 enforcement posture), and `on:<name>=` wiring at use sites is load-checked against the declaration, so signal typos join state typos as write-time catches. Value changes are not a signal: bindings own value flow; signals are for discrete events (`drag_started`, not `changed`). The shipped widget `signals` flag — *which node* exposes signals — is untouched; `signal` in a control block declares *which names* exist. Declared-and-checked vs the widget's ad-hoc `emit()` is deliberate: it is what `control` is relative to `widget`, stated once. Contract signals materialize as **real Godot signals on the control's root**, so shipped name-based auto-wiring (`gain` + `drag_started` → `OnGainDragStarted`) works on controls with zero new machinery — a requirement, not an implementation choice.
- **Never touches nodes.** Geometry flows only through declared automatic couplings. This is the invariant that makes every checked style safe under every behavior. Enforce via the interface shape (no node access is handed to the script), not via linting.
- Must cover: focus participation hooks (when `focus` is opted into), accessibility role/value reporting, keyboard mapping. **Keyboard reaches a control only through focus**: a behavior implementing the keyboard hook in a control that doesn't declare `state_builtin focus` is silently keyboard-dead — advisory lint D18 suggests the declaration.
- Slot-level hit shaping (`hit=`) is **parked in §9** — zero v1 consumers (Slider, Button and Toggle are rectangular; the first real customer is the color-wheel port round). When it comes, hit areas beyond the rect are declarative slot props, never script logic — the behavior still just hears "drag began on thumb".
- Hot reload: `.gd` behavior scripts hot-reload alongside `.gogo` files. Feasibility note: reloading GDScript in a running game is engine territory with real limits (editor-driven script reload is not guaranteed in exported/runtime contexts) — v1 may scope behavior hot-reload to editor/debug sessions; the bar is stated here so it isn't discovered later as a silent acceptance criterion.

### 5.6 What controls inherit for free

Because tiers 1–2 and all tooling are driven by contract data, a declared `control` automatically gets: `for=Knob` write-time-checked styles, `slot` fills with unknown-name errors, ownership errors on automatic properties, closed `slot.` completion, the explain pane for Knob, and debug-overlay support. **Zero per-control tooling work is acceptable; if any of these requires per-control code, the implementation is wrong.**

### 5.7 Built-in contracts and the `render` prop *(D111 — Slider composite + `render=` built 2026-07-14, CC6: controls/slider.gogo dogfoods §5, conformance harness with the native oracle, D15 fix-its live. Button composite RETIRED 2026-07-14, D112 — see the note under the v1 table. Toggle built 2026-07-15, CC8/D113 — checked DERIVES from the fraction on fraction-bearing controls, the completed-press flip stays on fraction-less ones)*

#### Two backends

Shipped built-ins are raw Godot nodes (`Slider` **is** an `HSlider`; REFERENCE "Vocabulary"). They have no slots and cannot be restyled. This spec does not change that: an unmarked built-in instance is **native pass-through** — instantiated exactly as shipped, byte-identical for existing projects; acceptance example A ("existing code unchanged") is true by construction. A built-in instance gains its Gogo contract (slots, `for=` styles, fills, automatic geometry, custom contract states) only when it opts in:

```gogo
Slider volume .ThinSlider render=gogo, value=$vol, min=0, max=100
```

`render=gogo` swaps the instance to the **Gogo composite implementation** of that control — a control built with §5's own machinery, implementing the same contract `for=Slider` checks against. There is no automatic backend selection; `render=native` is the spelled opt-out against an outer default (§5.8) — and written where nothing overrides it, it is **legal and silent**: a defensive pin (an `as HSlider` instance pinned *before* a future theme entry lands) is the migration pattern §5.8 recommends, and redundant `render=` values are uniformly silent (the redundant-`interactive` precedent). `render` is a contextual prop on contract-bearing built-ins only — on other nodes it is an unknown-prop error, and on a user control (always Gogo-implemented) `render=gogo` is a **silent redundant-true** (the redundant-`interactive=true` precedent).

#### The D83 carve-out

The shipped node-type-stability law says a property edit must never change what a node IS. `render=gogo` is the **one sanctioned exception**: it is type grammar riding prop syntax — the prop names the node's *implementation*, the change is visible at the instance, greppable, and consented (D15's note states "`as HSlider` stops working on it" at the moment of decision). The law's target was *spooky* class change — a style or theme flipping classes at a distance; an explicit instance-line declaration is the opposite of spooky. No future prop may cite this exception without the same three properties: instance-local, consequences stated in the diagnostic, opt-in only.

#### The routing error (D15)

Using a contract feature on a native instance is a load-time error with fix-its, following the ladder's routing pattern:

```
error: `.ThinSlider` restyles this slider's slots — Gogo has to draw it
  --> screens/main.gogo:41
   = note: native sliders can't be restyled; `render=gogo` swaps in Gogo's
           slider (looks the same until you style it, and `as HSlider` stops
           working on it)
help: opt this instance in
   |
41 | Slider volume .ThinSlider render=gogo, value=$vol, ...
   |
```

Triggers: any `for=X` style applied to a native instance; a `slot <name> { }` fill on one; an inline `slot.` assignment on one (§3.3); a plain style whose `when` uses a contract state **only the composite raises** (`dragging` on a native Slider — the native node runs no behavior to raise it; a state the *type* doesn't declare at all stays D14 — the keying is "who raises it", not "custom vs built-in", so it stays correct whatever the contract tables say). Fix-its, cheapest exit first: a `for=` style that touches no slots and responds only to universal states and the control's shipped conditions gets **drop `for=`** first — the style then applies natively (this guidance lives here, where the choice is real; a declaration-time "drop `for=`" lint would ping-pong with D2's "add `for=`"). Otherwise (and always, as the second option): **insert `render=gogo`** — at **confirm-level applicability, never `safe`**, because it changes the node's class. The parenthetical consequences in the note are normative wording: visual continuity and the class change are stated at the moment of consent.

**Rationale (rejected alternative, recorded):** automatic backend selection — the compiler flipping the backend when a contract style applies — was considered and rejected: a style application silently changing an instance's node class breaks `as HSlider` code with no diff at the instance, i.e. spooky action compensated by tooling. Explicit opt-in keeps the class of every node readable in source and reduces the design to one prop, one value, one error.

#### Scopes

- Instance (above).
- Widget-internal: a widget author may set `render=gogo` on a node inside a widget body; widget users are unaffected and unaware. Required in v1 — widgets that style their internals must not leak the opt-in to callers. (A widget-internal `render=` IS an instance line — the one inside the widget body; it is not a third precedence tier.)
- Theme-level (all instances of a type at once): **in scope — §5.8.** Nearest scope wins: an instance line (in a view or inside a widget body, the same thing) beats the theme entry — two tiers, no more.

Docs posture (guard against cargo-culting): `render=gogo` is documented as "what the D15 fix-it inserts," not as a setup step. Setting it on an unstyled instance is legal and un-linted (a stable class for code-behind is a valid reason).

#### Visual continuity (conformance)

The composite's default visuals render the control's Godot theme resources — the same styleboxes and grabber/icon assets the native node draws — expressed as the contract's slots. Flipping `render=gogo` on an unstyled instance must be near-pixel-identical to native; styles then override piecemeal on top of native-looking parts. Contract slots with no native counterpart (Slider's `label`) default to **empty content** for the same reason. This is a requirement, not guidance: the no-visual-cliff property is what makes the opt-in cheap enough to require.

#### Behavior parity (conformance)

Flipping backends may change visuals only, never input semantics: keyboard, wheel, page-step, drag, RTL, focus traversal, the visible-focus indicator (D129 — both backends gate it through the same modality service), and screen-reader exposure (AccessKit role/state reporting — wired 2026-07-17: the root consumes the behavior's `accessibility_role()` in its NOTIFICATION_ACCESSIBILITY_UPDATE pass, reporting role, value/range for fraction controls, `checked` where declared, the disabled flag, a `label`-slot labeled-by relation, and INCREMENT/DECREMENT/CLICK reader actions routed through the same `key_action` vocabulary keyboard uses. Composite internals hide from the reader tree — root-marked, self-stamped FLAG_HIDDEN per node pass — so a composite reads as ONE element, the native-oracle shape). Enforced by conformance tests — identical scripted input sequences against both backends must produce identical value/signal traces, with the native node as the oracle. Explicitly included: the composite exposes a signal surface compatible with the native class's, so shipped **auto-wiring** (`OnPaintButtonPressed`) binds identically across backends. Universal-state mappings are part of parity: native classes back the universal states with their own machinery, and the mapping is per-control and named in its conformance tests (`BaseButton.disabled` for buttons; `Range.editable` backs `disabled` on a native Slider — Godot has no `disabled` there). These tests ship with each rebuilt control; a control without them does not ship its contract.

#### Sequencing and the v1 contract set

Built-ins are rebuilt as Gogo controls using the §5 machinery itself (dogfooding; §5.6's zero-per-control-tooling promise), **one control per round**: Slider first — the oracle/conformance harness is built once there and reused — then Button, then Toggle. Theme-level defaults (§5.8) ship with the Slider round: they are what turn Tier 1 from per-instance ceremony into app policy, and they need exactly one contract to exist. Cut line, if a round must shrink: radial kinds (§4.6) drop first, then §5.8, then conformance-harness breadth (RTL/wheel staging) — the contract data model and the diagnostics catalog never cut. v1 contract set:

| Control | Slots | Contract states | Contract signals | Why v1 |
|---|---|---|---|---|
| `Slider` | `track`, `progress` (`automatic=length`), `thumb` (`automatic=position`), `label` | `dragging` — deliberately **not** `pressed` (shipped parity: `pressed` on a slider errors today, and stays an error) | `drag_started`, `drag_ended` | the port needs it; Tier 1's headline |
| `Button` (incl. toggle mode) | default slot (existing children behavior), `leading`/`trailing` | `pressed`, `checked` (toggle mode) | — | most-styled control in the port |
| `Toggle` (new; replaces `toggle=true` as a styling target) | `track`, `thumb` (`automatic=position`) | `checked`, `dragging` | `drag_started`, `drag_ended` | kills the Button-mode special case; exercises fraction-driven position |

The states and signals columns are **normative contract data** — D4's "its states:" lists, D14/D15 checks, `for=` completion and the explain pane all render exactly these (plus the universal states, which every control has).

**Button's composite: RETIRED (2026-07-14, D112 — user decision, CC7 cut).** Building it, we re-measured the row's "why" against shipped gogo and found nothing left to buy: gogo Buttons already take free-form children (the default slot's promise, verbatim), plain styles already cover `pressed`/`checked` (native-raised) and the whole box family, and CC2's widget slots already give named fill points the composite would have added (`widget IconButton { Button { Row { slot leading; slot } } }`) — with checking and defaults, one level up. What remains of the row: the contract keeps its STATES half and drops its slots — `for=Button` is **write-time state checking that applies natively** (a slot path in one dies at the declaration with the widget-wrap route); `render=native` on a Button stays the legal silent pin; `render=gogo` errors terminally ("has no Gogo renderer — its children are free-form content already"). D15 can no longer fire on Button. Revisit only if a port need surfaces that widgets cannot express; the D83 carve-out then applies unchanged. *(2026-07-17, D128 — the "whole box family" claim had one crack: box keys took over the button's per-state pigment as a side effect, killing press feedback (`Button padding=10`, user-caught). Fixed native-side by splitting shape from pigment in the box machinery — shape keys reshape per-state theme copies, `background=` remains the takeover. The retirement rationale stands; no composite needed.)*

**Two signal layers**, stated once so implementers don't resolve the tension by coin flip: the contract signals above are what `signal` declares, `emit()` may raise, and checking governs; the **Godot-compatibility surface** (`value_changed`, `pressed`, `toggled`, …) is what behavior parity and shipped auto-wiring bind to. The composite keeps both; only the contract layer is checked vocabulary. Value flow stays off the contract layer by doctrine (§5.5) — it lives in props/bindings, and on the compatibility surface where Godot put it.

**Toggle has no native backend**: it is Gogo-implemented always — `render=gogo` on it is the silent redundant-true (the user-control clause), `render=native` on it errors (``Toggle has no native backend``), and D15 can never fire on it. *Unsatisfiable* is different in kind from *redundant* — this error stays loud while redundant `render=` values stay uniformly silent; the two rules don't conflict.

All other built-ins remain native-only in v1; `render=gogo` on them is an error naming the v1 set (``Check doesn't have a Gogo renderer yet — controls with one: Slider, Button, Toggle``). Built-in contract relationships (slider↔progress etc.) move to §9, empty until their members exist.

#### Migration story for `as HSlider`

Unchanged instances: class unchanged, forever, by construction. Opted-in instances: the class change is visible at the instance in source and stated in D15's note. Recommended pattern in docs: code-behind addresses controls through the Gogo surface (value, states, signals), not node class. Long-term: because opt-in is greppable, composite adoption is measurable across real projects; any future default flip (composite-by-default, native as legacy opt-in) is a major-version decision made from that data, using the loud-migration machinery (the reserved-names precedent).

#### Interactions

- Hot reload: a `render=` edit rebuilds the node across backends preserving value and focus (extends the existing tree-rebuild rule by "carry the value across").
- §4.2/§4.5: the v1 **built-in** set needs only `length`/`position` (plus `presence` when `Check` joins). `arc`/`angle` ship with the `control` construct for user controls (§4.6 — the Knob is their acceptance case; no built-in consumes them). There is no clock kind (D134): loops that display no data are `repeat=forever` anims.
- D103: composites join the interactive-by-default set, respect `interactive=false`, and participate in the 44-epx touch lint and input-overlay tinting, identically to the native nodes they replace. The behavior interface's routed input is bound by the recorded gesture-API obligation (§5.5).

### 5.8 Theme-level type defaults (`theme { Slider ... }`) *(D111 — built 2026-07-14, CC6; entry props exclude placement — recorded implementation restriction, see PORTING)*

Per-instance ceremony (`.ThinSlider render=gogo` on every slider) is right for one-offs and wrong as app policy. A **type-keyed theme entry** sets defaults for every instance of a control type:

```gogo
theme {
    Slider .ThinSlider render=gogo   // every Slider: ThinSlider look, Gogo backend
    Button .AppButton                // plain default style — native Buttons keep working
}
theme dark {
    Slider .ThinSliderDark           // variant overrides the LOOK half only —
}                                    // render= is variant-invariant (default block)
```

- **Shape: an instance-line prototype.** A type-keyed entry is `Type .Style prop=..., ...` — exactly the vocabulary of an instance line, minus name and children. It lives where the shipped control defaults (`font-size`) live, and is variant-aware **for the look half only**: a variant block redefines a type's style-and-props wholesale (the theme-values rule, not per-prop merge), while **`render=` is variant-invariant** — legal only in the default block's entry; a variant entry carrying `render=` is a load-time error with a move-it fix-it. Backends are policy, variants are looks: a light/dark switch must never change what a node IS.
- **No wiring in prototypes.** `on:` keys (and the `signals` flag) are refused in a theme entry with a routing error (``wiring belongs to instances — a theme entry has no name to wire``). A theme-level default handler on every instance of a type would be spooky action; the entry carries looks and policy, never behavior.
- **Precedence: nearest scope wins.** An instance line — in a view or inside a widget body, the same thing — beats the theme entry; two tiers, no third. A theme default style merges base-most — one seat before explicit styles in the shipped merge order (theme style → applied styles → node props → `when` overrides). Instance props override the entry's props individually — with one atomicity exception, next.
- **`render=native` is the spelled opt-out, and it discards the theme entry whole.** An instance opting out of the backend sheds the entry **entirely** — style included: the entry is one prototype, taken whole or not at all (its `for=` default style cannot apply to a native instance, and no per-piece opt-out spelling exists). The instance whose code-behind casts `as HSlider` opts out *visibly, at the instance where the cast lives* — and may pin `render=native` **before** any theme entry exists (legal, silent; the defensive pin is how cast sweeps happen ahead of the flip).
- **The D83 tension, stated rather than hidden:** a theme-level `render=gogo` changes the class of every unopted instance of that type — en masse, with no diff at each instance. That is deliberately close to the rejected automatic backend selection (§12), and different in the ways that matter: it is **one greppable policy line an author wrote**, not a compiler inference; the opt-out is spelled at the instance; and adopting it is documented as a migration decision, not a style tweak. Teams with `as HSlider` casts sweep them (or opt those instances out) as part of adopting the entry.
- **A `for=` default style implies the backend**: a theme entry pairing a `for=X` style without `render=gogo` would D15 at every instance — so the error fires **at the entry**, once, with the fix-it adding `render=gogo` to the entry (D15's theme-entry shape).
- Ships with the Slider round (§5.7 "Sequencing").

---

## 6. Grammar summary (delta only)

```
styleDecl      := "style" Name [ "for" "=" controlRef "," ] props* block?
                  // "for" contextual keyword, style headers only; must be first prop if present
controlRef     := TYPE-IDENT                                // the type name as spelled in trees:
                                                            // for=Slider, for=Knob

slotPath       := "slot" "." slotName "." propPath        // inside for= styles and on
                                                          // contract-bearing instance lines (§3.3)
whenClause     := "when" stateName assignments             // stateName checked per §2.2/§2.3

slotFill       := "slot" slotName block                    // in tree position, inside a control/widget's braces
slotDecl       := "slot" [slotName] props* [ coupling ] block?
                  // in widget bodies (no coupling) and control bodies (full form);
                  // bare "slot" = default slot (existing)
coupling       := "automatic" "=" autoKind byClause [ inClause ]   // D134: fully spelled
autoKind       := "length" | "position"
                | ( "arc" | "angle" ) "(" "from" "=" NUMBER "," "sweep" "=" NUMBER ")"
                | "presence"                                // driver is the byClause state
byClause       := "by" ( "$" paramName | stateName )        // $param: fraction kinds;
                                                            // state: presence
inClause       := "in" "$" paramName ".." "$" paramName     // fraction kinds only; v1:
                                                            // exactly $min..$max

controlDecl    := "control" CapitalizedName headerParams block
                  // headerParams: "$param=default" declarations, the widget-line grammar
                  // (bare k=v bakes a prop onto the root, .Style applies to the root);
                  // block contains: scriptLine (required), stateBuiltinLine*, stateLine*,
                  // signalLine*, tree — plus scoped anim/style/transition
                  // directives alongside the tree (§5.2, §2.4)
scriptLine     := "script" StringLiteral
stateLine      := "state" name ("," name)*
stateBuiltin   := "state_builtin" builtinStateName ("," builtinStateName)*
signalLine     := "signal" name ("," name)*
builtinStateName := "hover" | "focus" | "pressed" | "disabled" | "checked"

liveValue      := "live" "(" "$" IDENT ")"
                  // the closed call-form set gains one member; legal only as a
                  // prop value in a control's default tree; the $ident must be
                  // a contract prop (D17)

themeTypeDefault := TYPE-IDENT { "." STYLE-IDENT | property }
                  // inside theme blocks (§5.8): an instance-line prototype —
                  // no name, no children; variants override the LOOK half
                  // wholesale (render= is variant-invariant, default block only)
```

Contextual keywords added: `for` (style headers), `slot` (path-root in `for=` styles and instance lines; already a tree keyword), `script` / `state` / `state_builtin` / `signal` / `automatic` (control blocks — `state` is un-retired in this position only, §5.3; `signal` declares names, distinct from the shipped `signals` flag that marks a widget's surface node), `live` (call form, control default trees only; `bind` is a redirect hint). All checked at their proper file location/context per the existing contextual-keyword machinery; none are globally reserved.

---

## 7. Diagnostics catalog

All diagnostics ride the shipped error contract (gogo-spec.md "Error contract", D68): structured diagnostics with spans/notes/help, the GCC-shaped one-liner and rust-shaped block formats, the JSON channel (`GogoError.to_dict()` / `add_diagnostics_listener` — there is no CLI or `--format` flag), and machine-applicable fix-its. Vocabulary per §0. IDs are for this spec's cross-references; shipped codes are assigned per the existing G-banding (mostly G3xxx styles/when/transition, G5xxx widgets).

| ID | Trigger | Message shape | Fix-it |
|---|---|---|---|
| D1 | `slot.` path in a plain style | ``'slot.thumb' needs to know which control — add `for=Slider` `` (target inferred from application sites; several distinct targets → one fix-it per candidate) | insert `for=<target>` first |
| D2 (lint) | custom state in plain style's `when`, **and** every **clean** application site is one control type (erroring applications — D14, D15 — establish nothing; two-plus distinct clean types = deliberately generic, the lint stands down — §2.3) | ``'dragging' isn't checked here — add `for=Slider` to catch state typos as you type`` (target inferred from application sites) | insert `for=` |
| D3 | assignment to automatic property (styles, fills, inline `when`, anim tracks, arrangements — §4.5) | see §4.3 | delete line |
| D4 | `when` state not declared by the target of a `for=` style (unknown names **and** contract states the control didn't opt into; universal states lint D16 instead) | ``no state 'draging' on Slider — its states: hover, focus, disabled, dragging`` / ``Knob doesn't use 'pressed' — its states: hover, focus, disabled, dragging`` | rename; remove; for user controls, add `state_builtin <name>` to the control |
| D5 | unknown slot (path or fill) | ``no slot 'thmb' on Slider — its slots: track, progress, thumb, label`` | rename |
| D6 | — retired: slot closure was rejected (§12); every slot is fillable. ID not reused | — | — |
| D7 | `control` without `script` | ``a control's script is what turns input into its value — without one, declare a `widget` instead`` | two fix-its: scaffold `<name>.gd` with interface stubs, or convert to `widget` |
| D8 | `for=` style attempts child nodes / structure | points at declaring a `control` or `widget`; offers scaffold-from-declaration fix-it (materialize the control's default tree) | scaffold |
| D9 | `state <builtinName>` | ``'hover' is a built-in state — declare it with state_builtin`` | move to `state_builtin` |
| D10 | `for=` style applied to wrong control | ``style 'ThinSlider' is for Slider — applied here to Button`` | — |
| D11 | `for=` not first property | reorder note | reorder |
| D12 | — retired before shipping: uncomputable post-§4.5 (§4.4); ID not reused | — | — |
| D13 | lint: script reads pointer-over ad hoc | suggest `state_builtin hover` | insert |
| D14 | application site: applied control lacks a **contract** state used by a plain style (universal states never trigger it) | error: ``Button has no state 'dragging' — states here: hover, focus, disabled, pressed, checked`` (§2.3 — the shipped `pressed`-needs-a-button rule, generalized to contract data) | rename; remove the style from this control |
| D15 | contract feature on a **native** built-in instance (§5.7): a `for=X` style, a `slot` fill, an inline `slot.` prop, or a plain style using a contract state **only the composite raises**; also a theme entry pairing a `for=` style without `render=gogo` (fires once, at the entry — §5.8) | ``.ThinSlider restyles this slider's slots — Gogo has to draw it`` + note stating both consequences (looks the same until styled; `as HSlider` stops working) | state-only style: drop `for=` (offered first); insert `render=gogo` (confirm-level applicability, never `safe` — it changes the node's class) |
| D16 (lint) | `for=X` style responds to a universal state X's declaration doesn't list | advice: add `state_builtin <name>` to X so the contract (explain pane, completion) lists what the style responds to | insert `state_builtin` |
| D17 | live-read family (§5.2): bare `$param` read in a control's default tree; `live()` of a non-param; `live()` outside a control's default tree | ``$value is a contract prop — a pasted read would freeze at its initial value`` / ``only contract props are live — Knob's props: value, min, max, step`` / route back to control trees | wrap in `live($value)` (machine-applicable); rename; delete |
| D18 (lint) | behavior implements the keyboard hook in a control that doesn't declare `state_builtin focus` (§5.5 — keyboard reaches a control only through focus) | ``Knob handles keys but never takes focus — keyboard input can't reach it`` | insert `state_builtin focus` |

Diagnostic-wording rule (write into the diagnostics guidelines): every string drafted from the user's goal inward; mechanism vocabulary stays in this spec.

---

## 8. Tooling requirements

### 8.1 Editor completion (gogo_assist — there is no LSP; the editor panel's assist is the completion surface)

- Style header property position: offer `for=` with hover: *"Styling a specific control, like a slider or checkbox? Add `for=Slider` and you can restyle its parts of the control (`slot.track`, `slot.thumb`, …) and react to its states (`when dragging`). Bonus: typos in slot and state names get caught as you type instead of blowing up later."* (Trim to fit surface; the register is the requirement.)
- Inside a `for=X` style: completion is **closed and exhaustive** — exactly X's slots after `slot.`, exactly X's styleable properties per slot (automatic-owned properties shown struck-through/annotated "automatic — the control drives this", not hidden), and after `when`: the universal states plus X's declared contract states (§2.3).
- Tree position inside a control/widget's braces: `slot ` completion lists that control's slots with their defaults summarized.
- Inside a `control` block, statement position: offer `script`, `state`, `state_builtin`, `signal`, `slot` — prefix-matching means typing `state` surfaces `state_builtin` adjacently (a deliberate discovery channel). `state_builtin ` completion is the closed built-in set with one-liners.
- Inside a control's default tree, `$` completion lists theme values as-is and contract props annotated "live — wraps in `live()`", inserting the wrapped form (`live($value)`); the explain pane's props section shows what binds them.
- Hover on a built-in state name anywhere: *"hover — built-in state: Gogo keeps it accurate (touch, capture, focus order); your script can read it, styles react with `when`."*

### 8.2 Explain (the editor panel's explain pane)

Rendered in the shipped explain surface (the editor panel's explain pane — there is no `gogo` CLI) from declaration data — identical pipeline for built-ins and user controls. Format:

```
slots
  track     restyle: height, background, radius…  ·  fill: tick marks, labels
  progress  restyle: background, radius… · length is automatic  ·  fill: yes (clipped)
  thumb     restyle: size, background… · position is automatic  ·  fill: yes (default: round handle)

states
  when hover      built-in
  when dragging   finger's down and moving it
props / signals / script / styles-in-project sections follow
```

One row per slot, both verbs on the row. No prose docs are written for contract inventory — the declaration is the single source; every surface (explain pane, completion, diagnostics, scaffolds) renders the same data.

### 8.3 Scaffolding fix-its

D7's "scaffold script" generates the `.gd` with interface stubs. D8's "scaffold from declaration" copies the control's default tree into the user's file, correct for the version being built against (the anti-"copy 200 lines from stale docs" mechanism).

D8's insertion rides the shipped edits channel as-is (a span replacement with `len=0` is an insert). D7 does not: the shipped channel is span edits in *existing* files, so this spec extends it with a **create-file edit kind** — new file path + full contents, offered under the same applicability rules, refused (not silently overwriting) if the path already exists. Scaffolds emit `live()` for any param-displaying content they generate.

### 8.4 Debug overlay

Per-widget toggleable overlay showing live contract values during interaction: `fraction: 0.63 · dragging · hover`. This is the only surface where fraction is visible (§4.1), hence a headline feature, not an afterthought. Exposed like the shipped D103 input overlay: a runtime call plus an editor-preview toolbar toggle.

---

## 9. Explicitly out of scope (v1) — design before/with v2

Listed so implementers don't accidentally half-build them:

1. **Collections** — the known missing mechanism. Repeating items with per-item states/slots (Tabs, Choice, Select, carousel, lists): needs `slot item repeated`-shaped contract declarations, per-item states (`selected`), and collection-aware automatic kinds (indicator interpolating between item bounds). Design this **before freezing the `control` grammar for v2**; it may pressure slot syntax. Note: `selected` as a built-in state likely belongs to this design (exclusivity semantics live here); `checked` already ships as a built-in state (§5.3).
2. **Popup/layering primitive** — z-order, screen positioning, edge flipping, outside-dismiss. Prerequisite for user-built Select/tooltip/dropdown. Runtime work, not language.
3. **Input arbitration spec** — public routing/claiming rules for nested behaviors (Knob inside a scroll). Proposed default to evaluate: innermost behavior claims on movement threshold; ancestors may reclaim by axis. Must be boring and explicit, not emergent.
4. **Behavior reuse** — `script builtin.slider`-shaped reference ("my structure, stock slider's input logic"). High-leverage; would let radial knobs skip GDScript. It is reuse of a script, not absence — the `script` line stays mandatory and later accepts a second value kind.
5. **Continuous state drivers** — style properties as functions of fraction. When designed, fraction graduates from internal to exposed; the relief valve for everything the closed automatic set can't express.
6. **`shape=` axes on built-ins** (e.g. `Slider shape=radial`) — permitted as per-control convenience, orthogonal to this spec; radial changes keyboard mapping, so it is a behavior prop, and linear-orientation vs. radial start-angle/sweep both live under it.
7. **Contract relationships** — the built-in pairs (slider↔progress, slider↔scroll bars) *and* user declarations ("Knob accepts slider styles"): deferred until their members exist (§5.7's v1 set has no Progress); the user-facing declaration syntax waits with them.
8. **App-wide restyling** — graduated to **§5.8** (2026-07-14): theme-level type defaults (default style + `render=`) are in v1 scope, shipping with the Slider round. Entry kept so the number stays stable.
9. **C# behaviors** — `script "….cs"` implementing the same behavior interface; v1 is `.gd`-only (§5.5).
10. **`hit=` slot prop** — declarative hit shaping for non-rectangular slots (parked 2026-07-14: no v1 consumer; the first customer is the CustomWheel port round). Candidate spelling, recorded so it isn't reinvented: `hit=rect` (default, never written) | `hit=alpha` (texture-alpha; legal only when the slot content is an Image) | `hit=[x,y, x,y, …]` (polygon, epx, closing edge implied — would join Grid's `cols=`/`areas=` and Gradient's `stops=` as the third list consumer, an explicit widening of the list fence). A slot prop, never script logic (§5.5).

---

## 10. Acceptance examples

These must compile/behave exactly as annotated; they double as integration tests. Each lettered block is its **own document** — F's theme entry must not leak into B/C/D (it would un-error their D15 lines).

```gogo
// A. Existing code unchanged
style AppBarButtonLook background=#00000000 {
    when pressed flash.visible=true          // built-in state in plain style: OK
    when disabled opacity=0.3
}
widget Card { Panel padding=12 { slot } }
Card { Label text="hi", font-size=14 }       // bare children → default slot: OK

// B. Tier 1
style ThinSlider for=Slider, slot.track.height=2, slot.thumb.size=12 {
    transition duration=0.15, easing=cubic_out   // times this style's when flips —
    when dragging slot.thumb.scale=1.2           // top-level transitions time ARRANGEMENTS
}
Slider volume .ThinSlider render=gogo, value=$vol, min=0, max=100, step=5   // OK (§5.7)
Slider plain .ThinSlider value=$vol                             // D15 — native instance; fix-it: render=gogo
Button save .ThinSlider                                         // D10

// C. Errors and lints
style Bad1 slot.track.height=2 { }                     // D1 (+fix-it: for=)
style Bad2 for=Slider, slot.thumb.x=10 { }             // D3 (automatic)
style Bad3 for=Slider { when draging slot.thumb.scale=1.2 }   // D4
style Grabby { when dragging opacity=0.8 }             // legal; lint D2 fires (clean applications: Slider only —
                                                       //   s3/b2 error, so they establish no genericity)
Slider s2 .Grabby render=gogo, value=$vol              // OK — Slider declares dragging; the composite raises it (§5.7)
Slider s3 .Grabby value=$vol                           // D15 — native Slider runs no behavior to raise 'dragging'
Button b2 .Grabby                                      // D14 error: Button has no state 'dragging'
style Bad4 for=Knob { when pressed slot.thumb.scale=1.1 }     // D4 — Knob doesn't declare state_builtin pressed
                                                              //      (fix-its: add it to Knob, or remove clause)

// D. Tier 2
Slider v render=gogo, value=$vol {
    slot thumb { Image texture="res://grip.png", 12x12 }       // OK
    slot thmb  { Image texture="res://grip.png", 12x12 }       // D5
}

// E. Tier 3
control Knob $value=0, $min=0, $max=1, $step=0 {
    script "res://controls/knob.gd"
    state_builtin hover, focus, disabled
    state dragging
    signal drag_started, drag_ended
    transition duration=0.15, easing=cubic_out          // default feel (§2.4)
    Circle color=$surface2 {
        slot track 48x48 {
            slot progress automatic=arc(from=-225, sweep=270), by $value in $min..$max { Arc thickness=4, color=$accent }
            slot thumb automatic=angle(from=-225, sweep=270), by $value in $min..$max { Circle 8x8, color=$text0 }
        }
        slot label { Label text=live($value) }                  // live read of a contract prop (§5.2)
    }
}
style BigKnob for=Knob, slot.track.size=64 { when dragging slot.thumb.scale=1.2 }  // OK, write-time checked
Knob gain .BigKnob value=$gain, min=-60, max=6 {
    slot thumb { Image texture="res://grip.png", 8x8 }          // OK
}

control Broken $value=0 { Panel { } }                           // D7
control AlsoBroken $value=0 {
    script "res://x.gd"
    state hover                                                 // D9
}
control Frozen $value=0 {
    script "res://x.gd"
    Panel { Label text=$value }                                 // D17 — pasted read would freeze; fix-it: live($value)
}

// F. Theme-level type defaults (§5.8)
theme {
    Slider .ThinSlider render=gogo               // every Slider: ThinSlider look, Gogo backend
}
Slider s4 value=$vol                             // OK — composite via the theme entry, wears ThinSlider
Slider s5 render=native, value=$vol              // OK — opt-out discards the entry WHOLE (style included);
                                                 //      stays HSlider (`as HSlider` code lives here)
Button b3 render=native                          // OK, silent — a defensive pin (protects a cast
                                                 //      against a future theme entry)
```

## 11. Design-doctrine notes for future extensions (informative)

Rules derived during design; apply to future keyword/syntax decisions in this area:
- One canonical spelling per concept; when two spellings appear, one is a bug.
- Author-owned is the unmarked case; marks point at language-owned machinery (`state_builtin`, `slot.`, `automatic=`, lowercase constructs).
- Spelling weight tracks usage frequency, not concept importance: hot-path syntax earns terseness; cold-path (once-per-control) syntax spends characters on self-evidence.
- Compound keywords must carry a permanent fact about the thing (not the moment of writing).
- Keywords should be words users already say ("slot", "script", "built-in").
- Every boundary between tiers is a diagnostic with a fix-it; the tiers teach themselves.

---

## 12. Rejected (recorded so they aren't re-derived)

- **Slot closure / `fillable=false`** (rejected 2026-07-14). Earlier drafts let a contract close specific slots to fills, with a mandatory reason string surfaced in a D6 diagnostic. Removed: no case survives the rest of the spec — automatic ownership (§4.3/§4.5) protects driven properties no matter what a fill contains; nested slots are contract structure that survives fills and draws above them (§3.1), so value display can't be buried; behaviors never touch nodes (§5.5), so no function depends on a slot's content; and anything genuinely load-bearing (an Input's caret machinery) is simply never declared as a slot in the first place. **Every slot is fillable, always.** Revisit only with a concrete restylable-but-must-not-fill case, through §9. D6 stays retired; the ID is not reused.
- **Automatic backend selection** for built-ins — recorded inline in §5.7 ("Rationale"), listed here for findability: a style application silently changing an instance's node class is spooky action compensated by tooling; `render=gogo` is the explicit spelling.