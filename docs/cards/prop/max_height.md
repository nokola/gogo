The size cap, height edition. On `height=hug` (D132): natural content size, at most the cap, re-fitting live as content changes. On `height=fill` (D245): the available space, at most the cap, with a REQUIRED `valign` stating where the box sits when the cap binds — see `max_width` for the full shape (free parent only; margins inset the measuring space; floor beats cap). `max_height=fill` caps at what a `fill` axis would get (parent frame minus region padding minus margins, live across resizes; not on a `Scroll` — a filling Scroll axis already scrolls past its frame — and not under a container, which grants no frame to read). Under a CONTAINER parent the cap needs a box that REPORTS its own minimum (D144): a `Layers` clamps its reported minimum at the cap — the container grants it honestly, and content past the cap DRAWS past the box (no clip; WPF's unclipped MaxHeight — the mode-row-with-a-wrapping-translation shape). `Scroll` and `List` scroll past the cap instead — see below; any other box under a container refuses toward Layers. Under free parents hard minimums always win (a cap never shears content). Adding `min_height` forms the min/max band (D135) — the floor beats the cap (WPF clamp order); see `max_width`.

On a `List` (and since D138 a `Scroll` — the same doctrine) it is the upper bound when the box sizes to its content: it follows the content up to the cap (epx), then stops and the scroll takes over — the centered menu grows while there's room, scrolls when there isn't (WPF's `MaxHeight` answer). The minimum stops at the cap, so engine containers grant it honestly.

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
