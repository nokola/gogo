The size cap — pairs with `width=hug` (content size, at most the cap) or `width=fill` (available space, at most the cap; D245 — see below).

On a hug axis: natural content size, at most the cap. Wrap-capable text hugs its UNWRAPPED extent and starts wrapping only when the cap binds; that is how a wrapping `Label` hugs instead of collapsing toward its longest word. Once the cap binds the box is as wide as the cap (a number and `fill` alike) and as tall as the wrapped lines, and a parent that hugs it is that tall too. The box re-fits live when code-behind swaps its text. The cap holds for any box under any parent, free or container: the box never reports more than its cap, and content wider than the cap is cut at the box's edge. `Scroll` and `List` carry it by their own doctrine: the capped axis SCROLLS past its cap (D138/D124). Two shapes are refused: a cap that reads the parent's space (`max_width=fill`, or any cap on `width=fill`) on a Row child — along the Row's own axis the slot is the frame; cap the Row itself — and `max_width=fill` inside a `Flow`.

Adding `min_width` forms the band (D135): `width=hug, min_width=100, max_width=300` clamps the natural size into the pair. The floor beats the cap (WPF clamp order) — a runtime `$param` floor that crosses above the cap wins; a literal floor above a literal cap refuses at build (G1013: "`min_width` above `max_width`").

`max_width=fill` caps at what a `fill` axis would get — parent frame minus region padding minus margins, live across resizes. On a `Scroll`, `width=hug, max_width=fill` is a build error (G1020): that side would grow with its content and never scroll — for a Scroll that fills its frame and scrolls, spell `width=fill`; for one that follows its content up to a size and then scrolls, cap at a number. On a fill-capped axis margins bind the measuring frame like they inset fill, whatever the alignment (a centered box sits in the middle of the room they leave):

```
Layers {
    Panel toast width=hug, max_width=fill, halign=center,
            margin_left=12, margin_right=12, padding=12 {
        Label tipText text="Saved!", wrap=true
    }
}
```

**Capped fill (D245)** — `width=fill, max_width=N`: take the available space, never more than N. On screens narrower than the cap the box fills edge to edge exactly like plain `fill`; when the screen is wider, the box stops at N and the stated side places it in the leftover (`halign=center`/`left`/`right`). With no side stated the box is centered, as in WPF: the compiler does not ask for one. This is the classic centered-column-with-a-maximum shape (CSS `width:100%; max-width:N; margin:auto`, WPF `MaxWidth` + Stretch):

```
Layers {
    Tiles catalog width=fill, max_width=380, halign=center, cols=2 {
        // fills a phone edge to edge; stops at 380 and centers on a tablet
    }
}
```

A capped fill works under a free parent (Layers/View root/slot) and across a stack (a Column child's width, a Row child's height). Along the stack's own axis (a Row child's `width=fill, max_width=N`) the compiler refuses it: wrap the box in a `Layers` (the Layers takes the Row's slot, the capped box places itself inside). Margins inset the measuring space on every side choice, like they inset plain fill. A `min_width` floor still beats the cap (D135); content wider than N does not: it is cut at the box's edge. A `Scroll` takes a capped fill too and still scrolls. `width=fill, max_width=fill` builds and changes nothing, since a fill axis already takes that room; with `halign=center` it is refused like a centered plain fill (G1011): nothing is left to position. A cap beside an EXACT width on the same line is a build error (G1019): the exact width is the width and the cap is never read — a box that takes the room it is given up to 40 is `width=fill, max_width=40`.
