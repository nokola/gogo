## Values in Gogo

| shape | meaning |
|---|---|
| `36`, `0.5`, `-4` | bare numbers on LENGTH properties are epx; there is no other unit (`36px` is build error G0038) |
| `"Paint"` | strings; `"new stroke"/newStroke` marks a translation term |
| `true` / `false` | booleans |
| `#E51400CC`<br>`red`<br>`accent`<br>`black/0.5` | colors: hex with alpha LAST, palette names, semantic tokens; every spelling takes `/opacity` (final alpha, 0..1) |
| `$name` | a widget parameter (innermost scope) or theme token |
| `fill`, `hug`, `fill(2)` | sizing words on `width=`/`height=`/tracks |
| `center_bottom` | enum members / handler / easing / anim names (enum words ignore letter case) |
| `220x110`, `220x`, `x110` | size shorthand (both axes, epx) |
| `[80, hug, fill(2)]` | lists — only Grid `cols=`/`areas=` and Gradient `stops=` take one |
| `play(appear)`<br>`go(expanded)`<br>`toggle(expanded)`<br>`emit(Saved)` | calls on `on:` keys — `toggle` flips between the pose and base |
| `"res://assets/icons/save.png"`, `"preview://brush"` | resource paths on `source=`/`icon=`-style properties, scheme always written (`res://` = a file under the build's resource root, checked at build). Any other scheme on an `Image`'s `source=` is a host picture the app's code registers at run time under that exact name; see the `source` card |
| `safe` | the screen's safe-area inset, on padding/margin edge keys |

**What is epx? (effective pixels)**

One epx is a perceptual pixel: the renderer multiplies it by the screen's
density scale, so `min_height=36` is the same finger-size touch target and
`font_size=15` the same legible text on a phone, a monitor, or a TV across
the room. Every number on a length property is epx; device pixels never
appear in a `.gogo` file.

```
$accent = #E51400
Column spacing=8 {
    Label title text="Size", font_size=15
    Slider s min=1, max=300, value=80, width=fill, min_height=44
    Rectangle swatch color=$accent/0.8, size=120x24
}
```
