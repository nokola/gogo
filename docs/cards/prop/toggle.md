`toggle=true` makes a Button latch (engine `toggle_mode`) — activating
flips it and the `selected` condition styles the latched look (it
persists, where `held` is only while the press is held); each genuine flip
fires `change(bool)`. One-of-N latching is `choice` + `Radio`, not
toggles.

```
Button paintButton text="Paint", toggle=true, min_height=36 {
    when selected background=accent
}
```
