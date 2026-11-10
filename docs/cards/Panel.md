A styled box + padding — blank canvas until told (no theme gray leaks).
`background=`, `corner_radius=`, `border_color=`/`border_thickness=`,
`padding=` are its vocabulary.

```
Panel card background=#27282C, corner_radius=6, padding=12 {
    Label text="content with breathing room", font_size=14
}
```

At a FILE's root, `Panel` announces a COMPONENT: the root IS the box (a
toast banner, a progress track), with no wrapper around it. Screens open
with `View` instead.
