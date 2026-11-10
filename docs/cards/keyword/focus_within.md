True while the focus is on this node or anywhere inside it (CSS
`:focus_within`, WPF `IsKeyboardFocusWithin`). The game-menu staple: the
thing that HOLDS focus is a leaf (a slider, a button), but the thing that
should LIGHT UP is the container around it — the settings row, the active
pane, the shop card. Same visible-focus flavor as `focused`: key-driven
focus lights it, a tap into a descendant parks silently.

```
// controller settings menu: D-pad up/down moves between rows, left/right
// drives the slider INSIDE the row — the whole row highlights. The
// accessibility_name gives a screen reader what the visual label gives
// your eyes (see the Accessibility card).
style SettingsRow padding=8, corner_radius=4 {
    transition duration=0.08
    when focus_within background=#FFFFFF1E
}
Column spacing=2 {
    Panel musicRow .SettingsRow {
        Row spacing=12 {
            Label text="Music volume", width=fill
            Slider music value=40, min=0, max=100, width=160, valign=center, accessibility_name="Music volume"
        }
    }
    Panel sfxRow .SettingsRow {
        Row spacing=12 {
            Label text="Effects volume", width=fill
            Slider sfx value=70, min=0, max=100, width=160, valign=center, accessibility_name="Effects volume"
        }
    }
}
```

Also the two-pane pattern: inventory + chest side by side, the pane whose
descendant holds focus brightens so the player knows where the D-pad
acts (`when` recolors — a border change is a second Panel state away):

```
style ActivePane border_thickness=1, border_color=#FFFFFF30, background=#00000000 {
    when focus_within background=#FFFFFF14
}
```

Priority: `focus_within` ranks below every other state — it is ambience,
input feedback layers on top.
