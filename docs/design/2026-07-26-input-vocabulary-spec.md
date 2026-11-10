# Gogo Input Vocabulary — Mini-Spec (v2)

Status: adopted (D181) · built 2026-07-26 · Scope: interaction states and events for all controls,
value states and events for value-bearing controls.

v1 was drafted blind to gogo and is superseded. v2 folds in the review
against the shipped language (gogo-spec.md, D83/D118/D121/D122/D129/D133)
and four user rulings (2026-07-26):

1. **Full rename round** — states and events both respell in one
   D83-style migration (retire-with-redirect, all app files same round,
   zero-visual-change bar). No mixed morphology left behind.
2. **Boundary events deferred** — `down`/`up`/`enter`/`exit`/`focus`/
   `blur` do not ship; no consumer exists. The raw register carries the
   rare need meanwhile.
3. **Auto-wiring is mechanical** — `On<Name>Change`, `On<Name>Activate`;
   no hidden tense-shift.
4. **`focus_within` keeps its spelling** — the one documented
   non-participle (it names a relation, not a condition of the node).

## 1. Design principles

1. **The slot carries the state/event axis.** `when <state>` declares a
   continuous condition; `on:<event>` binds a discrete moment. The
   grammar disambiguates — `when activate` and `on:held=` are errors.
2. **Tense carries the register.** Inside `on:`, bare stems are gogo
   vocabulary (`change`, `edit`, `activate`, `show`, `hide`); snake_case
   past-tense names are raw Godot signals (`on:drag_ended`). The register
   a line speaks is visible at a glance.
3. **States are past participles** — `hovered`, `held`, `focused`,
   `selected`, `disabled` — so every `when` line reads as an English
   clause ("when held, scale 0.97"). One exception, documented:
   `focus_within` names a relation ("focus is within me"), and a
   participle would claim the container itself is focused.
4. **New meaning, new stem — never a new affix.** Semantic distinctions
   are carried by different words (`edit` vs `change`), never by tense
   or suffix on one word (`change`/`changed`/`changing` are one concept,
   one spelling: `change`).
5. **Name the effect, not the cause or the device.** `activate`, not
   `click`/`tap`/`press`; `edit`, not `input`. Vocabulary is
   input-agnostic (pointer, keyboard, gamepad).

## 2. States (`when`)

| state | meaning | provided by |
|---|---|---|
| `hovered` | pointer is over the control; a press CAPTURES it (slip-off keeps the look until release) | universal, dormant |
| `held` | input is down on this control; IS the drag state on Slider and Toggle — no separate `dragging` word | press surfaces only |
| `focused` | VISIBLE focus — D129 semantics verbatim (key/gamepad-warranted indicator; caret exception on Input; buttons keep the injected default ring response) | universal, dormant |
| `focus_within` | focus is on the node or a descendant (D129) | universal, dormant |
| `selected` | "this one is chosen" — was `checked`; persists until the next flip | toggle-mode Button, Radio, Check, Toggle, List row root |
| `disabled` | unavailable; CASCADES (D133) unchanged; no event pair — states do not owe one | `disabled=` |

Priority when several hold (respelled only, order unchanged):
arrangements < `focus_within` < `focused` < `hovered` < `selected` <
`held` < `disabled`.

Rules carried over unchanged:

- **`held` stays gated** (the shipped `pressed` law): legal only where
  press tracking exists — the Button family (incl. templated Radio/Check
  and toggle mode), `ListItem`, and now `Slider` and `Toggle` (their drag
  IS a press). Elsewhere it is a load-time error listing what is
  available — a `when` that can never fire is a silent bug. The other
  states stay universal-dormant because providers can flip them on any
  node.
- **`focused` is visible-focus, one spelling, one meaning.** There is no
  raw-focus state or event in gogo; code-behind reads `has_focus()`, and
  raw focus edges live in the raw register (`on:focus_entered`) with
  honestly raw semantics.
- **`selected` flips live on Toggle** as the thumb crosses the middle —
  state is live presentation; `change` (below) is commitment. The two
  disagreeing mid-drag is the layer separation working, not a bug.
- The shipped `dragging` state — it existed on BOTH the composite Slider
  and Toggle — retires into `held` on both (one concept, one word).

## 3. Events (`on:`)

| event | fires | payload |
|---|---|---|
| `activate` | the control's OWN trigger gesture completed | — (List: the row key) |
| `change` | the committed value is now different — once per commit | the new value |
| `edit` | a live, uncommitted value moved | the live value |
| `show` / `hide` | visibility choreography (existing pair, unchanged — already bare stems, already effect-named) | — |

Semantics:

