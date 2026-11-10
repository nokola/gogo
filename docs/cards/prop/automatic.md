# Automatic slots: restyle any part of a control without breaking it

A slider is made of pieces:

- the **track** — the long bar
- the **progress** — the colored part showing how far along you are
- the **thumb** — the little circle you drag

In gogo, each piece is written with the `slot` keyword, and slots are the parts styles can reach — as `slot.` paths on the style line. A slot region takes placement (`width`/`height`/`halign`/`valign`/margins) and the box family (`background`/`corner_radius`/`border-*`/`padding`):

```kdl stale
style Chunky for=Slider, slot.thumb.width=32, slot.thumb.height=32
```

Size, background, corners — a style can set all of that. But there is one thing about the thumb no style may set: where it sits on the track.

## Why the thumb's position is off-limits

The thumb's position isn't decoration — it's how the slider shows you its value. If the value is 30%, the thumb sits 30% of the way along the track.

Now imagine a style could write it. Someone ships a theme with `slot.thumb.margin_left=40` in it. Every slider wearing that theme now has its thumb glued 40 pixels from the track's left edge. The value can be anything — thumb's at 40. You drag it with your mouse, it snaps back to 40. The slider still works inside, but you can't see the value and you can't really change it. The theme author didn't mean to do that, but they just broke every slider in the app.

Same with the progress part: its width *is* the value. Let a style set it, and you have a picture of a slider instead of a slider.

So a control needs a way to say, for each part: styles can change anything about this part except this one thing — gogo will automatically update that one, from the control's value.

## `automatic=` marks the property the control drives

