Shared tracks and spans — `cols=` is the signature every row is checked
against. Rows either spell structure (`row { }` blocks) or bind by name
(`areas=`); one Grid uses one form.

```
Grid cols=[20, fill, 30] {
    row height=30 {
        Panel background=green
        Panel background=purple
        Panel background=yellow
    }
    row {
        Label text="hello"
    }
    row {
        Label text="world"
    }
}
```

```
Grid cols=[200, fill], height=fill,
    areas = ["header header : 50",
            "sidebar body : fill",
            "footer footer : 80"] {
    Panel header background=red
    Panel sidebar background=blue
    Panel body background=green
    Panel footer background=purple
}
```
