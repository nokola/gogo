# D129 — visible focus and `focus-within` (2026-07-17, built same day)

Grew out of the D128 round: dropping the focus slot from the pigment
takeover ("item 3") would have restored Godot's engine focus ring on
styled buttons — and Godot's click-focus (`FOCUS_ALL` grabs on click/tap)
would then park a visible ring on every tapped button, the exact
press-residue artifact D122 fought on rows. The root fact underneath every
symptom (D75 hover stranding, D122 `kb_only`, the sticky ring): gogo did
not know HOW the user was driving the UI.

## The decisions

**1. One input-modality truth.** `GogoStates._modality_key` — key-driven
(keyboard, gamepad button, a real stick push) vs pointer-driven (mouse
press, touch). Fed by ONE lazy `ModalityWatcher` node per tree (created by
the first focus-wired machine) plus list_root's existing `nav_keyboard` /
`nav_pointer` calls (rows see events before viewport navigation).
Modality changes on PRESSES only — pointer motion never flips it, so a
keyboard user brushing the mouse does not strip every ring on screen (the
flicker bug in the naive `last_input != mouse` formula). D122's
`_cursor_by_key` static merged into this; the D75 hover mouse-evidence
sampler stays separate (it answers "is there a real mouse", a different
question) but lives in the same file as the second facet of one service.

**2. `focus` = visible-focus. One spelling, the correct semantics.**
The state holds while the control is focused AND an indicator is
warranted: latch reads modality at focus entry; a key press while focused
upgrades it on; a pointer press anywhere turns it off; a focused
`Input`/`TextEdit` always indicates (the caret exception — CSS's UA
heuristic for fields). This is CSS `:focus-visible` given the SHORT name:
CSS carries two states only because it could not break the old one — a
fresh language skips the legacy arc. Precedent is gogo's own `hover`
(D75): the raw engine signal was wrong for styling, the short name kept
the corrected meaning. Raw focus is data, not a style hook — code-behind
reads `has_focus()`; no `focus(any)` spelling until a real consumer
appears. Grep at decision time: ZERO shipped `when focus` responses in
Fantasia — the re-semanticization was free, and per doctrine it ships
loud (spec/REFERENCE/cards all updated), never silent.

**3. Buttons: the engine ring retires; the theme ring becomes the
injected DEFAULT `when focus` response.** Every gogo BaseButton gets
`@focus_ring=false` at build (a shared StyleBoxEmpty parked in the focus
slot) and, when no `when focus` is authored, a default response that
flips `@focus_ring` — so Tab/gamepad focus shows the THEME's own ring
(app themes and the D128 shape copies keep working: the gate reveals the
reshaped copy, a rounded button gets a rounded ring) and a tap parks
focus with no residue. Authoring `when focus` replaces the default — the
D122 ListItem doctrine, verbatim. `Input` is untouched (caret exception:
its native ring answers raw focus by design). The pigment takeover
(D128) now fills only the five PLATE slots — focus is an overlay, owned
by the gate.

**4. `focus-within` (universal, built now — gogo will be used for
games).** Holds while the focus is on the node or any descendant;
visible-focus flavored. The controller-menu staple: focus sits on the
leaf (slider), the highlight belongs to the container (settings row,
active pane, shop card) — the case the ListItem model (rows ARE the focus
unit) cannot cover. Wired to the viewport's `gui_focus_changed`;
ancestry via `is_ancestor_of`; modality edges re-evaluate against the
live focus owner (also self-heals a silent `release_focus()`). Ranks
below every other state — ambience under input feedback.

## Naming (the full loop, kept for the six-month read)

within → contains → is-in-descendant → **within**. `focus-within` was
challenged as directionally ambiguous ("focus is within me" vs "I am
within the focus"); `contains-focus` fixes the direction but hides from
prefix discovery (types `focus`, sees the family — "easy to find" is a
stated goal); `focus-is-in-descendant` is maximally explicit but WRONG
for the self case (CSS/WPF both match when the element itself holds
focus) and fails the room-friend vocabulary bar ("descendant"). The
standards name won on verified grounds: prefix clustering next to
`focus` in the assist, the only name both parent traditions agree on
(CSS `:focus-within`, WPF `IsKeyboardFocusWithin`), and the assist
one-liner teaches the direction at first contact — the ambiguity is a
first-read problem and the first read never happens naked in gogo. The
challenger's definition sentence became the card's first line.

## Accessibility posture

Screen readers are untouched at every layer (AccessKit reads the a11y
tree; TalkBack draws its own focus rect). The visual indicator for
sighted key/gamepad users is now gogo's responsibility — key-gated, like
every modern platform. §5.7 behavior parity gained "visible-focus
indicator" and "screen-reader exposure" — and the `accessibility_role()`
hook is CONSUMED (same-day follow-up): gogo_control_root reports role +
value/checked/disabled + reader actions in its
NOTIFICATION_ACCESSIBILITY_UPDATE pass, API probed against the real 4.7
binary (probes/probe_accesskit*.gd). Composite INTERNALS hide: the root
marks every body node (`gogo_a11y_internal` meta) and stamps exposed
elements FLAG_HIDDEN; gogo-scripted internals (GogoSlot, GogoLayers,
Circle/Arc/Gradient) re-stamp in their OWN update pass — element data
rebuilds wholesale per pass, and a driven thumb re-updates every move, so
self-stamping is the only stamp that sticks. The reader sees ONE element
per composite, the native-oracle shape; the label slot's text arrives via
the labeled_by relation. (The once-noted residual — a script-less leaf
re-exposing between root passes — was verified UNOBSERVABLE live by the
user, 2026-07-17: an item-by-item reader walk after a sustained drag shows
no leaked internals. Ancestor-hidden subtree semantics plus the root sweep
riding every value write contain it; closed.) Screen-reader announcements
verified live by the user (2026-07-17). Lists joined same-round: `List`
reports ROLE_LIST_BOX (plain LIST under select=off) with live count,
orientation and the many-mode multiselect flag — its `_accessibility_update`
OVERRIDES the composite base's, because rows are CONTENT: none of the
internals marking/hiding may run there. Every `ListItem` reports
index/selection (re-wired per roster commit, so reorders track) and a
reader CLICK that calls the same `_tap` a finger does — activation signal
and selection side effect identical by construction. Deliberately NOT done: Fantasia app-wide WP
plate-shade via a §5.8 theme default — on touch the indicator never
shows anyway (no keyboard), and shipped-app fidelity is king; add it
later as one theme entry if desired.

## Fences

- Test pins: `fvis.*` (latch/upgrade/downgrade/caret), `fring.*` (gate,
  theme ring, D128 shape compose, tap-no-residue, authored-replaces),
  `fwithin.*` (descendant lights, outside clears, tap-quiet, key-upgrade,
  did-you-mean).
- The old `kb_only` def tag is deleted; the ListItem cursor echo rides
  plain `when focus` semantics.
