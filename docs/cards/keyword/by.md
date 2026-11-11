The `repeat` identity clause. With it, items reconcile by key: reorder,
insert and remove keep per-item state attached to the right items. A
repeat with no `by` is the counted form: code shows the first N rows with
`Runtime::set_count` and fills them itself.

```
Column {
    Label text="three key shapes"
    repeat tasks by $item.id { empty { } Label text=$item.title }
    repeat names by $item { empty { } Label text=$item }
    repeat rows by $item.meta.uuid { empty { } Label text=$item.name }
}
```

The path starts at the item binding (pure by grammar — no calls, operators
or literals; a dotted path such as `$item.meta.uuid` names ONE field,
`meta.uuid`, because an item is a flat record; bare `by $item` = the item
IS its key), so every keyed repeat line shows its binding at least once.
Keys must be String or int; duplicates refuse the whole assignment,
loudly, and the last good items stay live.
