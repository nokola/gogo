"This one is chosen" — universal on every node, dormant until a provider
flips it: a toggle-mode button (`toggle=true`) holds it while ON, a
`choice`'s Radios carry it, `Check` and `Toggle` hold it while ON (Toggle
flips it live as the thumb crosses the middle), a `List` flips it on the
selected item's root. It persists until the next flip, unlike the
momentary `held`; a standing state of the whole screen is an
`arrangement`.

```
style Chosen min_width=88, min_height=36 {
    transition any -> selected duration=0.1, easing=cubic_out
    when selected background=accent
}
Button muteButton .Chosen text="Mute", toggle=true
```

Inside a `List`, style the `ListItem` row root — the List drives the state
on the ROOT, so a `when selected` on a node inside the row never fires:

```
style TaskLook { when selected background=accent }
List picker {
    empty { }
    repeat tasks by $item.id {
        ListItem .TaskLook { Label text=$item.title }
    }
}
```
