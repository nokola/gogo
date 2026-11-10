An empty Grid track — spells interior holes and the tracks a `row_span`
covers from above, so the bookkeeping stays visible.

```
Grid cols=[60, 60] {
    row {
        Rectangle a color=red, height=24
        Rectangle tall color=blue, row_span=2
    }
    row {
        Rectangle b color=green, height=24
        skip                    // the span above owns this track
    }
}
```
