Marks the node inside a widget body that plays the widget's wiring
surface: instance-site `on:` entries and the auto-wiring convention
connect to IT instead of the root.

```
widget LabeledSlider $label="" {
    Column spacing=4 {
        Label text=$label, font_size=14
        Slider signals min=0, max=100
    }
}

LabeledSlider sizeSlider label="Size"   // OnSizeSliderChange(v) hits the Slider
```
