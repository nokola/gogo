Space in a `Row`/`Column` stack — a lowercase structure word, never named.
Space you must style or animate is a layout element (use a real node).

```
Column {
    Label a text="above", font_size=14
    gap 12        // fixed epx along the stack's axis
    Label b text="below", font_size=14
    gap fill      // elastic: pushes what follows to the far end
    Button okButton text="OK"
}
```
