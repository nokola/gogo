An empty socket a `when` can move a declared node into —
`seat <name>` in the tree, `x.seat = <name>` in the response. Needed
because a margin can't reparent: the seat is the declared landing spot, so
the file tells the whole layout story (no code-behind RemoveChild/AddChild
half).

```
Row {
    Column mainColumn {
        Rectangle recents color=#444444FF, 36x180
        // savedStack rests here, below the recents
        Column savedStack {
            Rectangle saved color=#888888FF, 36x36
        }
    }
    seat sideSeat                    // the landscape home, beside them
}

when screen.landscapeSmall {
    savedStack.seat = sideSeat
}
```

Rules (each one a build error when broken): a seat is EMPTY (its occupant
is declared in its own resting home) and takes a name and nothing else —
it hugs to zero alone, to the occupant's minimum when occupied, fitting
the occupant to its full rect. Seats live under containers; the occupant's
resting home must be a container too (free-placed nodes re-pin with a
placement `when` block instead). One occupant per seat under conditions
that can hold together — two different arrangements are exclusive, so
their claims may share a seat; an arrangement claim and a screen claim
can hold at once, so theirs may not. One occupant's `.seat` claimed by a
bare class `when` AND a bare orientation `when` refuses outright (G3005
— both hold at once and a node sits in one seat; compose them instead).
Gotcha: in a row with `spacing=N` an empty seat still costs ONE spacing
gap — zero wide, but a child slot. Bare rows have no gap (D244), and a
seat declared last leaves any gap trailing the visible content.

A move is legal from every `when` whose change nothing can animate:
`when screen.<class>` / `when screen.<orientation>`, the composed pairs
(`when <arrangement> screen.<class or orientation>`,
`when screen.<orientation> screen.<class>`), and a plain
`when <arrangement>` while NO `transition` rule times the arrangement. A
timed arrangement carrying a seat move was a build error in the Godot
addon (not checked by the Rust compiler yet), pointing at both
lines (G3003) — a node is in one parent or the other, never partway, so
there is nothing for the animation to show. Contested seats follow the
placement rule: more conditions win (a composed pair over a single),
equal count → the later line in the file. Dropping every condition returns the node to its declared
home, parent AND position. Instant always; re-asserts on hot reload, and
a view built after the class or orientation was named starts seated.

```
arrangement previewAbove             // no transition rule -> instant

when previewAbove {
    ball.seat = aboveSeat            // the host-driven move, one line
}

Row strip {
    Column ballHome {
        Rectangle ball color=#888888FF, 40x40
    }
    seat aboveSeat
}
```

`x.seat = home` is the reserved target: sit at the declared home — parent
and position — while THIS condition holds. Needed because a more specific
condition may want the node back where a less specific one moved it from:

```
arrangement open

when screen.landscapeSmall {
    band.seat = barSlot        // rest: the band moves into the bar's slot
}
when open screen.landscapeSmall {
    band.seat = home           // open hides the bar — band back at its spot
}

Column page {
    Column homeCol {
        Rectangle band color=#444444FF, 120x30
    }
    Row bar {
        seat barSlot
        Rectangle dots color=#666666FF, 20x20
    }
}
```

`home` can't name a declared seat (a build error says so).

Not `slot`: a slot is where a widget's CALLER puts content at build time;
a seat is where a `when` puts an existing node at runtime.
