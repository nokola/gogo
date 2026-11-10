Single-line text entry. Engine properties complete in snake_case
(`placeholder_text=`, `max_length=`, ...). `padding=` insets the text
within the field's box (WPF TextBox Padding, verbatim). Each keystroke
fires `edit` (the live stream); Enter or focus loss commits and fires
`change(String)` — once per genuine change, programmatic `text=` writes
included.

```
Input nameInput placeholder_text="Painting name", width=fill
// OnNameInputChange(text) auto-wires — the committed text;
// OnNameInputEdit(text) fires per keystroke, only when you need it
```
