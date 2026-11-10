# Main Menu

A centered title-and-buttons column with a staggered entrance — the classic
entry screen. Copy it, rename the buttons, and the host auto-wires
`OnNewGameButtonActivate()` and friends by name. The `appear` anim plays every
time the screen shows: each `.MenuButton` slides up with a springy settle
(`back_out` overshoots home), then gives a little pop.

```
$bg     = #1F2430
$accent = #E51400

style MenuButton min_width=260, min_height=48, font_size=18 {
    when held background=white/0.13, scale=0.97
}

anim appear duration_each=0.2 {
    each .MenuButton stagger_by=0.1 {
        translate.y from=40, to=0, easing=back_out   // settle = overshoot
        and opacity from=0,  to=1
        then scale.x to=1.05, duration=0.08, autoreverse=true   // a little pop
    }
}

View screen background=$bg {
    Column menu halign=center, valign=center, spacing=12, on:show=play(appear) {
        Label title text="MY GAME", font_size=42, font_color=$accent, halign=center
        gap 16
        Button newGameButton .MenuButton text="New Game"
        Button settingsButton .MenuButton text="Settings"
        Button quitButton .MenuButton text="Quit"
    }
}
```
