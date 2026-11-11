Whole-node fade, 0..1. It is also a motion word — anims
and `when` responses drive it (`when disabled opacity=0.4`), and it
tweens under `transition` rules. Per-color alpha belongs to the color
itself: the `/opacity` suffix (`black/0.5`).

```
Row spacing=10 {
    Rectangle solid color=accent, size=60x28
    Rectangle faded color=accent, size=60x28, opacity=0.35
}
```
