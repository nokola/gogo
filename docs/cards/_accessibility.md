# Accessibility

What gogo gives every app out of the box, and the three things an author
adds. The bar throughout: a keyboard/gamepad player and a screen-reader
user get what a mouse user gets, without per-control ceremony.

## Out of the box

- **Visible focus** — every button shows its theme's focus ring for
  key/gamepad-driven focus, and ONLY then: a tap parks engine focus
  silently, so no ring is left standing after every touch. `when focused`
  replaces the ring with your own look; `when focus_within` lights the
  container while a descendant holds focus (the settings-row pattern).
  Text fields indicate on any focus — the caret is coming.
- **Composites read as one element** — a `render=gogo` Slider or a
  `Toggle` reports its role, live value/range, on/off truth and disabled
  flag to screen readers (AccessKit), exposes increment/decrement/click
  actions that run through the same logic keyboard uses, and hides its
  internals — a reader hears "slider, 40", not "panel, thumb, panel".
  A filled `label` slot names the control automatically.
- **Lists read as lists** — a `List` reports itself as a list box (a
  plain list under `select=off`) with a live item count; every `ListItem`
  row reports its index and selection, and a reader's click lands the
  SAME way a tap does — selection side effect included. The
  cursor is real engine focus, so it announces itself.
- **Native nodes report themselves** — `Button`, `Input`, `Check` and
  friends use Godot's own AccessKit integration; gogo never breaks it
  (state styling recolors plates, the a11y tree is untouched).
- **Radios read as radios** — a `choice`'s `Radio`s announce RADIO BUTTON
  with their group relation (the class stays plain CheckBox for casts and
  themes).
- **Reduced motion is one call** — `gogo.set_reduce_motion("on")` makes
  every transition and anim complete in one frame instead of playing:
  final values land, `finished` fires, nothing is skipped — only the
  journey. Wire it to an app setting; OS-preference auto-detection exists
  but is currently off by default (the setting should own the choice).
- **Names localize** — `accessibility_name="Undo"/undoKey` follows the same
  `/key` translation every string prop does; localized apps announce
  localized names.
- **Two lints guard the bar** — touch targets under 44 epx warn
  (`touch_target=off` accepts a deliberate exception), and a
  `focus_mode=none` button warns that keyboard/gamepad users can't reach
  it (`keyboard_target=off` accepts a deliberate touch-only surface — the
  WP app-bar pattern).

## What you add

**1. A name where there is no text.** An icon-only control is a mystery
to a reader — name it (engine property, snake_case):

```
Row spacing=8 {
    Button undoButton 44x44, text="↶", accessibility_name="Undo"
    Button redoButton 44x44, text="↷", accessibility_name="Redo"
}
```

A control whose visual label is a SEPARATE node needs the same — the
caption names it for eyes, `accessibility_name` names it for readers.

**2. Keep an indicator when you restyle focus.** Replacing the default
ring is fine — replacing it with nothing strands keyboard players. Make
the `when focused` look clearly distinct from resting (a shade plus motion
reads better than a faint tint):

```
style MenuEntry min_height=44 {
    transition duration=0.1, easing=cubic_out
    when focused background=accent/0.3, scale=1.05
    when disabled opacity=0.4
}
Button startButton .MenuEntry text="Start"
```

**3. A role for your own controls.** A custom `control`'s behavior
declares what it IS — one line, and the reporting above comes for free:

```gdscript
func accessibility_role() -> StringName:
    return &"slider"   # slider, switch, button, checkbox, radio, progress, spin
```

**4. Announce transient UI.** A toast or banner that appears on its own
is invisible to a reader unless it's a LIVE region — one property, and
its text is spoken when it shows or changes (`polite` waits for a pause;
`assertive` interrupts, for errors only):

```kdl stale
Panel savedToast background=#000000CC, corner_radius=6, padding=12, accessibility_live=polite, visibility=gone {
    Label savedText text="Sketch saved", font_size=15
}
```

## Worked example — a game settings menu

Rows highlight for the D-pad (`focus_within`), sliders and the toggle
carry reader names, everything meets the touch_target bar:

```
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
    Panel muteRow .SettingsRow {
        Row spacing=12 {
            Label text="Mute effects", width=fill
            Toggle muteToggle accessibility_name="Mute effects"
        }
    }
}
```

A reader walking this menu hears "Music volume, slider, 40" and "Mute
effects, switch, off" — and adjusts both without touching the screen.

See also: the `focused` and `focus_within` cards (the visible-focus rules
in full), `touch_target` (the size lint), `control` (custom controls and
their behavior hooks).
