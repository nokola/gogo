`on:<event>=go(arrangementName)` flips the view's arrangement — `go(base)`
returns home. The flip tweens with the matching `transition` rule, and the
active pose re-asserts on rotation and hot reload. From code:
`Runtime::go` (`rt.go(Arrangement::Expanded)`, the enum a build generates;
`Runtime::go_id` on a page compiled while the program runs). One button
that flips back AND forth is `toggle(name)`.

```
arrangement expanded
transition any -> expanded duration=0.2, easing=cubic_out
when expanded {
    bar.height = 90
}
Layers stage width=220, height=160 {
    Rectangle bar color=accent, width=200, height=50, halign=center,
        valign=top
    Button openButton text="Open", min_width=88, halign=center,
        valign=bottom, on:activate=go(expanded)
}
```
