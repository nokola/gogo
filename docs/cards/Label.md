Display text. A Label with no `font_size=` takes the nearest one above it
(a `theme font_size=` line in an ancestor's body, or the `font_size=` of
a Button it sits in), else the `theme { font_size = … }` default, else
15. `padding=` insets the text within the label's box (a list row
breathes without a wrapper); `background=` and a border draw a plate
behind the text, as on any box.

```
Label title text="Brush size", font_size=18, font_color=text
Label row text="Round brush", font_size=15, padding=8
```
