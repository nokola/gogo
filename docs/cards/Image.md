Bitmaps: the picture scales to fit the box (Uniform), centered. It gives
the box no minimum size unless the file asks: `width=hug` / `height=hug`
is the picture's own size, `aspect=natural` follows its shape, and a bare
Image (no size word) in a `Column` takes the Column's width and the
height the picture's shape gives there. `source=` names the picture,
scheme first:
`res://path` is an asset embedded at build time; any other scheme
(`preview://brush`, `canvas://tile`) is a HOST picture the app's code
registers at run time under that exact name — see the `source` card.

```
Image logo source="res://icon.svg", width=44, height=44
Image preview 240x80, source="preview://brush", fit=distort   // drawn by code
```

`loading=lazy` asks the host program to decode the picture off the frame,
starting at its first draw; the node shows empty until the pixels land.
gogo decodes nothing itself: the word is kept on the node for the host to
read. For image-heavy catalogs; default is `eager`.
