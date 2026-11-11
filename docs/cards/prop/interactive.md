THE input switch. Interactive controls (Button, Check, Toggle, Radio,
ListItem, Slider, Input, Scroll, List)
answer input by default, everything else is transparent — `interactive=`
overrides that for THIS control only, like any property. `true` claims
input (a drag surface, a blocking plate, a catcher); `false` makes an
interactive control hit-invisible — unlike `disabled`, nothing restyles.
Legal in `when` blocks (applies instantly, restores on release).

```
Panel card interactive=true, background=#1F1F1FFF {
    Label text="taps stop here"
}
Rectangle shield interactive=true, color=#00000000   // the catcher idiom
Button ghost text="soon", interactive=false          // hit-invisible, undimmed
```
