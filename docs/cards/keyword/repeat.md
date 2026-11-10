N children from data — a lowercase structure word, erased at build: the
items parent flat into the repeat's parent (a `Column`'s spacing spaces
them). The collection arrives from code by the repeat's NAME; each
assignment reconciles by key — kept keys keep their nodes, changed data
rebuilds, absent keys free, order follows the collection.

Default binding — the item is `$item`, `by` keys it, and the REQUIRED
`empty` block is the zero-items screen (a literal `empty { }` is legal:
"considered, show nothing"):

```
Column spacing=8 {
    Label text="Today"
    repeat tasks by $item.id {
        empty { Label text="Nothing due today" }
        Label text=$item.title
    }
}
```

`as` renames the binding — for readability, or when `$item` would shadow a
theme value or widget param in scope:

```
Column spacing=4 {
    Label text="Brushes"
    repeat brushes as $brush by $brush.id {
        empty { Label text="no brushes yet" }
        Button text=$brush.name, on:activate=OnBrushActivate
    }
}
```

**To wire it up**: save the tree above as `res://ui/brush_list.gogo`, then
create `res://ui/brush_list.gd` — the code-behind IS the view root — and
drop the node into any scene:

```gdscript
class_name BrushList
extends GogoView            # `Panel`-rooted component files: GogoPanelView

func _init() -> void:
    gogo_source = "res://ui/brush_list.gogo"

func _on_view_loaded() -> void:      # after every (re)build
    set_items("brushes", [
        {"id": "b1", "name": "Round"},
        {"id": "b2", "name": "Flat"},
    ])
    # Re-assign any time — assignment IS the change event:
    set_items("brushes", [
        {"id": "b2", "name": "Flat"},   # kept: same data, same node
        {"id": "b3", "name": "Fan"},    # new: built
    ])                                  # b1 absent: freed

func OnBrushActivate(key: String) -> void:
    print("row ", key)                  # handlers in the body get the
                                        # item's key as the LAST argument
```

(Outside a `GogoView`/`GogoPanelView` host, the same call is
`Gogo.set_items(view_root, "brushes", items)`.)

`by` is required — String or int keys, no index identity; a duplicate key
refuses the whole assignment and the last good items stay live. Names
inside the body are per-item local.

The static preview shows the `empty` state; to see items while designing,
use the **Test data** dropdown above the preview — every repeat gets
`name: 0 (off) / 3 / 10` rows (synthesized placeholders, editor-only).
Real data previews live via hot reload: run the scene and edit — the last
`set_items` re-applies on every rebuild.

When the items need SELECTION (selected rows, a keyboard cursor,
activation) or SCROLLING, put the repeat inside a `List` instead of a
`Column` — there the row template's root must be a `ListItem`.
