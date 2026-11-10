The `repeat` rename clause — declares the item binding, sigil and all
(like every `$` introduction). Omitted, the binding is `$item` (defined
sugar). Keyword-anchored and comma-free; `as` comes before `by`.

```
Column {
    Label text="Brushes"
    repeat brushes as $brush by $brush.id {
        empty { Label text="no brushes yet" }
        Label text=$brush.name
    }
}
```

Rename when `$item` would shadow a theme value or widget param in scope
(that collision is a build error pointing here), or for readability in
bigger templates.
