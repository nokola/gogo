Marks the node inside a widget body that plays the widget's wiring
surface: an `on:` entry written on an instance lands on IT instead of the
root.

```
widget LabeledSlider $label="" {
    Column spacing=4 {
        Label text=$label, font_size=14
        Slider signals min=0, max=100
    }
}

LabeledSlider sizeSlider label="Size"   // the Slider's Change names `sizeSlider`
```
