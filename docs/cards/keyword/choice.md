The one-of-N scope: erased at build (children parent flat), every `Radio`
inside joins one exclusive group — nesting layout works. `value=` picks
the initial Radio by name; the group reports the chosen NAME.

```
Column pickHost spacing=4 {
    choice size value=medium {
        Row {
            Column { Radio small text="Small"; Radio medium text="Medium" }
            Column { Radio large text="Large" }
        }
    }
}
// a Change event carries the picked Radio's name; the handler is `size`,
// or on:change=Handler / emit(Handler)
// on:change lives on the GROUP — a Radio inside refuses it
```
