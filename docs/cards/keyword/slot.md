Where a widget instance's children land. A body may also NAME slots
(`slot header { Label text="untitled" }`, the braces holding the default
content): an instance fills one with its own `slot header { ... }` child,
and its other children land in the bare `slot`.

```
widget Card {
    Panel background=panel, corner_radius=6, padding=12 {
        slot
    }
}

Card { Label text="I fill the slot", font_size=14 }
```
