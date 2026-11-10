The motion curve — WPF-compatible easing math. Set it on a `group` (shared by its chain), per property
line, per `key` (eases INTO that key), or on a `transition` rule.

| base | feel |
|---|---|
| `linear` | constant speed |
| `sine` | soft, gentle ease |
| `quad`, `cubic`, `quart`, `quint` | polynomial family, gentler to sharper |
| `circle` | quarter-circle arc — late, hard turn |
| `expo` | sharpest of the family (`exponent=` tunes it) |
| `power` | the polynomials with a fractional dial (`power=2.5`) |
| `back` | pulls past the target, then settles (`amplitude=`) |
| `bounce` | hits the target and bounces (`bounces=`, `bounciness=`) |
| `elastic` | springs around the target (`oscillations=`, `springiness=`) |

Every base takes a mode suffix: `_out` decelerates (UI entrances), `_in`
accelerates (exits), `_in_out` both ends. A bare name (`cubic`) means
`_out` — the friendly default. Tuning params are written as sibling properties:
`easing=bounce, bounces=2, bounciness=3`.

```
anim drop {
    badge.translate.y from=-30, to=0, duration=0.3, easing=back_out
}
Circle badge width=24, height=24, color=danger, on:show=play(drop)
```
