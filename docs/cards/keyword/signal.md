Inside a `control` block: declares the signals the control's behavior
raises — `drag_started` before a gesture's first write, `drag_ended`
after the release's commit and on a cancelled gesture too.

```
control Fader $value=0, $min=0, $max=100 {
    behavior slider by $value in $min..$max
    state_builtin held
    signal drag_started, drag_ended
    Layers {
        slot track width=fill, height=24 {
            slot thumb automatic=position by $value in $min..$max { Circle 12x12 }
        }
    }
}

Fader gain value=25
```

A declared signal reaches code as a `Signal` event on the control's root
(`gain`), and the handler that event names is the signal's own name
(`drag_started`). A contract without the `signal` line raises none.
Signals are not wired with `on:` — `on:drag_ended=` on an instance is
build error G0049. Value events are never declared here — gogo
synthesizes `edit`/`change` for the control's value (D181); `signal` is
for the behavior's OWN discrete moments.

(The widget-body `signals` FLAG — which node exposes signals — is a
different word and unchanged.)
