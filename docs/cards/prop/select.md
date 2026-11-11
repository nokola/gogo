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

Under `one`, a tap selects the row and tapping the selected row never
deselects (only code clears it); under `many` taps toggle; under `off`
code writes are refused. Code reads the selection with
`Runtime::selected_keys` and writes it with `Runtime::set_selected`. A
`Change` event on the List fires on genuine selection change, and every
completed tap (or Enter/Space on a focused row) then fires `Activate` on
the row, under `off` too.
