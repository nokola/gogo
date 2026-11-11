N children from data — a lowercase structure word that draws nothing and
takes no room: the rows lay out as children of the repeat's parent (a
`Column`'s spacing spaces them). The collection arrives from code through
the repeat's node, found by its NAME; each assignment reconciles by key —
kept keys keep their nodes, changed data rebuilds, absent keys free, order
follows the collection.

Default binding — the item is `$item`, `by` keys it, and the `empty`
block is the zero-items screen (a plain repeat may leave it out; a
literal `empty { }` is legal: "considered, show nothing"):

```
Column spacing=8 {
    Label text="Today"
    repeat tasks by $item.id {
        empty { Label text="Nothing due today" }
        Label text=$item.title
    }
}
```

`as` renames the binding — for readability, or when `$item` would shadow a
theme value or widget param in scope:

```
Column spacing=4 {
    Label text="Brushes"
    repeat brushes as $brush by $brush.id {
        empty { Label text="no brushes yet" }
        Button text=$brush.name, on:activate=OnBrushActivate
    }
}
```

**To wire it up**, code hands the repeat its collection by the repeat's
node (`page.find("brushes")`, or the generated `NAMES.brushes`), every
time the collection changes. An item is a record of named fields:

```rust
fn task(id: i64, title: &str) -> Item {
    Item::Record(vec![("id".to_string(), ItemValue::Int(id)), ("title".to_string(), ItemValue::Str(title.to_string()))])
}
```

```rust
    // The whole collection, every time it changes. Rows are matched by
    // key: a kept key keeps its row.
    host.rt.set_items(&mut host.page, tasks, &[task(1, "Buy milk"), task(2, "Call mom")]).expect("two rows with different keys");
```

An event from inside a row carries the node it happened on, and
`Runtime::item_key(&page, ev.node)` is the key of the row around it.

`by` makes the repeat keyed — String or int keys; a duplicate key refuses
the whole assignment and the last good items stay live. A repeat with no
`by` is the counted form: code shows the first N rows with
`Runtime::set_count` and fills them itself (`capacity=` reserves rows up
front). Names inside the body are per-item local.

When the items need SELECTION (selected rows, a keyboard cursor,
activation) or SCROLLING, put the repeat inside a `List` instead of a
`Column` — there the row template's root must be a `ListItem`.
