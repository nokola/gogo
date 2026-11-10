# Animation sequencing — connectives proposal (2026-07-18, DRAFT)

Status: **decided (2026-07-18 review) — ready for an implementation
round.** The shape (connectives `and`/`then`, bare lines as steps,
`group` for property distribution and naming, `play` for composition,
single-name `after=` as the graph escape hatch) comes from an external
design discussion; this file grounds it against shipped gogo. Grounding
fixes are listed at the bottom; the five review calls are recorded in
"Decisions" near the end.

## Summary

An `anim` is a list of **steps**. A step is a bare property line, a
`group { }`, a `play`, or a `delay=`. Every step after the first declares
its relationship to the timeline with a **connective**:

- `and` — joins the previous step; they run together
- `then` — starts after the previous step (and everything `and`-joined
  to it) completes

A second step with no connective is a compile error (exception: a
referenced step — see `after=`). Sequencing is always written, never
defaulted.

```
anim appear {
    card.translate.y from=40, to=0, duration=0.2
    and  card.opacity from=0, to=1, duration=0.3
    then card.scale.x to=1.05, duration=0.08, autoreverse=true
}
```

Reads aloud: slide and fade, then pop.

## Steps

### Bare property lines

The lightest step: one property, animated in place. Targets are dotted
(`node.prop`, `self.prop`) exactly as today; inside an `each`, paths are
bare (the wearer is the target) — unchanged.

```
card.opacity from=0, to=1, duration=0.2
```

