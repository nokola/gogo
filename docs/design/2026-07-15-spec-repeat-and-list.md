# Gogo Spec: `repeat` and `List`

Status: Settled 2026-07-15 (design review of `2026-07-15-spec-each-and-list.md`
plus discussion). `repeat` ships first; `List` is designed here and builds in
its own round on top of it.
Supersedes: `2026-07-15-spec-each-and-list.md` (kept for history).

---

## 1. Design principles

List-like UI is three separable problems. Gogo keeps them separate — and
spends new syntax on exactly one of them:

| Problem | Owner | What it costs the language |
|---|---|---|
| Repetition — N children from data | `repeat` (compiler-owned construct) | the one new construct |
| Selection — which item is chosen | `List` (control) | **zero new states**: rides shipped `checked`/`focus` |
| Materialization — eager vs. virtualized | the container, at runtime | none (invisible; eager in v1) |

Reuse before invention. Shipped gogo already owns most of a list control:

- "this one is chosen" is the universal **`checked`** state — that is
  literally how `choice`/Radio works today (CheckBox + group). `List`
  becomes its third provider; item styling is `when checked ...`.
- the keyboard cursor is **engine focus** on an item root, styled by the
  universal **`focus`** state. One focus system, no second cursor.
- change/activation wiring follows the **`choice` convention**
  (`On<Name>Changed`), because GDScript cannot observe a property write —
  signals are the honest spelling.
- the selection value is a **control `$param`** (live prop on the root, code
  sets and reads it) — the same shipped two-way shape as Slider's `value`,
  already reconciled with the one-writer doctrine.

**Error doctrine.** Invalid structure cannot parse. Invalid declarations
cannot build. Invalid **data is refused, loudly, and the last good state
stays live** — gogo's shipped error philosophy ("while a file has an error,
the last good tree stays live") applied to data. A refused assignment prints
a rich diagnostic and changes nothing; it never half-applies and never halts
shipped software. Refusal makes the bug impossible to miss in dev and CI
without shipping a crash to a painter.

Case doctrine: `repeat` and `empty` are lowercase (structure); `List` is
capitalized (an addressable node).

---

## 2. The `repeat` construct

### 2.1 Syntax

```
repeat tasks as $task by $task.id {
    empty { Label .Muted text="Nothing due today" }
    TaskRow title=$task.title, done=$task.done
}
```

Grammar (delta to the node EBNF):

```ebnf
node   = TYPE-IDENT ,
         { name | "." STYLE-IDENT | flag | clause | property | param-decl
           | arg | arrow-endpoint | "," } ,
         [ block ] , terminator ;
clause = ( "as" | "by" ) , value ;   (* keyword-anchored; meaningful on repeat lines *)
```

- **`repeat <name>`** — the name is **required**: it is the address
  `set_items` targets. A nameless `repeat` is a build error.
- **`as $binding`** — optional; declares the item binding. Omitted, it
  defaults to **`$item`** — defined sugar: `repeat tasks by $item.id` is
  exactly `repeat tasks as $item by $item.id`. Declared with the sigil,
  like every other `$` introduction in the language (`$param=default`,
  `$name = value`).
- **`by <key-path>`** — **required**; the identity key. The path must start
  at the item binding: `by $task.id`, `by $item.meta.uuid`, or bare
  `by $item` (primitive collections — the item is its own key). Anything
  else cannot parse (purity by grammar: no calls, no operators, no
  literals). Because `by` is required and must name the binding, **every
  `repeat` line displays its item variable** — the binding is announced in
  situ even in the sugar form.
- **Clauses are comma-free** (the keyword is the delimiter — the
  arrow-endpoint precedent); `=`-members keep their commas. Canonical
  order: `repeat <name> [as $b] by <path> {`. `as` before `by`
  (introduce, then use); `as $task by $item.id` is a build error.
- `as`/`by` are reserved **on repeat lines only** (a repeat named `as` or
  `by` errors with a rename hint — the `interactive` precedent). Node names
  elsewhere stay free.
