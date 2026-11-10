A Flow's wrap direction: `horizontal` (default — fill a row, wrap to the
next) or `vertical` (fill a column, wrap to the next). Rows and Columns
don't take it — they ARE their direction.

```
Flow chips orientation=horizontal, spacing=6, width=200 {
    Button aButton text="red"
    Button bButton text="green"
    Button cButton text="blue"
    Button dButton text="wraps"
}
```
