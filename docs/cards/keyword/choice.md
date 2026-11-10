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
// OnSizeChange(radio_name) by convention, or on:change=Handler / emit(HostSignal)
// on:change lives on the GROUP — a Radio inside refuses it
```
