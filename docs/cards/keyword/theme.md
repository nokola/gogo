Three shapes, one keyword. Top level: `theme { }` holds document defaults,
`theme dark { }` a variant that wins while active. Inside a node's body:
`theme font_size=15` is the PROPAGATING subtree default (unlike node-local
`font_size=`).

```
$accent = #E51400
theme dark {
    $accent = #FF6B4A     // wins while set_theme_variant("dark") is active
    red = #FF5C49         // built-in color keywords remap too
}

View root {
    theme font_size=15    // every descendant Label without its own size
    Column spacing=6 {
        Label themed text="reads the subtree default"
        Rectangle stripe color=$accent, size=140x10
    }
}
```
