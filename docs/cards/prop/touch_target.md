`touch_target=off` marks a deliberately small interactive control: small
is the design (a dense desktop toolbar). The Godot addon warned about any
control under 44 epx and this word silenced the warning; the Rust crate
accepts the word and has no such warning yet, so keeping phone controls
at least 44 epx is yours to do.

```
Button tinyButton text="×", size=20x20, touch_target=off
```
