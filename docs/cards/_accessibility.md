# Accessibility

What gogo does today for a keyboard user, what it does not do yet, and the
two things an author adds.

## What works today

- **Visible focus** — every button shows a focus ring for key-driven
  focus, and ONLY then: a tap moves focus without showing it, so no ring is
  left standing after every touch. `when focused` replaces the ring with
  your own look; `when focus_within` lights the container while a
  descendant holds focus (the settings-row pattern). Text fields indicate
  on any focus — the caret is coming.
- **The keyboard reaches everything a tap reaches** — Tab and Shift-Tab
  walk the page in file order, the arrows move to the nearest control in
  their direction, Enter/Space activate. A focused Slider steps with the
  arrows and Home/End; a focused Toggle takes them as its two sides.
  `keyboard_target=off` takes a control out of that walk (a touch-only
  app bar).
- **Lists have a keyboard cursor** — the arrows walk the rows, Enter
  selects and activates, and a List scrolls just enough to keep the
  focused row in view.

## What is not there yet

- **No screen-reader bridge.** gogo hands nothing to the platform's
  accessibility tree: a reader does not hear roles, values or names. The
  page holds everything such a bridge needs (kinds, names, rects, text),
  and `accessibility_name=` is kept on the node for the host program to
  read.
- **No reduce-motion switch** and no speed setting for animations.
- **No lint for small touch targets.** `touch_target=off` is accepted and
  nothing warns.

## What you add

**1. A name where there is no text.** An icon-only control is a mystery
to a reader — name it now, so the name is there when a bridge is:

```
Row spacing=8 {
    Button undoButton 44x44, text="↶", accessibility_name="Undo"
    Button redoButton 44x44, text="↷", accessibility_name="Redo"
}
```

A control whose visual label is a SEPARATE node needs the same — the
caption names it for eyes, `accessibility_name` names it for readers.
The name takes a translation key like any displayed string
(`accessibility_name="Undo"/undoKey`).

**2. Keep an indicator when you restyle focus.** Replacing the default
ring is fine — replacing it with nothing strands keyboard users. Make
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

## Worked example — a game settings menu

Rows highlight while a control inside them has key focus (`focus_within`),
and the sliders and the toggle carry names for a reader:

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

See also: the `focused` and `focus_within` cards (the visible-focus rules
in full), `keyboard_target` and `touch_target`.