- **Render order is collection order.** `repeat` never reorders, never
  filters, never sorts.

### 2.2 The item binding

- `$binding.field` (dotted chain allowed) reads a field of the current
  item; bare `$binding` is the item itself. One register everywhere: the
  same spelling in the `by` clause and in children's props.
- Item bindings are **static values re-applied at each assignment** — the
  bare-`$`-is-static doctrine survives untouched; no `live()` inside
  `repeat` (assignment IS the update event, §3).
- If the binding name collides with a `$` value already in scope (theme
  value or widget param), that is a build error with a rename fix-it
  (`as $<other>`). The rename is the escape hatch that makes the `$item`
  default safe.
- Dotted `$` paths outside a repeat body are an error naming the rule
  (field access exists only on repeat bindings).

### 2.3 Where `repeat` may appear

Inside any container's child block, freely mixed with static siblings —
source order is tree order (`Header`, the expanded items, `Footer`).
Multiple repeats per container are legal.

Not legal in v1 (one fence, one reason — **addressability**: `set_items`
targets a repeat by name, and both cases multiply the name per instance):

- **inside a widget/control body** — a widget used three times would give
  one name three reconcilers; the future form sources the collection from
  a widget param,
- **nested inside another `repeat`'s body** — the future form sources the
  inner collection from the outer item (`repeat $task.subtasks …`); see §8.

### 2.4 The `empty` block

Every `repeat` contains exactly one `empty` block, **first** in the body —
it is **required** (user call, 2026-07-15), even when there is genuinely
nothing to show: a literal `empty { }` is legal and means "considered:
show nothing". Rationale: the empty state is the classic never-designed
screen, so every list answers the zero-items question VISIBLY; the block
appearing in every repeat is also what keeps the feature discoverable —
nobody learns about `empty` from documentation they didn't read. First
position states the base case before the per-item case and gives the body
one clean boundary — everything after `empty` is per-item. It must be a
block (not a `when`) because `when`s overlay properties and this swaps
structure.

Its children show iff the collection has zero items — including the
initial state, before any assignment. `empty` replaces only the repeat's
own expansion; static siblings stay regardless.

### 2.5 Erasure and per-item scope

- `repeat` is **erased**, like `choice`: the items become the parent's
  children in place (a `Column`'s `spacing` spaces them; no phantom wrapper
  with layout props nobody chose). Anchoring within the parent is internal
  machinery.
- **Names inside a repeat body are per-item local** (the widget-body rule):
  visible to the body's own styles/whens, never in the view's named map.
- **Handlers inside a repeat body carry the item's key.** An
  `on:<event>=Handler` (or auto-wire) declared inside the body binds the
  item's key as the handler's **last** argument: `Handler(…usual args, key)`
  — trailing because that is Godot's native `Callable.bind` shape (bound
  args append), so it works for any signal arity with no wrapper machinery.
  A row's delete button knows its row; defined sugar, documented, tested.

---

## 3. Data: assignment is the change event

Gogo has no ambient data store and does not watch your arrays. The
collection is handed to the repeat, whole, by name:

```gdscript
Gogo.set_items(view, "tasks", tasks)   # any code-side caller; C# via the autoload
```

On assignment gogo **reconciles by key**:

- **new keys** instantiate (in collection order),
- **kept keys with equal data** keep their nodes untouched — focus, states,
  anim positions survive,
- **kept keys with changed data** rebuild their item subtree in place
  (Dictionaries/primitives compare by value; an Object item is assumed
  changed — gogo cannot cheaply diff it),
- **absent keys** free their nodes,
- order follows the collection (`move_child`, no rebuild for pure
  reorders).

Rules:

