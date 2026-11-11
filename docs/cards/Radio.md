One-of-N inside a `choice { }` scope — every Radio in the scope joins one
exclusive group and renders radio visuals. The NAME is the reported value;
the picked Radio holds `selected`. `on:change` lives on the GROUP — a
Radio refuses it (the value is "which one", not on/off).

```
Column sizePick spacing=4 {
    choice size value=medium {
        Radio small  text="Small"
        Radio medium text="Medium"
        Radio large  text="Large"
    }
}
// the Change event comes from the picked Radio, names the handler `size`
// and carries that Radio's name (large)
```
