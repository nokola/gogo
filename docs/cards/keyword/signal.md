Inside a `control` block: declares the signals the control's behavior
raises — `drag_started` before a gesture's first write, `drag_ended`
after the release's commit and on a cancelled gesture too — and
`on:<name>=` wiring at use sites is checked against this list, so signal
typos are caught like state typos.

```kdl partial
control Fader $value=0, $min=0, $max=100 {
    behavior slider by $value in $min..$max
    signal drag_started, drag_ended
    ...
}

Fader gain on:drag_ended=OnGainReleased   // checked against the declaration
```

Declared signals arrive on the control's root as `Signal` events, so
name-based auto-wiring (`gain` + `drag_started` → `OnGainDragStarted`)
works with zero extra machinery; a contract without the `signal` line
raises none. Value events are never declared here — gogo synthesizes
`edit`/`change` for the control's value (D181); `signal` is for the
behavior's OWN discrete moments, raw-register spelled.

(The widget-body `signals` FLAG — which node exposes signals — is a
different word and unchanged.)