- Accepted items: an `Array` of Dictionaries/Objects (field access) or
  primitives (`by $item`). `null` is **refused** with a diagnostic ("pass
  [] to clear"); `[]` clears (the `empty` block shows).
- **The last assigned collection re-applies across rebuilds** — theme-
  variant switches and hot reload rebuild the tree, then re-run the last
  assignment, so lists never silently empty on a theme flip.
- Assignment before the view finishes building, or to an unknown repeat
  name, is refused with the known-names list.

## 4. Keys

- Key values must be **String or int**, checked at assignment (gogo binds
  GDScript data; there is no compile-time schema — pretending otherwise
  would be vapor). Mixed int/String within one collection is refused.
- **Refusal family** — each of these refuses the whole assignment, keeps
  the previous items, and prints a rich diagnostic (offending key/index,
  the item data — both items for duplicates — and the repeat's source
  line): duplicate key; missing `by` field on an item; key of wrong type;
  non-Array/`null` items; unknown repeat name.
- No index identity, ever — there is no fallback and no opt-in.
- Uniqueness is scoped per repeat instance; sibling repeats may reuse keys.

---

## 5. The `List` control (D118 — designed 2026-07-15, built 2026-07-15)

> **Amended by D121** (`2026-07-16-spec-listitem-and-self-scroll.md`):
> List scrolls itself (the "composes with Scroll" call below is retired),
> and the one-root-per-item rule tightened to "the root is a `ListItem`".

`List` is the one control that owns selection semantics. Its body is static
children plus **exactly one** direct child `repeat` (zero or several is a
build error). It is a gogo composite control (the `Toggle` shape: no native
backend).

```
List picker select=many {
    Label .SectionHeader text="Today"
    repeat tasks by $item.id {
        empty { Label .Muted text="Nothing due today" }
        TaskRow title=$item.title
    }
}
```

- **`select=`** — `one` (default; the `choice` precedent says one-of-N is
  the overwhelming case and it is not a trap-default), `many`, or `off`
  (activation/focus without selection — menus; reuses gogo's shipped
  opt-out word, no `none` literal, no reserved word).
- **Selection state IS `checked`.** `List` flips the universal `checked`
  state on item roots; item styling is `when checked …` — always legal,
  dormant when unprovided, exactly like every universal state today. No
  new state names, no dormancy doctrine, no `focused`-vs-`focus` split.
- **The cursor is engine focus** on item roots (universal `focus` state
  styles it). Statics and `empty` content are skipped by navigation and
  never selectable. Which gestures select/toggle/activate is per-platform
  control implementation — state and identity semantics only, here.
- **`selected` is a control `$param`**: a key (or `null`) for `one`, an
  Array of keys for `many` (`[]` = none). Code reads/writes it like any
  control param; interaction writes it back — the shipped Slider-`value`
  shape. Changes auto-wire **`On<Name>Changed(selected)`**; activation
  auto-wires **`On<Name>Activated(key, item)`** (`on:changed=` /
  `on:activated=` are the explicit forms).
- **Inert keys**: a selected key with no present item renders nothing,
  errors nothing, and reads back until reassigned or the item arrives —
  select-then-load works with no author code. Read-back order for `many`:
  present keys in collection order, then inert keys in the order they
  entered the selection (stable, deterministic).
- **One root node per item** inside a `List`'s repeat (item = the unit
  focus/checked/hit-testing attach to) — build error with a wrap fix-it;
  plain containers stay unrestricted. The compiler never inserts an
  implicit wrapper.
- A duplicate key in a `selected` assignment is refused (the §4 family).
- Virtualization is `List`-internal future work; v1 is eager. The
  invariant that enables it is stated now: item subtrees are constructible
  from (item, key) alone. `List` composes with `Scroll` for scrolling in
  v1.

## 6. Hot reload

Keys come from data, not markup, so reload preserves by key: the rebuilt
tree re-applies the last assigned collections (§3), and `List` re-applies
`selected` — state reattaches wherever the same keys reappear. Edits that
invalidate identity reset honestly: changing a repeat's `by` path clears
that repeat's derived state; changing `select=` mode resets selection
(`null`/`[]`), writing through the param so app reads never diverge.

## 7. Diagnostics summary

Build-time (codes assigned at implementation; each gets a REGISTRY entry +
`docs/errors/` page):

