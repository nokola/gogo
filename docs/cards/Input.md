Single-line text entry, with `placeholder_text=` for the hint and
`max_length=` for a limit. `padding=` insets the text
within the field's box (WPF TextBox Padding, verbatim). Each keystroke
fires `edit` (the live stream); Enter or focus loss commits and fires
`change(String)` — once per genuine change, programmatic `text=` writes
included.

```
Input nameInput placeholder_text="Painting name", width=fill
// a Change event carries the committed text (handler: `nameInput`);
// Edit events carry the text per keystroke, only when you need it
```
