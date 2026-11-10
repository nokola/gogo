Single style inheritance: the parent's props and when/transition blocks
apply first, the child's override per key (the same last-writer rule as
listing two styles on a node). Chains work across `uses` imports; cycles
and unknown parents are build errors.

```
style Base min_height=36, corner_radius=6
style Warn extends=Base, background=danger {
    when held background=danger/0.7
}
Button deleteButton .Warn text="Delete"
```
