Bitmaps: the picture scales to fit the box (Uniform), centered, never
dictating a minimum size. `source=` names the picture, scheme first:
`res://path` is an asset embedded at build time; any other scheme
(`preview://brush`, `canvas://tile`) is a HOST picture the app's code
registers at run time under that exact name — see the `source` card.

```
Image logo source="res://icon.svg", width=44, height=44
Image preview 240x80, source="preview://brush", fit=distort   // drawn by code
```

`loading=lazy` decodes the texture on a background thread — the build never
blocks on IO; the node shows empty until the pixels land. For image-heavy
catalogs; default is `eager`.
