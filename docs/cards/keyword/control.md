A widget plus an INPUT BEHAVIOR: a `behavior` line names the behavior that
turns the finger and keys into `$value`, beside declared states and
signals, and slots with automatic geometry the value drives. Styled,
filled and wired exactly like a built-in.

```
control Fader $value=0, $min=0, $max=100, $step=0 {
    behavior slider by $value in $min..$max, step=$step
    state_builtin hovered, focused, disabled, held   // held: the drag IS a press
    signal drag_started, drag_ended
    transition duration=0.12, easing=cubic_out   // default feel

    Layers {
        slot track width=fill, height=24 {
            slot progress automatic=length by $value in $min..$max { Rectangle color=$accent }
            slot thumb automatic=position by $value in $min..$max { Circle 12x12 }
        }
        slot label { Label text=live($value) }
    }
}

Fader gain value=25   // drag_started reaches code as a Signal event
```

Use `control` when input must become a value/state machine; pure
composition stays a `widget`. A control without a `behavior` line takes
its input in code (`on:pointer=` + `keys`).
