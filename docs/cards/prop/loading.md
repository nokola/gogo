How an `Image`'s picture gets to the screen: `eager` (the default —
decoded before the first frame that shows it) or `lazy` — the host program
decodes it off the frame and it lands when ready, in declaration order, so
the top of the file fills first. Use it on
image-heavy pages (galleries, pickers) so the UI appears instantly and
the pictures stream in.

```
Tiles cols=2 {
    Image a source="res://icon.svg", loading=lazy
    Image b source="res://icon.svg", loading=lazy
}
```

Only an `Image` loads lazily — `loading=` on anything else is a
build error. See the cookbook: "How do I open an image-heavy page fast
(lazy images)?"
