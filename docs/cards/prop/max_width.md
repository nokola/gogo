The size cap — pairs with `width=hug` (content size, at most the cap) or `width=fill` (available space, at most the cap; D245 — see below).

On a hug axis: natural content size, at most the cap. Wrap-capable text hugs its UNWRAPPED extent and starts wrapping only when the cap binds; that is how a wrapping `Label` hugs instead of collapsing toward its longest word. The box re-fits live when code-behind swaps its text. The cap holds for any box under any parent, free or container: the box never reports more than its cap, and content wider than the cap is cut at the box's edge. `Scroll` and `List` carry it by their own doctrine: the capped axis SCROLLS past its cap (D138/D124). Two shapes are refused: a cap that reads the parent's space (`max_width=fill`, or any cap on `width=fill`) on a Row child — along the Row's own axis the slot is the frame; cap the Row itself — and `max_width=fill` inside a `Flow`.

Adding `min_width` forms the band (D135): `width=hug, min_width=100, max_width=300` clamps the natural size into the pair. The floor beats the cap (WPF clamp order) — a runtime `$param` floor that crosses above the cap wins; a literal floor above a literal cap refuses at build (G1013: "`min_width` above `max_width`").

`max_width=fill` caps at what a `fill` axis would get — parent frame minus region padding minus margins, live across resizes. On a `Scroll` the compiler accepts it, but a `width=hug, max_width=fill` Scroll does not scroll that axis: it stops at its frame and cuts the rest — for a Scroll that fills its frame and scrolls, spell `width=fill`. On a fill-capped axis margins bind the measuring frame like they inset fill, whatever the alignment (the one place a centered axis takes margins):

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

A capped fill works under a free parent (Layers/View root/slot) and across a stack (a Column child's width, a Row child's height). Along the stack's own axis (a Row child's `width=fill, max_width=N`) the compiler refuses it: wrap the box in a `Layers` (the Layers takes the Row's slot, the capped box places itself inside). Margins inset the measuring space on every side choice, like they inset plain fill. A `min_width` floor still beats the cap (D135); content wider than N does not: it is cut at the box's edge. A `Scroll` takes a capped fill too and still scrolls. `width=fill, max_width=fill` adds nothing — a fill axis already takes exactly that space (the compiler refuses the pair only with `halign=center`, as it refuses a centered plain fill).
