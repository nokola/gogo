Holds while the pointer or finger is DOWN on the control — momentary input
feedback (a quick tap is a brief flash). A look that should stick is
`selected` or an arrangement. Time it with `transition` rules; motion words
(`scale=`, `translate.x=`, `rotate=`) are legal standing responses.
Press surfaces only: the Button family, a `List` row's `ListItem` root,
`Slider` and `Toggle` (their drag IS the press — no separate word), and
composites that declare `state_builtin held`.

```
style Pressy min_width=88, min_height=36 {
    transition any -> held duration=0.05, easing=cubic_out
    transition held -> any duration=0.2,  easing=cubic_out
    when held background=white/0.13, scale=0.97
}
Button tryButton .Pressy text="Press me"
```
