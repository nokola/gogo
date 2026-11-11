How many degrees of turn something covers. Two homes:

- On an `Arc`: the length of the drawn ring segment (`from=` says where it
  starts; `sweep=360` is a full ring).
- Inside a radial kind's config — `automatic=arc(from=-225, sweep=270)`
  or `angle(...)`: how far the full `$min`→`$max` travel of the value
  goes around the circle — a knob with `sweep=270` turns three quarters
  of a circle from its minimum to its maximum.

Degrees, clockwise. On an `Arc` 0° points up (the compass); inside a
radial kind's config 0° points right.

```
Arc gauge 48x48, from=-90, sweep=180, thickness=3, color=red
```
