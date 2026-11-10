Where a widget instance's children land — one per widget body (v1).

```
widget Card {
    Panel background=panel, corner_radius=6, padding=12 {
        slot
    }
}

Card { Label text="I fill the slot", font_size=14 }
```
