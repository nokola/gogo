Exact-size shorthand for both axes (epx): `size=220x110`, or bare on the
node line (`Button b 44x36`). Omit a component to leave that axis alone
(`size=x72` sets height only). Exact means exact — containers don't
stretch it; the stretchy spelling is `min_size=`.

```
Column spacing=8 {
    Rectangle chip color=teal, size=120x28
    Rectangle tall color=purple, size=60x44
}
```
