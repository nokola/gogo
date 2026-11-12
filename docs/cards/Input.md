Single-line text entry, with `placeholder_text=` for the hint and
`max_length=` for a limit (code may write it: typing, a paste and the phone keyboard stop at the last number written). `padding=` insets the text
within the field's box (WPF TextBox Padding, verbatim). Each keystroke
that changes the text fires `edit` (the live stream; a letter typed into
a full field changes nothing and fires nothing); Enter or focus loss commits and fires
`change(String)` — once per genuine change. A text written from code
(`Runtime::set_text`) shows and fires nothing at the write. The field's
plate takes the box words of a Panel: `background=`, a border,
`border_dash=`, `corner_radius=` and the four corner radii.

```
Input nameInput placeholder_text="Painting name", width=fill
// a Change event carries the committed text (handler: `nameInput`);
// Edit events carry the text per keystroke, only when you need it
```
