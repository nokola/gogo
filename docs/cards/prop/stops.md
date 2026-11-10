The Gradient's color ramp — REQUIRED (no hidden default). Entries are any
color spelling (hex, palette names, `$refs`, the `/opacity` suffix is allowed),
optionally pinned with `@position` (0..1, always after any `/`): no
positions = even spacing; if one entry is pinned, ALL must be; positions
never decrease (equal = a hard step); under `repeat=mirror` they stop at
0.5. One stop errors with a machine fix — a solid is a `Rectangle`. Stops
stay live: `stop_1_color`, `stop_1_offset`, ... are real per-stop
properties whens and anims drive — numbered from 1 in declaration order
(`stop_1` = the first stop in `stops=[...]`, not an array index).

Fading out: the RGB under alpha 0 steers the blend even though it never
shows on its own (interpolation is straight RGBA) — `white/0` fades white
out clean, `#00000000` fades through dark gray. Bare `transparent` is
refused in `stops=` for exactly that reason: it's #00000000 without saying
so. (Porting note: WPF's DEFAULT stop color is transparent black — a XAML
`GradientStop` with no `Color=` fades dark; port it as `#00000000` only
when that dark edging is the shipped look.)

```
Gradient fade fill=linear, angle=90,
    stops=[white@0, blue/0.5@0.24, accent@1], height=40, width=100
```
