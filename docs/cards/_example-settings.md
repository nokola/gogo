# Settings form

Labeled sliders and checks in a scrolling column. The `theme` line gives
every Label a density-correct default size in one place.

```
theme { font_size = 15 }

Scroll page width=fill, height=fill {
    Column form spacing=10, width=fill {
        Label heading text="Settings", font_size=22
        Label l1 text="Brush size"
        Slider sizeSlider min=1, max=300, value=80, width=fill, min_height=44
        Label l2 text="Smoothing"
        Slider smoothSlider min=0, max=1, value=0.35, width=fill, min_height=44
        Check hapticsCheck text="Haptic feedback", selected=true, min_height=44
        Check autosaveCheck text="Autosave", min_height=44
    }
}
```