| Condition | Kind | Fix-it |
|---|---|---|
| `repeat` without a name | error | name it (the `set_items` address) |
| `repeat` without `by` | error | insert `by $item.<field>` / `by $item` |
| `by` path not rooted at the item binding | error | shows the binding |
| impure key path | parse error | — (grammar rejects) |
| binding collision with in-scope `$` value | error | rename via `as $<other>` |
| repeat named `as`/`by` | error | rename hint |
| nested `repeat` | error | v1 fence, names the future form |
| `repeat` inside a widget/control body | error | v1 fence (addressability), names the future form |
| missing `empty` block | error | add `empty { ... }` (or bare `empty { }`) first — required |
| second `empty` / `empty` not first | error | one `empty`, first |
| dotted `$` path outside a repeat body | error | names the rule |
| props on the repeat line | error | repeat takes clauses only (v1) |

Runtime (the §4 refusal family): duplicate key, missing `by` field, wrong
key type, mixed key types, `null`/non-Array items, unknown repeat name —
all refuse the assignment, keep the last good items, print the rich
diagnostic (error overlay + Output panel, the shipped surfaces).

`List` adds (its round): missing/multiple direct child `repeat`; multiple
roots per item; unknown `select=` value; duplicate keys in a `selected`
assignment (runtime refusal).

## 8. Non-goals / not in v1

- **No `items=` data-property form** in v1 — collections are code-set;
  when a data-binding layer exists, `items=` joins as an ordinary prop (the
  grammar slot is already there). Not a second spelling: the block is the
  template either way.
- **No nested repeats (v1)** — future form sources the inner collection
  from the outer item (`repeat $task.subtasks as $sub by $sub.id`); the
  `as` rename already gives outer-item access its spelling.
- **No index keys, no index binding** — zebra striping and "3 of 10"
  derive a field app-side.
- **No inline filtering/sorting** (`where`, `sort`) — ever. `repeat`
  renders a collection; deriving it is the data layer's job, which keeps
  every collection the UI shows nameable and testable outside the UI.
- **No repeat-N form** (`repeat 5 {}`) — keyless counting is index
  identity by the back door; derive an array.
- **No dictionary iteration** — unordered source + deterministic render
  order is a contradiction; deferred until a defined ordering exists.
- **No `none` literal / reserved word** — absence is omission in `.gogo`
  and `null`/`[]` at the code boundary (v1 has no declarative initial
  selection; data keys aren't knowable at author time anyway).
- **No separate virtualized control** — materialization is the container's
  business (§5).
- **Item enter/exit animation is named future work** — keyed identity is
  precisely what enables Metro-style list motion; it will be an `anim`
  question, not new `repeat` grammar.

## 9. Examples

```
// Plain repetition — any container; statics mix freely
Column spacing=8 {
    Label .SectionHeader text="Today"
    repeat tasks by $item.id {
        empty { Label .Muted text="Nothing due today" }
        TaskRow title=$item.title, done=$item.done
    }
    Footer
}

// Primitives — the item is its own key (legal when values are distinct;
// ID-bearing records are the pattern for user/external data). A literal
// `empty { }` is the required block saying "considered: show nothing".
Row spacing=12 {
    repeat badges by $item {
        empty { }
        Image "res://icons/badge.png" 24x24
    }
}

// Renamed binding (collision escape; also the future nested-outer spelling)
repeat brushes as $brush by $brush.id {
    empty { Label text="no brushes" }
    BrushRow icon=$brush.icon, name=$brush.name
}

// Selection (List round)
List picker select=one {
    repeat brushes by $item.id {
        empty { Label text="no brushes" }
        BrushRow name=$item.name    // when checked / when focus style it
    }
}
```

```gdscript
Gogo.set_items(view, "tasks", [
    {"id": 1, "title": "Sketch", "done": false},
    {"id": 2, "title": "Ink",    "done": true},
])
# later — assignment reconciles by key: node 1 kept, 2 freed, 3 new
Gogo.set_items(view, "tasks", [
    {"id": 1, "title": "Sketch", "done": true},
    {"id": 3, "title": "Paint",  "done": false},
])
```
