Picks a `List`'s items panel — how the rows arrange. A closed set, each
mode owning its scroll axis and cursor keys; the block's props style the
panel itself (`spacing=` here is ROW spacing, separate from the List's
statics spacing). An inventory bag — items wrap into a grid, arrows walk
it in 2D, taps select:

```
List bag select=one {
    Label text="Inventory"
    empty { Label text="your bag is empty" }
    layout Flow spacing=6 {
        repeat loot by $item.id {
            ListItem padding=6 {
                Column spacing=2 {
                    Rectangle 48x48
                    Label text=$item.name
                }
            }
        }
    }
}
```

`Runtime::set_items` fills `loot`; a `Change` event on `bag` reports a
pick — the List's surface is unchanged by the panel choice.

| panel | rows | scrolls | arrows |
|---|---|---|---|
| `Column` | top-to-bottom — the default, no block needed | vertically | up/down |
| `Row` | left-to-right (a strip) | horizontally | left/right |
| `Flow` | wrap to the next line | vertically | geometric (2D) |
| `Tiles` | unit-lattice mosaic (`cols=`) | vertically | geometric (2D) |

The block holds exactly the List's one `repeat`; statics and `empty` sit
on the List body beside it (G9017 fences misuse). Under `layout Row` the
content-following words transpose: `width=hug` + `max_width=`.