Here is a slider built from scratch — call it Fader. In the `control` declaration, the coupling goes on the slot line, fully spelled (gogo's built-in `Slider` declares exactly these couplings on its own track/progress/thumb):

```kdl stale
control Fader $value=0, $min=0, $max=100 {
    script "res://ui/fader_behavior.gd"
    transition duration=0.12, easing=cubic_out
    Layers {
        slot track width=fill, height=24 {
            slot progress automatic=length by $value in $min..$max {
                Rectangle color=#4488FFFF
            }
            slot thumb automatic=position by $value in $min..$max {
                Circle 12x12, color=#FFFFFFFF
            }
        }
    }
}
```

Each coupling line reads complete: the kind after `automatic=` says what the part does (grow, slide), `by $value` names which value drives it, and `in $min..$max` says where that value's ends sit — at `$min` the progress is empty and the thumb at the start, at `$max` full and at the end. All three pieces are required; nothing about the coupling is left for you to remember from a manual.

That line does two things for you.

First, gogo now updates those parts by itself. Value at zero: the progress part has no width, the thumb sits at the start. Value at max: progress covers the track, thumb at the end. Drag with your finger and they follow instantly; set the value from code and they glide over smoothly, through the control's `transition` rule. You never write that movement code — `automatic=` is the movement code.

Second, if a style sets one of those properties anyway, it doesn't build:

```text
error[G3002]: slot.thumb.margin_left moves by itself —
        Fader drives it from the value
  --> res://ui/theme.gogo:1:30
  = note: you style the thumb's look (size, background, radius…);
          the control handles where it sits
          (automatic=position by $value in $min..$max, spec §4.5).
  help: remove `slot.thumb.margin_left=`
```

The theme author from before finds out the moment they build, with a message pointing at the exact line — instead of shipping the theme and getting bug reports about sliders that won't drag.

## Build one: a progress bar

A progress bar is the simplest control with an automatic slot — a track, and a progress part that grows with the value:

```kdl stale
control Progress $value=0, $min=0, $max=100 {
    script "res://ui/progress_behavior.gd"
    transition duration=0.12, easing=cubic_out
    Layers {
        slot track width=fill, height=8, background=#33333366 {
            slot progress automatic=length by $value in $min..$max {
                Rectangle color=#4488FFFF
            }
        }
    }
}
Progress dl 200x8, value=30
```

The `script` line names the control's behavior (every control has one — see the `control` card); a progress bar takes no input, so its behavior has nothing to do. And nowhere is there a "when the value changes, set the width" function — `automatic=length` is that code. Set `value` on the node from your code-behind (`dl.set("value", 62)`) and the fill glides over through the `transition` rule.

Now style it. Everything except what the value drives is open:

```kdl stale
style Thicc for=Progress, slot.track.height=16
style Dark for=Progress, slot.track.background=#222222FF
```

Taller, or darker — and it still works, because neither style touched the driven property. Two things a style can NOT do:

Recolor the Rectangle inside the progress slot — content isn't the region. Restyling content IS filling, so bring your own node at the use site instead:

```kdl stale
Progress dl 200x8, value=30 {
    slot progress { Rectangle color=#44CC88FF }
}
```

And write the driven property:

```text
error[G3002]: slot.progress.width moves by itself —
        Progress drives it from the value
  --> res://ui/theme.gogo:1:26
  = note: you style the progress's look
          (size, background, radius…);
          the control handles where it sits
          (automatic=length by $value in $min..$max, spec §4.5).
  help: remove `slot.progress.width=`
```

Gogo caught it before it could break a single progress bar. That's the whole system: the declaration says who updates what, styles change everything else freely, and the compiler referees.

## Why it says `length`, not `width`

A kind names what the value *does* to the part, not a property. On a horizontal control, "grows with the value" means width; on a vertical one it would mean height. Because the declaration says `length`, one style serves both orientations without mentioning either — today's controls run horizontal, and the vocabulary is already right for vertical when it arrives. Same for `position`: it claims the whole "where it sits" family (alignment, margins) along the travel axis, not one property name.

## The five kinds, and the forms at a glance

```
slot fill  automatic=length by $value in $min..$max
slot thumb automatic=position by $value in $min..$max
slot mark  automatic=presence by selected
slot wedge automatic=arc(from=-225, sweep=270) by $value in $min..$max
slot dot   automatic=angle(from=-225, sweep=270) by $value in $min..$max
```

Two grammar rules cover all five: parens after the kind carry the kind's own config (only the radial kinds have any, and `from=`/`sweep=` are required there — nothing defaults invisibly), and `by` carries the driver — a `$param` with its `in` range for the four value kinds, a state name for `presence` (a bool has no range). Each kind takes over specific properties of its slot, which styles then can't touch:

- **`length`** — the part grows and shrinks with the value. That's a progress bar's fill.
  *Takes over:* its extent along the travel axis (`width`, `size`) and its placement there (`halign`, the side margins). Thickness stays yours — a fatter bar is a style choice (`height`).
- **`position`** — the part slides along with the value. That's the slider thumb.
  *Takes over:* placement along the travel axis (`halign`, the side margins). Size stays yours — a bigger thumb is a style choice.
- **`presence by selected`** — the part only exists while the named state is on. That's the checkmark in a checkbox: there when `selected`, gone when not. And "gone" still plays your enter/exit transitions, so the mark can fade or pop instead of blinking.
  *Takes over:* whether the part exists at all — showing and hiding it is the control's job.
- **`arc(from=…, sweep=…)`** — a pie-slice-shaped window opens wider as the value grows. This one's for round progress: you put a full ring inside the slot, and the window reveals more and more of it.
  *Takes over:* the window itself — the aperture config, rotation. The ring inside is fully yours (it's content — fill it).
- **`angle(from=…, sweep=…)`** — the part moves around a circle's edge with the value. That's the little dot on a volume knob.
  *Takes over:* the part's seat on the rim — the aperture config, rotation. Size, color, shape of the dot: still yours.

## Loops are animations, not automatic

There is no kind for a spinner going round or a caret blinking, on purpose. The rule that decides: **if the geometry displays data, it's `automatic=`; if it's presentational motion, it's an animation.** A spinner and a caret display nothing — no value you could read or drag — so they use the anim channel, which already loops: `anim spin repeat=forever { self.rotate …, duration=1.2 }` for the spinner, an opacity loop for the caret. An indeterminate progress bar is the same call: "still working" is a state, not an amount — a `when` shows the marquee part and a `repeat=forever` anim moves it. And when something loops *and* means something — a clock face's second hand — the meaning is a value, so it's data again: a Clock control with `$value=0, $min=0, $max=60` and `automatic=angle(from=-90, sweep=360) by $value in $min..$max`, with your code writing the seconds into `value` on a timer.

## One more: a knob, to see `arc` and `angle`

The round ones are more fun. A volume knob is a circle with a progress arc and a dot that moves around the rim:

```kdl stale
control Knob $value=0, $min=0, $max=1, min_size=48x48 {
    script "res://ui/knob_behavior.gd"
    transition duration=0.15, easing=cubic_out
    Circle color=#333333FF {
        slot track width=fill, height=fill {
            slot progress automatic=arc(from=-225, sweep=270),
                    by $value in $min..$max {
                Arc thickness=4, color=#4488FFFF
            }
            slot thumb automatic=angle(from=-225, sweep=270),
                    by $value in $min..$max {
                Circle 8x8, color=#FFFFFFFF
            }
        }
    }
}
```

(The radial lines run long, so they wrap at a trailing comma — gogo's one line-continuation mechanism; the `by` clause picks up on the next line.)

`from=` and `sweep=` are degrees; 0° points right and positive turns clockwise (Godot's screen convention). Value at min: the dot sits at `-225` — lower-left, the 7:30 mark on a clock face. The value sweeps it 270° clockwise to the 4:30 mark — the classic amp-knob range; at half, the dot points straight up. The arc's wedge opens over the same range, and it clips hits too: taps only land on the revealed part.

## Two placement rules the compiler holds you to

- A `length`/`position` slot lives in a free host — `Layers`, `Circle`, or another slot region — never directly in a `Column`/`Row`/`Grid`. Those measure their children, and a value that rewrites child geometry would re-run that measurement on every frame of a drag. Gogo refuses at build (G5105) and says where to seat it.
- A `position` slot is as wide as its content, so `width=fill` content gives it no width at all. Gogo refuses that too: size the content (`width=20`) or the slot line (`slot thumb width=20`).

## Why you can't add your own kinds

You might want a seventh kind — some custom formula from value to property. Gogo doesn't allow it:

```text
error: unknown automatic kind 'sparkle' — the set is
       closed: length, position, presence, arc, angle
```

Because the list is fixed, gogo knows exactly which properties each kind owns, so when it reads a style it can say for certain: "this line touches a property the control drives — error, here's the line." If controls could claim any property through any formula, gogo couldn't tell what's claimed and what's free. Style mistakes would stop being build errors and become weird behavior you debug at runtime — and to know what's safe to style, you'd have to read the control's internals. If a new kind ever earns its place, that's a deliberate language change, made when a real consumer shows up to pin its design.

## `automatic=` lives in control declarations only

A plain widget has no value, so there is nothing for the part to follow:

```text
error: `automatic` is automatic geometry — it needs a
       `control` (a value to drive it)
  help: widgets have no value; declare a `control` to
        couple slots to one
```

And a `control` without a `script` line gets the mirror lesson (G5101): a control's script is what turns input into its value — without one, declare a `widget` instead.

## Recap

- Controls are built from `slot`s; styles restyle the regions (placement + the box family) and use sites fill the content.
- A slot line can mark one aspect `automatic=` — gogo updates it from the driver, and a style that touches it gets error G3002 at build. That's how any style stays safe on any control.
- Every coupling is fully spelled: the kind (config in its parens), `by` the driver, `in` the range — `automatic=length by $value in $min..$max`. The line reads complete with nothing remembered from a manual.
- The kinds (`length`, `position`, `presence`, `arc`, `angle`) name what the value does, not a property — so one declaration serves both orientations. Loops that display no data are animations, not automatic.
- You get movement, dragging, and gliding for free. `automatic=` is the behavior; you just declare it.

You now know enough to build a progress bar, a slider, a checkbox mark, or a knob — and to style any of them without fear.
