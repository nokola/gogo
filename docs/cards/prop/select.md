A `List`'s selection mode — `one` (default), `many`, or `off`. Closed set;
`off` is gogo's shipped opt-out word (activation and focus still work —
menus).

```
List picker select=many {
    empty { Label text="no tags" }
    repeat tags by $item {
        ListItem { Label text=$item }
    }
}
```

Under `one`, `selected` is a key or null (tapping the selected item never
deselects — null comes from code); under `many` it's an Array of keys and
taps toggle; under `off` writes are refused. `change(selected)` fires on
genuine selection change; double-tap or Enter fires `activate(key, item)`
(`On<Name>Activate` by convention — a single tap activates under `off`,
where there is no selection to move).
