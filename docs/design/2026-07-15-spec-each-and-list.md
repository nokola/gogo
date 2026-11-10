# Gogo Spec: `each` and `List`

Status: SUPERSEDED by `2026-07-15-spec-repeat-and-list.md` (settled after the
2026-07-15 design review: the repetition keyword is `repeat` — `each` is the
anim stagger block and REFERENCE reserved `repeat` for lists; selection rides
the shipped universal `checked` state; the keyed-identity core, inert keys,
and the non-goals survive). Kept for history.
Scope: Repetition construct (`each`), selection semantics (`List`), keying, empty states, materialization.

---

## 1. Design principles

List-like UI is three separable problems. Gogo keeps them separate:

| Problem | Owner | Spelling |
|---|---|---|
| Repetition — N children from data | `each` (compiler-owned construct) | one |
| Selection — which item is chosen | `List` (control) | one |
| Materialization — eager vs. virtualized | the container, at runtime | none (invisible) |

Consequences of the case doctrine: `each` and `empty` are lowercase (compiler-owned constructs). `List`, `Column`, etc. are capitalized (nodes in the tree — whether built-in controls or author-defined widgets).

There is no data-property list (`items=[...]` with a template reference) and no parallel-array convenience form (`items=`, `icons=`, ...). Both would be second spellings of repetition, and parallel arrays make invalid states (length mismatch) representable.

**Error doctrine.** Invalid structure cannot parse. Invalid types cannot compile. Invalid data halts — with a rich diagnostic, in all builds. The only states tolerated at runtime are genuinely pending ones (inert selection keys awaiting their items), never swallowed bugs. Every rule in this spec is an instance of this doctrine.

---

## 2. The `each` construct

### 2.1 Syntax

```
each $collection as item by $item.id {
    Label text=$item.title
}
```

Grammar:

```
each <binding> as <name> by <key-expr> { <children> }
```

- `<binding>` — a `$` data binding to a collection.
- `as <name>` — introduces the loop variable. Required.
- `by <key-expr>` — the identity key for each item. See §3.
- Ordering is fixed: `as` before `by`. Names are introduced left-to-right; the key expression may reference the loop variable because it is already in scope.

**Render order is collection order.** Items appear in the tree in the order they appear in the collection; `each` never reorders. `each` iterates **ordered collections only** (arrays). Dictionary iteration is future work — unordered iteration plus a deterministic render-order guarantee is a contradiction this spec refuses to paper over with whatever the runtime's hash order happens to be.

`each` is a **description, not a loop**. It declares that these children exist as a function of the collection. It does not promise that all children are instantiated — that decision belongs to the container (§5).

### 2.2 Where it may appear

Inside any container's child block. `each` is not specific to `List`.

```
Column spacing=8 {
    Header
    each $tasks as task by $task.id {
        TaskRow title=$task.title
    }
    Footer
}
```

Static and repeated children may be freely mixed. Child order in source is preserved in the tree: `Header`, then the expanded items, then `Footer`.

### 2.3 The `empty` block

An `each` may contain one `empty` block. Its children are shown if and only if the collection has zero items.

```
each $tasks as task by $task.id {
    empty {
        Label text="No tasks yet"
    }
    TaskRow title=$task.title
}
```

`empty` must be the **first** entry in the `each` body. Rationale: it is the base case of the repetition, so it is stated before the per-item case; the dominant failure mode of empty states is that they are never designed, and first position puts the zero-items question in front of every reader; and it gives the body one clean boundary — everything after the `empty` block is per-item, unconditionally.

`empty` replaces only the `each`'s own expansion. Static siblings of the `each` (a `Header` before it, a `Footer` after it) remain in the tree regardless of the collection's size.

Its children are ordinary author-owned widgets.

---

## 3. Keying

### 3.1 Why keys exist

Item identity determines where per-item state (focus, animation, selection, widget-local state) attaches when the collection changes. Index-based identity silently misattaches state on reorder, insert, or removal. Gogo therefore has no index fallback.

### 3.2 Rules

