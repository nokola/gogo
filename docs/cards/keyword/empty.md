A `repeat`'s zero-items content — shown iff the collection is empty (the
initial state included, before the first `set_items`). REQUIRED, exactly
one per repeat, FIRST in the body: every list answers the zero-items
question visibly, the base case reads before the per-item case, and
everything after it is per-item. Genuinely nothing to show? A literal
`empty { }` says so explicitly.

```
Column {
    repeat tasks by $item.id {
        empty {
            Label text="No tasks yet"
        }
        Label text=$item.title
    }
}
```

It replaces only the repeat's own expansion — static siblings (headers,
footers) stay regardless. A block, not a `when`: it swaps structure, which
property overlays can't.
