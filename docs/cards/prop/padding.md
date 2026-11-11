Air between a node's box and its CONTENT — children or text. Inside the
border, part of the box family: content sits `border_thickness +
padding` from the edge (WPF/CSS stacking), and padding counts into a
hug/derived minimum. Per-side variants mirror margins:
`padding_left/-top/-right/-bottom`; `padding=safe` reads the safe-area
insets (scroll content that must glide under a cutout pads the CONTENT,
not the viewport). Measured air around button content also goes here —
`Panel padding=6 { Row { ... } }` is the WPF Padding analog — and text
leaves inset their own text: `Label`/`Input` pad like buttons (WPF's
TextBlock/TextBox Padding, verbatim). On free-placement regions —
`Layers`, the `View` root, slot lines — the same word insets every
declared child's placement instead. Containers
(`Column`/`Row`/`Grid`/...) take it too, as air between their edges and
their children; space between children is
`spacing=`/`gap`. Drawing leaves (`Rectangle`,
`Circle`, `Image`) have nothing inside — outer air is `margin`.

```
Panel plate background=panel, corner_radius=6, padding=12 {
    Label plateText text="padded content", font_size=15
}
Label chip text="padded text", font_size=15, padding=8
```