1. **`by` is required.** An `each` without `by` is a compile error with a fix-it suggesting `by $<name>.<field>` (or `by $<name>` for primitive collections).
2. **Key expressions are pure by grammar.** A key expression is the loop variable, or field access on it: `$task.id`, `$task.meta.uuid`, `$name`. No function calls, no operators, no literals. Unstable keys are unrepresentable, not linted.
3. **Key types are string and integer only.** The key expression's type is known at compile time; any other type — floats above all (precision, NaN) — is a compile error. There is no float-key escape hatch.
4. **Item-as-key is legal but not recommended.** For primitive collections, `by $item` uses the item's value as its key. Because duplicate keys are a hard error (§3.3), item-as-key is only correct for collections the author can guarantee are distinct. Collections carrying user-entered or external data should be ID-bearing records; wrapping primitives in a record with an `id` field is the supported pattern, not a workaround.
5. **Uniqueness is scoped per `each` instance.** Two sibling `each` blocks may reuse key values. For nested `each`, an item's identity is the composite path of keys from root to leaf; inner keys need only be unique within their own `each`.
6. **Selection is stored by key, never by index** (§4.3).

### 3.3 Duplicate key diagnostic

Duplicate keys cannot be caught at compile time. At runtime, a duplicate key within one `each` is an error, not a warning, and the diagnostic must include:

- the offending key value,
- the source data of **both** colliding items,
- the source location of the `each`.

There is no fix-it: the fix is always in the user's data model, and the diagnostic must make that clear.

**Behavior on collision: halt with the error, in all builds — dev and release alike.** A duplicate key is a developer bug; degrading silently (first-wins, drop, skip) would let keyed identity — the foundation selection and per-item state stand on — become quietly wrong in shipped software. If it's a dev bug, the dev must fix it; the crash is the mechanism that guarantees they find out.

---

## 4. The `List` control

### 4.1 What `List` adds over `Column`

`List` is the one control that owns selection semantics: selection state, keyboard navigation, focus movement, and item activation. A `Column` with `each` renders items; a `List` makes them selectable.

### 4.2 Selection mode

```
List tasks select=one, selected=$currentId {
    each $tasks as task by $task.id {
        TaskRow title=$task.title
    }
}
```

`select` is **required** — a `List` without it is a compile error with a fix-it offering the three modes. Selection is the reason `List` exists; the choice is made explicitly, never defaulted.

| Mode | Meaning | Type of `selected` |
|---|---|---|
| `none` | items are not selectable (List still provides focus, keyboard navigation, activation) | declaring `selected` is a **compile error** |
| `one` | zero or one item selected | key type or `none` (§4.3) |
| `many` | any subset selected | set of key values |

Invalid combinations do not compile. `select=none` plus `selected=` is rejected with a fix-it offering `select=one`.

### 4.3 Selection is keyed; `none` is the empty selection

The `selected` binding holds **key values** — whatever type the `by` expression produces. The compiler checks that the binding's type matches the key expression's type of the `List`'s **direct child `each`**. Nested `each` blocks inside item children do not participate in the `List`'s selection; their keys are identity only (§3.2.5).

For `select=one`, the no-selection state is the lowercase literal **`none`** (compiler-owned, per the case doctrine). The binding's type is "key or `none`":

- initial value is `none` unless the binding says otherwise,
- the app clears selection by assigning `none`,
- conditions may test it: `when $currentId == none { ... }`.

A sentinel value inside the key's own type (empty string, `-1`) is deliberately not used — it would make an invalid state representable.

**`none` the mode and `none` the literal share a spelling deliberately** — in both positions it means "absence of selection." Same concept, one spelling. Consequently `none` is a **reserved word**: a node name, `let`, or `$` binding named `none` is a compile error. This also reserves `none` as the DSL's general absence word for future optional values.

**`select=many`: the binding is semantically a set with a defined canonical form.** The underlying representation is an array (the runtime has no set type), but set semantics are enforced by contract:

- **assigning duplicate keys is a runtime error** — an assigned `[3, 3, 7]` halts with a diagnostic showing the duplicated key and the assigned value, same philosophy and same behavior as §3.3: a selection containing the same key twice means the app's model is already wrong, and degrading silently would hide the bug,
- keys written by user interaction never duplicate — the control maintains canonical form by construction,
- selection order is not meaningful and not preserved,
- canonical read-back order is collection order, with inert keys after, in their order within the most recent assignment (assignment replaces the set wholesale, so there is exactly one such order; deterministic reads, tests do not flake),
- the empty array is the no-selection state; `none` does not apply.

The `selected` binding is **two-way**: the control writes it on user interaction, and the app writes it to select programmatically. **Inert-key rule:** a selected key not present in the collection — whether the item was removed or the app assigned an absent key — is inert. It does not error, does not render as selected, and reads back until assignment or user interaction removes it. **User interaction is per-item**: toggling item A touches only A's key; inert keys are never evicted by interaction with other items. Data mutation and programmatic assignment behave identically. (Note the inert-key rule is deliberately observable contract: a key assigned before its item arrives becomes selected when the data loads — select-then-load flows work with no author code.)

