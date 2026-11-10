Two homes. On a `Slider` (any Range): the current value, between `min=`
and `max=`. On a `choice` line: the pre-selected Radio's NAME (omit for
none selected) — the same name its `change` wiring reports.

```
Slider sizeSlider min=1, max=100, value=40, width=200
```

```
Column sizePick spacing=4 {
    choice size value=medium {
        Radio small  text="Small"
        Radio medium text="Medium"
        Radio large  text="Large"
    }
}
```
