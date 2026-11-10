The Grid's placement-by-NAME form (CSS grid-template-areas with heights
riding each row): one quoted string per row — area names + `: height`
after the colon (REQUIRED on every row; `row_height=` belongs to the row
form); `.` marks an empty region; every area must be one contiguous
rectangle. Children bind by NODE NAME; an unbound area is a warning.
Heights are literals here ($refs need the row form). One Grid uses
cols= + row blocks OR areas=, never both.

```
Grid cols=[90, fill], height=200,
    areas=["header header : 40",
           "nav    body   : fill"] {
    Rectangle header color=charcoal
    Rectangle nav    color=teal
    Rectangle body   color=purple
}
```