Because selection is keyed, sorting or filtering the collection preserves the user's selection with no author code.

**GDScript interop.** At the binding boundary, `none` crosses as `null`; a `many` selection crosses as an `Array` in canonical form (an empty selection is `[]`, never `null`). Implementations must not invent wrapper or sentinel objects for absence.

### 4.4 Body composition

A `List` body contains **exactly one direct child `each`** — zero or more than one is a compile error. Static children are allowed before and after it (headers, footers, separators); they are never selectable, are skipped by keyboard navigation, and never appear in `activated`. The children of the `each`'s `empty` block are likewise non-selectable — there are no items to select when it shows. Only the `each`'s items participate in selection.

**Inside a `List`'s `each`, the per-item body is exactly one root node** — compile error otherwise, fix-it: wrap in a container. Selection, focus, hit-testing, and the item states (§4.6) all attach to a single node, so item = node must hold. The compiler never inserts an implicit wrapper — a node the author didn't write, with layout properties nobody chose, has no place in the tree. In plain containers (`Column`, `Row`), the `each` body may still produce multiple children per item; only `List` requires the constraint, because only `List` needs items to be units.

**Nested `List`s are legal by composition**, not by special case: a `List` inside another's item children owns its own direct child `each`, its own `select` mode, and its own `selected` binding. The two selections never interact.

### 4.5 Events

`List` emits (names indicative, final names per the event-naming doctrine):

- `activated` — item confirmed, carries **both the item's key and the item itself**. Every handler that received only a key would begin with a key→item lookup the control has already done; the control hands over the element, not the index.
- Selection changes are observed through the `selected` binding itself; there is no separate changed signal.

**Scope fence:** interaction gestures — which clicks, keys, or touches select, toggle, or activate — are defined by the control implementation per platform, not by this spec. This document specifies state and identity semantics only.

### 4.6 Item selection and focus states

Items render their own selection through the existing `when` state system — the same mechanism as `pressed` and `disabled`, no new machinery:

```
widget TaskRow $title="" {
    when selected background=$accentColor
    when focused outline=$focusColor
    Label text=$title
}
```

`List` exposes two states on each item's root:

- **`selected`** — true iff the item's key is in the selection. Follows the `selected` binding automatically; inert keys, having no item, style nothing.
- **`focused`** — true iff the item is under the keyboard cursor. At most one item at a time, and only while the `List` itself has focus. Focus is a cursor, selection is a choice; they are independent states (a `select=many` list navigates with focus and toggles into selection).

This is the **only** supported spelling of selection styling. Binding item visuals to the `selected` collection manually (a contains-check per item) is a second spelling with a per-frame cost; the states exist so it is never needed. In `select=none` mode, the `selected` state never fires; `focused` still does.

**Focus architecture: the cursor rides engine focus.** There is one focus system. The `List` cursor is engine focus on an item root; the `focused` state is derived from it, not tracked in parallel. "The `List` has focus" means engine focus is anywhere inside the `List` — including on a focusable child within an item (a delete button inside a row). How Tab and arrows move between item roots and item internals is gesture design, behind the §4.5 scope fence; but the invariant is architectural and fixed: no second cursor, ever.

