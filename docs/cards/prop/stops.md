The Gradient's color ramp — REQUIRED (no hidden default). Entries are any
color spelling (hex, palette names, `$refs`, the `/opacity` suffix is allowed),
optionally pinned with `@position` (0..1, always after any `/`): no
positions = even spacing; if one entry is pinned, ALL must be; positions
never decrease (equal = a hard step); under `repeat=mirror` they stop at
0.5. One stop errors with a machine fix — a solid is a `Rectangle`. Stops
stay live: `stop_1_color`, `stop_1_offset`, ... are real per-stop
properties whens and anims drive — numbered from 1 in declaration order
(`stop_1` = the first stop in `stops=[...]`, not an array index).

Fading out: stops mix with their alpha multiplied in (premultiplied
Oklab), so the RGB under alpha 0 adds nothing — `white/0` and `#00000000`
both fade the neighboring color out clean. Bare `transparent` is still
refused in `stops=` (G2006): write `white/0` or `black/0`. (Porting note:
WPF's DEFAULT stop color is transparent black, and in WPF such a stop
fades dark; in gogo it fades clean.)

```
Gradient fade fill=linear, angle=90,
    stops=[white@0, blue/0.5@0.24, accent@1], height=40, width=100
```
