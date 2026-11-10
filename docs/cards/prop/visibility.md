Three-state visibility: `visible` shows; `gone` hides and RELEASES the
space (siblings and hug parents close over it); `space_only` hides but
the node keeps holding its layout slot — toggling `visible <->
space_only` never moves a neighbor (WPF `Visibility="Hidden"`). Hidden
is hidden either way: hit-inert, unfocusable, skipped by screen
readers. Initial `visibility=gone` is state, never identity — `on:show`
/ `on:hide` track the pixels, so entering `visible` from either hidden
mode plays the show.

`space_only` holds space under FREE parents (Layers / plain Control /
slot / control roots), where gogo owns the measure; an engine container
skips hidden children in its own sorting and refuses the spelling (v1).
Code-behind flips through `gogo.set_visibility(node, "space_only")`;
plain `.Visible` writes keep meaning visible/gone.

```
Layers strip width=hug, height=hug, halign=left, valign=top {
    Rectangle a color=#4488CCFF, width=120, height=24
    // hidden, but the strip still hugs this 200x48 seat
    Rectangle seat color=#CC8844FF, width=200, height=48, visibility=space_only
}
```
