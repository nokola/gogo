The `repeat` identity clause — REQUIRED. Items reconcile by key: reorder,
insert and remove keep per-item state attached to the right items. There is
no index fallback, ever.

```
Column {
    Label text="three key shapes"
    repeat tasks by $item.id { empty { } Label text=$item.title }
    repeat names by $item { empty { } Label text=$item }
    repeat rows by $item.meta.uuid { empty { } Label text=$item.name }
}
```

The path starts at the item binding (pure by grammar — no calls, operators
or literals; nested fields fine; bare `by $item` = the item IS its key), so
every repeat line shows its binding at least once. Keys must be String or
int; duplicates refuse the whole assignment, loudly, and the last good
items stay live.
