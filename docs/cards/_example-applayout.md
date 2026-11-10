# App layout

Header, sidebar, main and footer on one Grid — `areas=` names the
regions, children bind to them by NAME (spans spelled in the map, no
coordinates anywhere).

```
Grid app cols=[220, fill], width=fill, height=fill, spacing=2,
        areas=["header header: 56",
               "sidebar main: fill",
               "footer footer: 40"] {
    Rectangle header color=#2E4053
    Rectangle sidebar color=#3A5268
    Panel main background=#1F2430, padding=12 {
        Label body text="main content", font_size=15
    }
    Rectangle footer color=#2E4053
}
```
