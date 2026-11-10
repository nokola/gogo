Which implementation draws this built-in control. Unmarked, a `Slider` IS
Godot's HSlider — untouched, exactly as always. `render=gogo` swaps in
gogo's own slider: it looks the same until you style it, but it has the
full contract behind it — slots to restyle (`for=Slider` styles, `slot`
fills), the `held` drag state, `live()` labels. The trade: code-behind
casts like `as HSlider` stop working on that instance (address it through
`value`/signals instead).

| value | meaning |
|---|---|
| `gogo` | gogo's composite draws it — slots, contract states and fills work |
| `native` | the plain Godot node — also the spelled opt-out against a `theme { Slider ... }` default |

```
Slider volume render=gogo, value=30, min=0, max=100, step=5
```

You rarely type this by hand: the error you get when a `for=Slider` style
lands on a native slider offers it as the fix. To flip every slider at
once, put it in the theme instead: `theme { Slider .ThinSlider render=gogo }`
— an instance-level `render=native` still opts out (and drops the theme's
default style with it). `Slider` is the one control with both backends.
`Toggle` is gogo-only (`render=native` on it errors — there is nothing to
opt back into), and `Button` never needs one — its children are free-form
content, and plain styles cover its held/selected looks.
