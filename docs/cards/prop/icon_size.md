A Button icon's maximum width (epx) — the icon scales down to fit and
never dictates the button's size. The icon itself is `icon=` (a `res://`
path).

The Rust crate accepts both words so that old files build, and draws no
icon yet: put an `Image` inside the Button to show one.

```
Button undoButton text="Undo", icon_size=20, min_height=36
```
