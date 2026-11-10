How an `Image`'s `texture=` gets to the screen: `eager` (the default —
decoded during the build, ready on first frame) or `lazy` — the texture
decodes on a background thread and lands when ready. Requests fire in
declaration order, so the top of the file fills first. Use it on
image-heavy pages (galleries, pickers) so the UI appears instantly and
the pictures stream in.

```
Tiles cols=2 {
    Image a texture="res://icon.svg", loading=lazy
    Image b texture="res://icon.svg", loading=lazy
}
```

Only `texture=` loads lazily — `loading=` on anything but an `Image` is a
build error. See the cookbook: "How do I open an image-heavy page fast
(lazy images)?"
