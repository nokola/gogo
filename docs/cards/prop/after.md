Sequencing by NAME — the escape hatch for dependencies that CROSS the `and`/`then` flow: `group d after=b { }` starts d when group b's chain completes (its longest line, autoreverse's way back included), no matter what else is running. A referenced step stands alone — no connective; the reference IS its placement, and a connective plus `after=` on one step is an error (two placements). Only named `group`s can be referenced; names are anim-flat (forward references legal, wrappers reachable-into), unknown names and cycles refuse at load. To overlap the anchor's tail, add `offset=` (`after=b, offset=-0.05`).

```
anim openMenu {
    group dim { backdrop.opacity to=0.6, duration=0.15 }
    and group drop {
        menu.translate.y from=-40, to=0, duration=0.25, easing=back_out
    }
    then menu.opacity from=0, to=1, duration=0.2
    group nudge after=dim { backdrop.scale to=1.02, duration=0.1 }
}
Layers stage width=200, height=140, on:show=play(openMenu) {
    Rectangle backdrop color=black/0.5, width=fill, height=fill
    Panel menu background=panel, padding=10, halign=center, valign=top,
        margin_top=12 {
        Label menuText text="menu", font_size=15
    }
}
```
