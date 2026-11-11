An Image's picture, named by a path string that always carries a scheme:

- `res://path` — an asset under the build's resource root, checked and
  embedded at build time. A bare `icon.png` is refused (G0071; the check
  knows `.png`, `.jpg` and `.jpeg` names): a bare
  word could be a label or a path, the scheme says which.
- `<host>://name` — any other scheme is a HOST picture: pixels the app's
  code hands the renderer at run time under that exact name (a live
  brush preview, the canvas tiles, a crop photo). The name is baked as
  the picture's id at build time, so the box shows whatever the code
  registers under it, and nothing until it does. `gogo-ui` hands the
  renderer the picture's id in the draw list; showing the pixels is the
  renderer's job.

An empty string means no picture (no load attempt — safe widget
`$icon=""` defaults). The box sizes with the normal vocabulary; the
picture fits `contain`, centered, unless `fit=` says otherwise.

```
Image logo source="res://icon.svg", width=48, height=48
Image preview 240x80, source="preview://brush", fit=distort   // the code draws it
```

The `source=` IS the contract: an `Image` without one is a picture of
nothing and never draws, whatever the code registers.
