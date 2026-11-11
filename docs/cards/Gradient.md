Generated gradient fills. `stops=` is required (one color is a
`Rectangle`); `fill=linear|radial|conic`; `@position` pins a stop,
`/opacity` is allowed; `angle=` aims linear fills (CSS compass: 0 = to
top, 90 = to right). Size the box — a Gradient has no natural size.

```
Gradient fade fill=linear, angle=90, stops=[white@0, blue/0.5@0.24, accent@1],
    height=40, width=100
```

```
Gradient flash fill=radial, stops=[#FFFFFF6B@0, #FFFFFF6B@0.94, #FFFFFF6B/0@1],
    width=100, height=40
```