**States without providers never fire — by definition, with a lint.** `selected` and `focused` are provided only by `List`. A widget consuming them, instantiated where no provider is in scope, has those `when` blocks inert — this is *valid* (a `TaskRow` in a style gallery, a settings preview, a drag ghost is a correct program with dormant states), consistent with the general `when` rule (a never-disabled widget's `when disabled` also never fires), and it is dead code, not an invalid state — so per the error doctrine it is not an error. It is, however, a default-on **compile warning** at the instantiation site where statically knowable ("`TaskRow` consumes `selected`; no provider in scope — state never fires here"), because the alternative discovery mechanism is an hour of wondering why the highlight doesn't render. State consumption is part of a widget's interface and propagates transitively through composition, which is what makes the check decidable.

*(Property names in this section's example — `background`, `outline` — are illustrative pending the property vocabulary; the state mechanism is the normative part.)*

---

## 5. Materialization

`each` never promises eagerness. Materialization is a runtime concern, never a syntax concern:

- Non-selecting containers (`Column`, `Row`, etc.) instantiate all items. Authors wanting plain eager repetition choose these containers.
- **`List` owns virtualization internally.** Whether a `List` instantiates all items or only visible ones plus a margin is an implementation decision of the control (potentially heuristic, potentially a tuning property later). There is no separate virtualized control and no second dialect — a `VirtualList` would fork the selection semantics of §4, and one canonical spelling forbids it.

The `each` syntax is identical in all cases. Requirement this places on `each`: item children must be constructible from `(item, key)` alone — which the pure-key rule and binding-based item access already guarantee. Note that virtualization affects widgets only, never data: the collection, keys, and selection are always fully materialized, which is why `activated` can always carry the item (§4.5).

---

## 6. Hot reload

Keyed identity gives hot reload its preservation rule for free: **runtime state survives reload by key**. Keys come from data, not markup, so editing item children, styles, or static siblings preserves selection, focus, and per-item state — the reloaded tree reattaches state wherever the same keys reappear.

Two edits invalidate keys, and both **reset selection** (`none` for `one`, empty for `many`). Reset **writes through the two-way binding** — the app's bound variable is actually assigned, so control state and app state never diverge; "reset" is never the control silently ignoring a stale binding. The two edits:

- **Changing the `by` expression.** Old keys were produced by an expression that no longer exists; carrying them forward would attach state to coincidentally-equal values. Reset is the honest answer.
- **Changing the `select` mode.** There is no non-arbitrary mapping between a `many` selection and a `one` selection (or between either and `none`). Reset, never guess.

Focus (§4.6) follows the same rule: preserved by key when keys survive the edit, cleared when they do not.

---

## 7. Diagnostics summary

| Condition | When | Kind | Fix-it |
|---|---|---|---|
| `each` without `by` | compile | error | insert `by $<name>.<field>` / `by $<name>` |
| impure key expression | parse | error | — (grammar rejects) |
| key type not string/integer | compile | error | — (shows key expression and its type) |
| `List` without `select` | compile | error | insert `select=one` / `many` / `none` |
| `List` with zero or multiple direct child `each` blocks | compile | error | suggest wrapping extra repetition in a non-`List` container, or merging the collections |
| `none` used as a node name, `let`, or binding name | compile | error | rename (reserved word) |
| `selected` with `select=none` | compile | error | change to `select=one` |
| `selected` type ≠ key type | compile | error | show both types and the `by` expression |
| duplicate key within one `each` | runtime | error | — (shows key + both items' data; halts, all builds) |
| duplicate keys in a `selected` assignment (`many`) | runtime | error | — (shows duplicated key + assigned value; halts, all builds) |
| multiple root nodes per item in a `List`'s `each` | compile | error | wrap the item body in a container |
| state consumed with no provider in scope (e.g. `when selected` outside a `List`) | compile | **warning** | — (state never fires here; dormancy is legal, §4.6) |
| `empty` not first in `each` body | parse | error | move `empty` to start |

---

## 8. Non-goals

- **No `items=` data-property form.** Second spelling of repetition; template-by-name fails at a distance.
- **No parallel-array convenience** (`items=` + `icons=` + ...). Length mismatch would be a representable invalid state.
- **No index keys, ever.** Not even as an explicit opt-in.
- **No inline filtering or sorting on `each`** (`where`, `sort`, and kin). `each` renders a collection; it does not query one. Transformations are the data layer's job — derive a collection app-side and bind that. This keeps `each` a pure description and keeps every collection the UI shows nameable and testable outside the UI.
- **No repeat-N form** (`each 5 { ... }`). Skeleton placeholders and star ratings iterate a collection like everything else — derive the array app-side. A count form would be a second spelling of repetition with no keys, which is index identity by the back door.
- **No float keys, no sentinel "no selection" values.** Both make invalid states representable.
- **No separate virtualized syntax.** Materialization is the container's business.

---

## 9. Examples

Minimal string list (item-as-key — legal only because the collection is known-distinct; user-entered or external data must use ID-bearing records, §3.2.4):

```
List themes select=one, selected=$picked {
    each $themeNames as name by $name {
        Label text=$name
    }
}
```

Task list with empty state, multi-select, and static children inside the `List` (header/footer rows are non-selectable and skipped by keyboard navigation, §4.4):

```
List tasks select=many, selected=$checkedIds {
    Label .SectionHeader text="Today"
    each $tasks as task by $task.id {
        empty {
            Label .Muted text="Nothing due today"
        }
        TaskRow title=$task.title, done=$task.done
    }
    Label .Muted text=$summary
}
```

Clearing selection programmatically (`select=one`): assign `none` to the bound value — `$picked = none`. Selecting programmatically: assign a key; absent keys are inert (§4.3).

Non-selectable repetition — no `List` needed:

```
Row spacing=12 {
    each $badges as badge by $badge.id {
        Image texture=$badge.icon, 24x24
    }
}
```
