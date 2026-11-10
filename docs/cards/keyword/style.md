A named property bag worn with `.Name` (local props win). Holds `when`
condition responses and `transition` timing; `extends=` chains.

```
style ToolButton min_height=36 {
    transition any -> held duration=0.05, easing=cubic_out
    transition held -> any duration=0.2,  easing=cubic_out
    when held background=white/0.13
}

Button paintButton .ToolButton text="Paint"
```
