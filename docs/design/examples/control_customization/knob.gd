# knob.gd — behavior for `control Knob` (see knob.gogo and
# gogo-control-customization-spec.md §5.5)
#
# This file is the REFERENCE for the behavior interface. Everything a behavior
# may do appears here; everything it may not do is absent by construction:
# there is no node access on the behavior interface — geometry arrives as
# plain values (Rect2, Vector2) and leaves only through set_value(). The
# runtime maps the resulting fraction onto the declared automatic slots
# (progress wedge, thumb angle). This script never knows what the visuals
# look like. That is the whole point: any style checked against Knob's
# contract is safe under this code, and this code is safe under any style.

extends GogoControlScript

# NOTE deliberately absent: no SWEEP/START angle constants. The aperture is
# declared ONCE, on the slot lines in knob.gogo (from=-225, sweep=270), and
# read here via slot_aperture() — input mapping and visuals share the
# declaration as their single source of truth (spec §4.6).

# --- drag state --------------------------------------------------------------
var _drag_from_value := 0.0       # value at drag start (restored on cancel)


# =============================================================================
# Input. The runtime routes events pre-hit-tested: `slot` names where the
# gesture began ("thumb", "track", ...). Positions are in the control's local
# space; slot geometry is queried as VALUES via slot_rect()/slot_aperture().
# A disabled control never reaches these functions — the runtime blocks input
# upstream. Emitted signals must be declared on Knob's `signal` line
# (undeclared emit = first-run error, spec §5.5).
# =============================================================================

# Return CLAIM to take the gesture, PASS to let it bubble (e.g. to a scrolling
# ancestor). Arbitration beyond this (movement-threshold reclaim by an
# ancestor) is runtime policy, not behavior code — see spec §9.3.
func input_began(slot: StringName, p: Vector2) -> int:
	# A knob turns from anywhere on it — track or thumb both start a drag.
	# A behavior that only wants its handle would test `slot == &"thumb"`.
	_drag_from_value = value()
	emit(&"drag_started")
	_apply_pointer(p)
	return CLAIM

func input_moved(p: Vector2) -> void:
	_apply_pointer(p)

func input_ended(_p: Vector2) -> void:
	emit(&"drag_ended")

func input_canceled() -> void:
	# Gesture reclaimed by an ancestor or interrupted by the system:
	# restore the pre-drag value. Never leave a half-turn behind.
	set_value(_drag_from_value)
	emit(&"drag_ended")


# =============================================================================
# Keyboard. Only called while focused (Knob declares `state_builtin focuseded`,
# which opts it into the focus system — without that declaration this hook
# would be dead and lint D18 would say so). Actions are Gogo's logical set,
# so platform key mapping stays out of behavior code.
# =============================================================================

func key_action(action: StringName) -> bool:
	var span := prop_max() - prop_min()
	var step := prop_step() if has_step() else span / 100.0
	match action:
		&"increment":       set_value(_snap(value() + step))
		&"decrement":       set_value(_snap(value() - step))
		&"increment_large": set_value(_snap(value() + span / 10.0))
		&"decrement_large": set_value(_snap(value() - span / 10.0))
		&"to_min":          set_value(prop_min())
		&"to_max":          set_value(prop_max())
		_:                  return false   # not ours; runtime keeps routing
	return true


# =============================================================================
# Accessibility. The runtime reports value/min/max from props automatically;
# the behavior supplies only what it alone knows: the role, and optionally a
# human phrasing of the value.
# =============================================================================

func accessibility_role() -> StringName:
	return &"slider"    # a knob IS a slider to a screen reader


# =============================================================================
# Internals — plain functions over plain data.
# =============================================================================

func _apply_pointer(p: Vector2) -> void:
	# Absolute-angle model: the value follows the pointer's angle around the
	# track center. (The other common model — relative vertical drag, pro-audio
	# style — would use p.y deltas from _drag_from_value instead. Pick one per
	# control; shipping both behind a prop is a decision, not a default.)
	var track := slot_rect(&"track")           # Rect2, control-local. A value,
	var center := track.get_center()           # not a node.
	var v := p - center
	if v.length_squared() < 1.0:
		return                                  # dead center: no defined angle

	var ap := slot_aperture(&"thumb")          # Vector2(from_deg, sweep_deg) — the
	var deg := fposmod(rad_to_deg(v.angle()) - ap.x, 360.0)   # thumb slot line's declared
	                                                          # aperture (radial slots only)
	# Pointer in the gap outside the sweep: clamp to the nearer end instead of
	# jumping across the discontinuity mid-drag.
	if deg > ap.y:
		deg = ap.y if deg - ap.y < (360.0 - deg) else 0.0

	var f := deg / ap.y
	set_value(_snap(prop_min() + f * (prop_max() - prop_min())))

func _snap(v: float) -> float:
	# Step lives in VALUE space and snapping is the behavior's job (spec §5.5;
	# step=0 = continuous): the runtime derives fraction from the snapped
	# value, so the thumb and wedge land exactly on detents — styles can't
	# ever show an off-step position.
	if has_step():
		v = prop_min() + roundf((v - prop_min()) / prop_step()) * prop_step()
	return clampf(v, prop_min(), prop_max())
