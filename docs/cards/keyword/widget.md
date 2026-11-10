A reusable component: `$params` with defaults, one body root, an optional
`slot` for instance children and a `signals` surface. Scoped `anim`/`style`
directives sit alongside the root.

```
widget LabeledSlider $label="", $min=0, $max=100, $value=0 {
    Column spacing=4 {
        Label text=$label, font_size=14
        Slider signals min=$min, max=$max, value=$value
    }
}

LabeledSlider sizeSlider label="Size", min=1, max=300, value=80
```
