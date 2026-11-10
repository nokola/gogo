A `List` row's root — the selectable, focusable, tappable unit, spelled by
you (WPF generates its ListBoxItem behind your back; gogo never inserts
wrappers). Required as the row template's root, legal nowhere else.

```
style RowLook padding=8 {
    when selected background=accent
    when held opacity=0.7
}
List picker {
    empty { Label text="nothing yet" }
    repeat tasks by $item.id {
        ListItem .RowLook {
            Row spacing=8 {
                Label text=$item.title
                Button text="ok"
            }
        }
    }
}
```

- **The row's states live here.** The List flips `selected` on it
  (selection) and `held` while a press holds it (WP press feedback — legal
  on rows like on buttons); focus is the keyboard cursor
  (`when focused` styles it).
- **Default looks, out of the box** (D122): an unstyled row still SHOWS
  selection, the cursor and the press — `selected` fills the row with a
  selection color, `focused` with a quarter-strength echo, `held` with a
  half-strength fill while the press is held (an unselected row shades up, a
  selected one dims — `held` outranks `selected`). Same doctrine as
  a Button's default plate: the default is the base look, `when` overrides it —
  your `when selected` (or `when focused`/`when held`) replaces that
  state's default entirely, per state. `hovered` stays opt-in. Under
  `select=off` the selected default is skipped (no selection to show);
  menus still flash on press.
- **The press shade mirrors "release here would tap"**: it clears the
  moment the pointer slides off the row (or a pan takes over) and returns
  if it slides back — a release anywhere off the row leaves no residue.
- **The cursor echo obeys focus-visible**: it draws only while the
  KEYBOARD last moved the cursor. A tap moves focus to the row
  silently — no shade survives a press-and-wander. (Your authored
  `when focused` is never gated — style it and it answers every focus.)
- **A drawn box like `Panel`**: `background=`, `padding=`, `corner_radius=`,
  `border-*=` — blank until told. One child fills it; multi-part rows lay
  out with a `Row`/`Column` inside.
- **One tap surface.** The row claims taps wherever the finger lands;
  interactive children (the `Button` above) take their own presses first —
  tapping the button fires the button, not the row.
- Names inside are per-item local; handlers wired in the row get the item's
  key as the last argument.