- **`activate`** — "the do-the-thing moment," and the control owns the
  gesture: Button — a completed tap (press + release on the control;
  slide-off clears, slide-back re-arms, D122) or key/gamepad accept;
  List — EVERY tap or Enter/Space on a row (the shipped 2026-07-15 user
  call: activation always fires, selection is the mode's side effect —
  WP list semantics, not desktop double-click). Never "fires on release"
  as a universal law — that was v1's device-y slip against principle 5,
  and it broke the List case. Never fires disabled.
- **`change`** — inherits the shipped choice-group law verbatim:
  **changed means CHANGED** — one report per genuine transition. Re-pick
  of the current value repairs, never re-reports; programmatic /
  binding-driven writes report like interaction (a no-signal seat stays
  invisible); a same-value write is no event, which naturally breaks
  handler→write→event loops.
- **`edit`** — exists only on continuous/free-form controls (Slider,
  Input). Discrete controls have no uncommitted value to stream.

### The register law

`on:` stays **open**. Raw Godot signals remain legal for anything gogo
has no word for (`on:drag_ended`, `on:resized`, `on:mouse_entered`).
Raw spellings that duplicate a gogo concept are SHADOWED with a
one-keystroke redirect — the D83 duplicate-raw law applied to signals:

| raw spelling | redirects to |
|---|---|
| `on:pressed` | `on:activate` |
| `on:toggled` | `on:change` |
| `on:value_changed` | `on:edit` (live) / `on:change` (committed) |
| `on:changed` | `on:change` (shadows `Range.changed` — one letter must not mean two things) |
| `on:activated` | `on:activate` (retired gogo spelling) |
| `on:text_changed` | `on:edit` (duplicates Input's live stream) |
| `on:text_submitted` | `on:change` (duplicates Input's commit) |

### Auto-wiring

Mechanical, no irregularity:
`name + event → "On" + PascalCase(name) + PascalCase(event)` —
`saveButton` + `activate` → `OnSaveButtonActivate()`, `brushSize` +
`change` → `OnBrushSizeChange(value)`. Raw signals keep the same
mechanical rule (`OnFooDragEnded`). One grep finds both sides of a wire.

**Stale-handler lint (required, ships before the app migrates):**
auto-wiring matches by convention, so after the rename a leftover
`OnSaveButtonPressed()` would silently unwire — the exact failure the
explicit-wiring load check exists to prevent. A host method matching
`On<KnownNamedNode><RetiredEvent>` (`Pressed`, `Toggled`,
`ValueChanged`, `Changed`, `Activated`) is a load-time error naming the
new spelling.

## 4. Layer 2 — value-bearing controls

| control | value state (`when`) | live (`on:`) | commit (`on:`) | commit payload / notes |
|---|---|---|---|---|
| `Check` | `selected` | — | `change` | `bool`; fires on the control |
| `Toggle` | `selected` (flips live mid-drag) | — | `change` | `bool`; on settle (tap-glide or drag release), only if it differs from the pre-gesture value — wiggling across the middle is zero changes |
| `choice` (Radios) | `selected` per Radio | — | `change` on the GROUP | the picked Radio's name; items refuse `on:change` (double-registration unrepresentable); the group is erased structure — no node exists to carry a `when` at all |
| `Slider` | — (`held` is the drag) | `edit` per tick | `change` on release | `float`; keyboard/gamepad: each step commits directly (`change` only — a step and a programmatic write are indistinguishable at the engine seam, and an `edit` landing in the same instant as its commit adds nothing); undo coalescing is the app's job |
| `Input` | — | `edit` per keystroke | `change` | `String`; on Enter or focus loss; programmatic `text=` is synthesized (Godot emits nothing on set) and reports per the changed-means-changed law |
| `List` | `selected` on the row's `ListItem` root | — | `change` on the List | the key (`select=one`), Array of keys (`many`), `null`/`[]` on clear; `select=off` refuses `change` (G9104 behavior, respelled) but `activate` still fires with the row key |

## 5. Composition

`activate` is interaction; `change` is consequence.

- **Button** — `on:activate` → handler. No value layer.
- **Check / Toggle** — `on:activate` → runtime flips `selected` →
  `on:change` fires.
- **Radio** — `on:activate` on the item → group moves `selected` →
  `on:change` fires on the group.
- **Slider** — `held` while dragging; each tick fires `on:edit`; release
  commits and fires `on:change`.
- **Input** — `focused` while editing; each keystroke fires `on:edit`;
  Enter or focus loss commits and fires `on:change`.
- **List** — tap selects (`selected` moves, `change` commits);
  every row tap (and Enter/Space) fires `on:activate` with the key —
  the shipped single-tap law; selection is the mode's side effect.

Rules of thumb:

- Listen to `activate` on buttons, `change` on everything else.
- Reading a value from an `activate` handler on a value control is a
  smell — use `change`, which also catches programmatic updates.
- Reach for `edit` only when you genuinely need the live stream.

## 6. Rejected words (and why)

| word | rejected as | reason |
|---|---|---|
| `pressed` | state and event | One token, two timings in the shipped language: `when pressed` held while down, `on:pressed` fired on release (Godot's signal). Retired entirely. |
| `click` / `tap` / `press` | event | Device-specific; lies about keyboard/gamepad activation. |
| `checked` | state | Same concept as `selected`; the rename unifies with List's `select=` and reads right on rows/tabs. (`Check` the control stays — control names name artifacts, states name conditions.) |
| `toggle` | event | Second spelling for "value changed"; use `change`. The word stays in its other registers: `toggle=` (button mode) and `toggle(pose)` (arrangement call). |
| `dragging` | state | Second word for `held` (was shipped on Toggle; folds in). |
| `on:changed` / `on:changing` | event | Affix magic on `change` (principle 4); `changed` is also a real `Range` signal — shadowed, redirected. |
| `on:input` | event | Collides with the `Input` control; names the cause, not the effect. |
| `on:hold` | event | `hold` root is reserved for the `held` state. |
| `enter` / `exit` | event | RETIRED gogo words (old state-block spellings — reusing them poisons the redirect machinery and every old doc), and `enter` collides with the Enter key in the same vocabulary (`Input` commits on Enter). Off the table permanently. |
| `blur` | event | Web-only jargon; in a paint app `blur` means the image effect. If raw focus edges are ever needed: `on:focus_entered`/`on:focus_exited`, visibly raw. |
| `down` / `up` | event | Not verbs, no consumer; deferred. Raw `on:button_down`/`on:button_up` are legal meanwhile. |

## 7. Lowering to Godot (informative)

| Gogo | Godot |
|---|---|
| `on:activate` | `pressed` signal (Button family); List gesture synthesis |
| `held` | `button_down`/`button_up` tracking; Slider grabber, Toggle drag |
| `hovered` | `mouse_entered` / `mouse_exited` + press capture |
| `focused`, `focus_within` | D129 visible-focus machinery — NOT raw `focus_entered` |
| `selected` | `button_pressed` (toggle mode); List row state |
| `on:change` (Check) | `toggled(bool)` |
| `on:change` (Toggle) | gogo-drawn settle logic (crossing-count-insensitive) |
| `on:change` (choice) | synthesized from members' `toggled`, emitted once with the name |
| `on:edit` / `on:change` (Slider) | `value_changed` split by `drag_ended`; per-step for keys |
| `on:edit` / `on:change` (Input) | `text_changed` / `text_submitted` + focus-out commit; programmatic writes synthesized |

## 8. Diagnostics and the migration round

Familiar-but-wrong words get targeted errors (D84 voice, one-keystroke
fixes where mechanical), riding the existing RETIRED machinery:

- `when pressed` → "use `when held`."
- `when hover` / `when focus` / `when checked` → "use `when hovered`" /
  "`when focused`" / "`when selected`."
- `when dragging` → "use `when held`."
- `on:pressed` → "`pressed` isn't a gogo event; use `on:activate`
  (the control's trigger gesture) or the raw
  `on:button_down`/`on:button_up`."
- `on:toggled` / `on:changed` / `on:activated` / `on:value_changed` →
  redirects per the shadow table (§3).
- `on:changing` → "use `on:change` (committed) or `on:edit` (live)."
- `on:input` → "use `on:edit` (this is DOM's `input`)."
- `on:change` on a `choice` Radio → "`change` lives on the group; items
  expose `when selected`."
- `on:enter` / `on:exit` / `on:blur` → "gogo has no boundary events; use
  `when hovered`/`when focused` for styling, or the raw
  `on:mouse_entered`/`on:mouse_exited`/`on:focus_exited`."
- Stale auto-wire handler (`On<Node>Pressed` et al.) → load-time error
  naming the new spelling (§3, Auto-wiring).

**The round itself** (D83 precedent): all app files, `controls/*.gogo`,
docs cards, and injected defaults (Button focus ring, ListItem D122
responses) migrate in the same round; the acceptance bar is zero visual
change (parity states match baselines exactly). Reserved-words list
updates: add `held`, `hovered`, `focused`, `selected`; `pressed`,
`hover`, `focus`, `checked` join `state`/`enter`/`exit` in the retired
set. The stale-handler lint lands FIRST so it catches every code-behind
rename.

## 9. Deferred, deliberately

- Boundary events (`down`/`up`, hover edges, focus edges) — until a
  consumer appears; the raw register carries the need meanwhile.
  `enter`/`exit`/`blur` stay off the table permanently (§6).
- `long_press` (house snake_case) — press held past a duration
  threshold.
- `when dirty` — value edited but not committed (stem kept far from
  `change`).
- Vetoable pre-events — if ever added, an explicit mechanism, not a
  suffix convention.
