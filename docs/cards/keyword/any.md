The `transition` wildcard — one side of a rule: `any -> held` times
every entry into held, `held -> any` every exit, and a bare
`transition duration=…` is `any -> any`, the default for every change.
Specificity: exact pair, then entering (`any -> X`), then leaving
(`X -> any`), then bare. `any` is never a pose — poses are declared with
`arrangement`.

```
style Springy min_width=88, min_height=36 {
    transition any -> held duration=0.05, easing=cubic_out
    transition held -> any duration=0.3,  easing=back_out
    when held scale=0.94
}
Button pushButton .Springy text="Push"
```
