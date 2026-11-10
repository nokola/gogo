The size cap — pairs with `width=hug` (content size, at most the cap) or `width=fill` (available space, at most the cap; D245 — see below).

On a hug axis: natural content size, at most the cap. Wrap-capable text hugs its UNWRAPPED extent and starts wrapping only when the cap binds; that is how a wrapping `Label` hugs instead of collapsing toward its longest word. The box re-fits live when code-behind swaps its text. Under a CONTAINER parent the cap needs a box that REPORTS its own minimum (D144): a `Layers` clamps its reported minimum at the cap — the container grants it honestly, and content past the cap DRAWS past the box (no clip; WPF's unclipped MaxWidth/MaxHeight, tucking under the next sibling). `Scroll` and `List` carry it by their own doctrine: the capped axis SCROLLS past its cap (D138/D124). Any other box under a container refuses the cap toward Layers. Under free parents hard minimums always win (a cap never shears content).

Wrap-capable text is `Label` and, since D135, a raw-register `RichTextLabel` — give the latter `fit_content=true` (its engine minimum counts no content otherwise; the shaped minimum then lands the box when it enters the tree).

Adding `min_width` forms the band (D135): `width=hug, min_width=100, max_width=300` clamps the natural size into the pair. The floor beats the cap (WPF clamp order) — a runtime `$param` floor that crosses above the cap wins; a literal floor above a literal cap refuses at load ("the pair can never both hold").

`max_width=fill` caps at what a `fill` axis would get — parent frame minus region padding minus margins, live across resizes. It doesn't go on a `Scroll` (a filling Scroll axis already scrolls past its frame — spell `width=fill`). On a fill-capped axis margins bind the measuring frame like they inset fill, whatever the alignment (the one place a centered axis takes margins):

```
Layers {
    Panel toast width=hug, max_width=fill, halign=center,
            margin_left=12, margin_right=12, padding=12 {
        Label tipText text="Saved!", autowrap_mode=word_smart
    }
}
```

**Capped fill (D245)** — `width=fill, max_width=N`: take the available space, never more than N. On screens narrower than the cap the box fills edge to edge exactly like plain `fill`; when the screen is wider, the box stops at N and the stated side places it in the leftover. The side is REQUIRED (`halign=center`/`left`/`right` — a load error tells you when it's missing): WPF silently centers here, and gogo makes the reader see where the box goes instead. This is the classic centered-column-with-a-maximum shape (CSS `width:100%; max-width:N; margin:auto`, WPF `MaxWidth` + Stretch):

```
Layers {
    Tiles catalog width=fill, max_width=380, halign=center, cols=2 {
        // fills a phone edge to edge; stops at 380 and centers on a tablet
    }
}
```

A capped fill sits under a FREE parent (Layers/plain Control/View root/slot) — it reads its parent's space and places itself in it, which a container child can't do; under a container, wrap it in a `Layers` (the Layers takes the container's slot, the capped box places itself inside). Margins inset the measuring space on every side choice, like they inset plain fill. The floor still beats the cap (D135): rigid content wider than N wins. It doesn't go on a `Scroll` (a filling Scroll axis already scrolls past its frame), and `width=fill, max_width=fill` refuses — a fill axis already takes exactly that space.
