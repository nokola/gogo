Display text. Font size is node-local (no inheritance) — give every Label
one, or set a `theme font_size=` default on an ancestor. `padding=` insets
the text within the label's box (a list row breathes without a wrapper);
a drawn background/border stays a `Panel`'s job.

```
Label title text="Brush size", font_size=18, font_color=text
Label row text="Round brush", font_size=15, padding=8
```
