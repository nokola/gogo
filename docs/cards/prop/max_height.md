The size cap, height edition. On `height=hug` (D132): natural content size, at most the cap, re-fitting live as content changes. On `height=fill` (D245): the available space, at most the cap, with `valign` stating where the box sits when the cap binds (centered when none is stated) — see `max_width` for the full shape (margins inset the measuring space; floor beats cap). `max_height=fill` caps at what a `fill` axis would get (parent frame minus region padding minus margins, live across resizes). The cap holds for any box under any parent, free or container: the box never reports more than its cap, and content taller than the cap is cut at the box's edge. `Scroll` and `List` scroll past the cap instead — see below. Two shapes are refused: a cap that reads the parent's space (`max_height=fill`, or any cap on `height=fill`) on a Column child — along the Column's own axis the slot is the frame; cap the Column itself — and `max_height=fill` inside a `Flow`. Adding `min_height` forms the min/max band (D135) — the floor beats the cap (WPF clamp order); see `max_width`.

On a `List` (and since D138 a `Scroll` — the same doctrine) it is the upper bound when the box sizes to its content: it follows the content up to the cap (epx), then stops and the scroll takes over — the centered menu grows while there's room, scrolls when there isn't (WPF's `MaxHeight` answer). The minimum stops at the cap, so the parent gives it exactly that.

```
Column menu {
    Button newGame text="New Game"
    List picker height=hug, max_height=400 {
        empty { Label text="nothing yet" }
        repeat tasks by $item.id {
            ListItem { Label text=$item.title }
        }
    }
}
```

On a List the cap requires `height=hug` (the capped-fill pairing stays
off Lists — a plain viewport List already takes exactly what its parent
offers; cap that with `height=`). Bare `height=hug` is the uncapped
form: it grows with the items forever, and content past the screen
overflows instead of scrolling.