- `from=` is optional (current value used); `to=` is required. There is
  no bare positional value — `card.opacity 1` is an error ("write
  `to=1`").
- Step-level words on the line: `duration`, `easing`, `offset`,
  `autoreverse`. This is a closed list; unknown words get a did-you-mean
  against both step words and property targets. (`autoreverse=` on a
  property LINE is new — today it rides track lines only; under this
  grammar every step is its own envelope.)
- `key` blocks are unchanged: a keyframed line is still one step, its
  last key is its duration (so the line still refuses `duration=`), and
  `autoreverse=true` still mirrors the keys.
- Bare lines are anonymous. A step that needs to be referenced (see
  `after=`) must be a named `group`. For documentation, use a comment.

### `group <name> <props> { }`

A group does two jobs, neither of which is sequencing:

1. **Distributes step-level properties** (`duration-each`, `easing`,
   `autoreverse`) to its children. A line resolves each step word from
   the NEAREST enclosing scope that sets it — plain lexical scope, any
   depth, anim header included, a child's own setting always winning.
   (Decided at review; the draft's "two levels, no deeper" broke its own
   nested-wrapper joins pattern. The distributing duration is named
   `duration-each=` — addendum below — because plain `duration=` on a
   container header reads as the TOTAL and a `then`-chain would make
   that reading false.)
2. **Carries the name** when a step must be referenced by `after=`. Name
   follows keyword, matching `anim`/`style`/`widget`. Names are opt-in —
   only referenced groups need one (today every `track` name is
   mandatory).

```
group duration=1 {
    dial.translate.y to=100
    then dial.rotate to=90
    then dial.scale  to=1.5
}
```

A group contains its own connective chain. There is exactly one
sequencing grammar — the connectives. `group` never implies order:
several properties running together is a chain of `and`s. `{ }` never
appears without a keyword — a bare braces step is an error ("write
`group { }`").

`anim` headers distribute too — but honestly named (addendum, decided
2026-07-18 after the first implementation pass): the review first
REMOVED the header envelope because `anim x duration=1` reads as "the
whole anim is 1 second" and three `then`-steps make that reading false.
The user then spotted that plain `duration=` on a GROUP header carries
the exact same lie one level down — so instead of banning the position,
gogo renames the WORD: the distributing default is **`duration-each=`**,
legal on `group` and anim headers alike, and it cannot be misread — the
word says per-step. `easing=` and `autoreverse=` distribute from both
headers under their plain names (neither has a total-vs-per-step
reading). Plain `duration=` on any header refuses, teaching
`duration-each=`; lines keep plain `duration=` (there it IS the line's
own length). The common anim keeps its light header form:
`anim appear duration-each=0.3, easing=cubic_out { two and-lines }`.

### `play <animName>`

Runs another anim as a single step. This is how a parallel arm gets
internal sequence without nesting blocks. Same word as the response form
`on:show=play(name)` — one meaning, two registers, and the register
decides the shape (decided at review): lines are keyword-then-name like
every other line form (`anim appear`, `group pop`, `uses "file"`),
values are calls (`on:pressed=play(x)`). A call-shaped LINE would be
the only one in the language, and step words compose awkwardly on it.
Durations are static, so a played anim inlines at bake time like
`after=` chains do today; the runtime player is untouched.

```
anim badgeIntro {
    badge.translate.y from=-10, to=0, duration=0.15
    then badge.scale to=1.2, duration=0.3, autoreverse=true
}

anim panelOpen {
    panel.translate.x from=-200, to=0, duration=0.3
    and  play badgeIntro
    then panel.scale to=1.0, duration=0.1
}
```

### `delay=t` — a pause

A standalone step: a timed pause in the chain.

```
then delay=3
```

A pause is always positive — `delay=-1` is an error ("a pause can't be
negative — to start a step early, use `offset=` on that step").

This retires the `delay=` ATTRIBUTE (anim header, track lines): a pause
in the chain is this step; a shift relative to placement is `offset=`.
One job per word.

### `offset=t` — a signed shift

A step property on a line or group: shifts the step's start relative to
wherever it was placed — by a connective or by `after=`. Positive is
later, negative is earlier. Negative offset creates overlap:
`then card.scale ..., offset=-0.05` overlaps the previous step's tail;
`after=b, offset=-0.05` overlaps b's. One sign convention, every
placement. Use negative offsets sparingly — they reintroduce
read-the-numbers timing.

`delay` and `offset` are deliberately two words: a pause is a step and
always positive (that's what the English word means); a shift is a
property and signed. One concept per spelling, in both directions.

## The graph escape hatch: `after=`

`and`/`then` (plus nesting via `play` and groups) express every
series-parallel graph. Dependencies that **cross** — a step waiting on
some but not all of the parallel work before it — use a reference:

```
anim dashboardIn {
    group a { tiles.opacity to=1, duration=0.6 }
    and group b { header.translate.y to=0, duration=0.2 }
    then chart.scale to=1, duration=0.2            // joins a and b structurally
    group d after=b { header.background to=amber, duration=0.3 }  // crossing edge
}
```

- `after=` takes **exactly one name**, nothing else: the step starts
  when that group's chain completes (autoreverse's way back included, as
  today). There is no compound form — `after=b-0.05` is an error
  (identifier-minus-number, the value-position hyphen ambiguity banned
  everywhere else). To overlap, shift the step with `offset=`:
  `after=b, offset=-0.05`.
- Only named groups can be referenced; referencing forces both the group
  and the name. This weight is deliberate — a crossing graph is the
  heaviest rung of the ladder and reads as such.
- A step is **positional or referenced, never both** (confirmed at
  review). Every step gets exactly ONE placement: a connective places
  it relative to the previous line, `after=` relative to a named step.
  A referenced step therefore carries NO connective — the reference IS
  its placement, and the missing connective is the visible signal that
  the step floats free of the top-to-bottom flow. `then group c
  after=b { }` is an error: it names two different start times, and
  picking a winner would be silent precedence. (The discussion draft's
  own example broke this rule; fixed here — see grounding fixes.)
- `then` following a referenced step means "after that step completes,"
  resolved in graph time.
- Cycles are a compile error (as today between tracks).
- Lint: `after=` targeting the immediately preceding step with no
  `offset=` — "this is a sequence — write `then`."

### Joins are structure, not references

Waiting on several things at once needs no special form — it is what
the grammar already does:

- `then` after an `and`-cluster waits for the whole cluster.
- A named wrapper group completes at its latest member, so
  `after=wrapper` joins everything inside it:

```
group ab {
    group a { tiles.opacity to=1, duration=0.6 }
    and group b { header.translate.y to=0, duration=0.2 }
}
group c after=ab { chart.scale to=1, duration=0.2 }
```

**Caution when restructuring:** a join waits for *everything* in its
cluster or wrapper. Moving a step into a chain or wrapper adds it to
every join over that container — timing may coincidentally match today's
durations and silently diverge when a duration is edited. If a step must
be *exempt* from a downstream join (c waits on a and b but not d), the
exempted tail cannot share the joined container; name the sub-step and
use `after=` — that is exactly the case the escape hatch exists for.

### Deferred: multi-name `after=(x, y)`

Rejected for now. Single-name `after=` plus wrapper groups expresses
every DAG whose join-sets nest. The class it cannot express:
**overlapping join-sets** — c waits on (a, b) while d waits on (b, e); b
cannot live in both wrappers. No real animation has needed one. If a
first consumer arrives, multi-name `after=` returns as a deliberate
change; until then, one name, one grammar for joins.

## Names and scope

- Names are **anim-scoped and flat** (confirmed at review — the scope
  is the anim, for all names): visible anywhere in the same `anim`,
  regardless of nesting depth (references may reach inside wrappers —
  see `after=b` above). Forward references are legal by the same rule —
  `after=` may name a group defined later in source; the cycle check is
  the only fence. Duplicate names in one anim are a compile error.
  (Today `after=` resolves per block; this widens it.)
- Names do not cross `anim` boundaries. A `play`ed anim's internal names
  are not reachable from the caller — internal names are not API, and
  renaming inside `badgeIntro` can never break `panelOpen`.

## Interaction with `each`

`each` keeps its wearer/stagger meaning and its bare paths, but becomes
a **step**: it can sit in a chain (today it is anim-top-level only), and
its contents are a chain of steps instead of a list of tracks. It
staggers its contents as units: everything inside one iteration
(including `and`-joined steps) shares that iteration's stagger offset.

```
anim menuAppear {
    each .MenuButton stagger-by=0.1 {
        group duration=0.2, easing=back_out {
            translate.y from=40, to=0
            and  opacity from=0, to=1, duration=0.3
            then scale.x to=1.05, duration=0.08, autoreverse=true
        }
    }
}
```

## Errors

Final wording will follow gogo's message + help convention; the content:

```
step needs a connective — add `and` (runs with previous) or `then` (runs after)
'card.opacity 1' has no target — write `to=1`
'{' needs a keyword — write `group { }`
'after=' needs a named group — wrap the step at line 2 in `group <name> { }`
'after=(a, b)' — `after=` takes one name; wrap a and b in a named group and reference it
'after=b-0.05' — `after=` takes a name only; to start early write `after=b, offset=-0.05`
'delay=-1' — a pause can't be negative; to start a step early, use `offset=` on that step
'then' and 'after=' on one step — a step is positional or referenced, not both
duplicate name 'b' — names are unique within an anim
'a' -> 'c' -> 'a' is a cycle — steps can't wait on themselves
warning: 'after=slide' is the previous step — write `then`
```

## Worked examples

Godot's three-tween sequence:

```
anim intro {
    group duration=1 {
        logo.translate.y to=100
        then logo.rotate to=90
        then logo.scale  to=1.5
    }
}
```

A toast — in, hold, out:

```
anim toast {
    group easing=back_out {
        plate.translate.y from=60, to=0, duration=0.25
        and  plate.opacity from=0, to=1, duration=0.3
        then delay=3
        then plate.translate.y to=60, duration=0.2, easing=quad_in
        and  plate.opacity to=0, duration=0.2
    }
}
```

A dialog — backdrop fades while the panel drops and settles, then
buttons pop:

```
anim dialogOpen {
    veil.opacity from=0, to=0.6, duration=0.2
    and group duration=0.3, easing=back_out {
        card.translate.y from=-40, to=0
        and  card.scale from=0.95, to=1
        then each .DialogButton stagger-by=0.05 {
            scale.x to=1.05, duration=0.08, autoreverse=true
        }
    }
}
```

A crossing edge — d follows b, c joins a and b without waiting for d:
see the `dashboardIn` example above.

## What this changes in shipped gogo (migration map)

Breaking deltas, in weight order:

1. **The flat form loses its header envelope.** Today's default form —
   `anim appear duration=0.5, easing=cubic_out { lines... }` with all
   lines parallel — becomes a wrapping `group` carrying the shared
   words, and the lines gain `and`. The two-line entrance pays one
   wrapper + connectives. (~16 anim blocks across 11 .gogo files, plus
   run_tests/run_panel_tests fixtures, `_anim`/`_track`/`_play` cards,
   prop-duration/-delay/-after/-autoreverse/-stagger-by cards, COOKBOOK,
   spec, REFERENCE, assist word lists, highlighter, formatter.)
2. **A second step with no connective becomes an error** — today flat
   lines run together silently. Every shipped anim block edits.
3. **`track` retires into `group`** — big bang, NO retirement
   machinery (decided at review): every in-repo file migrates in the
   round, and the addon has no external users yet, so a teaching error
   for `track` would serve nobody. Afterward `track` is an ordinary
   unknown word. Names become opt-in.
4. **The `delay=` attribute retires** (anim header + track lines) — its
   two jobs split into the `delay=` pause STEP and the signed `offset=`
   shift.
5. **`each` widens**: from anim-top-level-only container of tracks to a
   step whose body is a chain.
6. **`after=` scope widens**: per-block today, anim-flat here.
7. Unchanged: dotted targets (bare inside `each`), the motion channel
   and its fences, `key` blocks, `autoreverse=true` semantics (mirrored
   keys, doubled after-anchor), `repeat=forever`/`speed=` on the header,
   `on:*=play(name)`/`transition anim=` triggers, the runtime player
   (everything still bakes to absolute starts).

## Decisions (2026-07-18 review, user calls)

1. **Header envelope: REMOVED, not sugared** — then RESTORED under an
   honest name (addendum, same day). The clincher both times: `anim x
   duration=1` naturally reads "the whole anim is 1 second," and three
   `then`-steps would make that reading false (3 seconds effective).
   The user then caught the review's own inconsistency: plain
   `duration=` on a GROUP header lies identically. Resolution: the
   distributing word is **`duration-each=`** on group AND anim headers
   (per-step by name, a line's own `duration=` wins); `easing=` and
   `autoreverse=` distribute under plain names (no total reading
   exists for them); plain `duration=` on a header refuses with a
   teaching error. Spelling `duration-each` chosen over `step-duration`
   (stagger-by precedent; sorts beside `duration` in completion),
   accepting the mild overload of the word "each" (also the wearer
   block).
2. **Distribution + names: one scope story.** Step words resolve from
   the nearest enclosing group that sets them (lexical, any depth);
   names are anim-scoped and flat — forward references legal, cycles
   refused.
3. **Referenced steps stand alone.** One placement per step, connective
   XOR `after=`; the missing connective is the visible "this step
   floats" signal. Connective + `after=` on one step is a compile
   error, never a precedence rule.
4. **`play badgeIntro`, keyword-name.** The register decides the shape:
   lines are keyword-then-name, values are calls (`on:pressed=
   play(x)`). Same word, same meaning, both registers.
5. **Big bang, no retirement machinery.** All in-repo .gogo files,
   tests, cards and docs migrate in the implementation round; `track`
   and the header envelope become ordinary unknown-word refusals (the
   addon is pre-release — there are no external users to teach).

## Grounding fixes applied (vs the discussion draft)

Audit trail — what changed moving the external draft into gogo:

- Targets dotted everywhere outside `each` (`card.opacity`, not
  `opacity`) — gogo anims address named nodes; bare paths are the
  `each`-wearer form only.
- `stagger=` → `stagger-by=` (shipped spelling).
- Bare `autoreverse` flag → `autoreverse=true` (gogo has no bare flags;
  shorthand only as defined sugar).
- `rotate to=90deg` → `rotate to=90` (gogo's `rotate` is degrees by
  definition; there is no `deg` unit).
- Naming-shape reference `anim`/`style`/`control` → `anim`/`style`/
  `widget` (gogo's keyword).
- The draft's crossing-edge example wrote `and group d after=b { }`,
  violating its own positional-XOR-referenced rule; referenced steps
  now stand alone, and the no-connective error exempts them.
- Toast example: `and plate.opacity to=0` had no duration in scope
  (group only sets easing) — would error today; gave it `duration=0.2`.
- `color to=amber` → `header.background to=amber` (`background` is the
  animatable color prop; `color` is Rectangle/Circle fill — legal too,
  but the example's node is styled chrome).
- Anim names to lowerCamel single words (`badgeIntro`), matching every
  shipped anim.
- Added: keyframe (`key`) interplay, `repeat=`/`speed=` header
  survival, `delay=` attribute retirement, per-line `autoreverse=`
  widening, `each`-as-step widening, `after=` scope widening, bake-time
  `play` inlining note, migration map.
