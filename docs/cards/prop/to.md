Two homes. On an anim line, `to=` is the journey's destination — alone it
starts from the LIVE value when the step begins (WPF "To" animations,
perfect for interruptible fades); add `from=` to pin the start. On a
`Gradient`, `from=`/`to=` are box points (0..1) aiming a linear pattern (a
round fill accepts them and stays centered).

```
anim fadeOut { veil.opacity to=0, duration=0.4 }
Rectangle veil color=charcoal, size=120x40, on:show=play(fadeOut)
```
