// UI quads: one instanced draw per (clip, texture) batch for the whole gogo
// draw list. Kinds: 0 rounded rect (+border), 1 circle, 2 linear gradient,
// 3 radial gradient, 4 glyph (atlas coverage × color), 5 image (texture ×
// tint), 6 conic gradient (sweep about the center), 7 arc stroke
// (params: thickness, from radians, kind, sweep radians), 8 rect with
// per-corner radii (the radii ride the uv slot, tl/tr/br/bl).
// Anti-aliased by a signed distance in the fragment stage.

struct Screen {
    size: vec2<f32>,
    _pad: vec2<f32>,
};

@group(0) @binding(0) var<uniform> screen: Screen;
@group(1) @binding(0) var tex: texture_2d<f32>;
@group(1) @binding(1) var samp: sampler;

struct Quad {
    @location(0) rect: vec4<f32>,          // x, y, w, h (device px)
    @location(1) color: vec4<f32>,         // fill (gradient: from; glyph/image: tint)
    @location(2) border_color: vec4<f32>,  // border (gradient: to)
    @location(3) params: vec4<f32>,        // corner, border, kind, angle (radians)
    @location(4) uv: vec4<f32>,            // u0, v0, u1, v1 (kinds 4, 5); band start, end (kinds 2, 3)
    @location(5) rot: vec4<f32>,           // sin, cos, center x, center y (device px)
    @location(6) wedge: vec4<f32>,         // mask: center x, y (device px), from rad, span rad (<0 = none)
};

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) local: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) border_color: vec4<f32>,
    @location(3) params: vec4<f32>,
    @location(4) size: vec2<f32>,
    @location(5) uv: vec2<f32>,
    @location(6) band: vec4<f32>,
    @location(7) wedge: vec4<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, q: Quad) -> VsOut {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0), vec2<f32>(0.0, 1.0));
    let c = corners[vi];
    var px = q.rect.xy + c * q.rect.zw;
    // Turned about the rotation center (clockwise on screen, y down).
    let rel = px - q.rot.zw;
    px = q.rot.zw + vec2<f32>(rel.x * q.rot.y - rel.y * q.rot.x, rel.x * q.rot.x + rel.y * q.rot.y);
    let ndc = vec2<f32>(px.x / screen.size.x * 2.0 - 1.0, 1.0 - px.y / screen.size.y * 2.0);
    var out: VsOut;
    out.pos = vec4<f32>(ndc, 0.0, 1.0);
    out.local = c * q.rect.zw;
    out.color = q.color;
    out.border_color = q.border_color;
    out.params = q.params;
    out.size = q.rect.zw;
    out.uv = mix(q.uv.xy, q.uv.zw, c);
    out.band = q.uv;
    out.wedge = q.wedge;
    return out;
}

fn rounded_rect_sdf(p: vec2<f32>, half: vec2<f32>, r: f32) -> f32 {
    let q = abs(p) - half + vec2<f32>(r, r);
    return length(max(q, vec2<f32>(0.0, 0.0))) + min(max(q.x, q.y), 0.0) - r;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    // Arc-wedge mask (GP33): only fragments whose angle about the wedge
    // center sits within [from, from + span] survive (the shared wedge
    // formula -- draw.rs wedge_contains; hits use the same test).
    if (in.wedge.w >= 0.0) {
        let d = in.pos.xy - in.wedge.xy;
        if (in.wedge.w <= 0.0) {
            discard;
        }
        if (in.wedge.w < 6.2831853 && dot(d, d) > 0.000001) {
            let tau = 6.28318530718;
            var a = (atan2(d.y, d.x) - in.wedge.z) % tau;
            if (a < 0.0) { a = a + tau; }
            if (a > in.wedge.w) {
                discard;
            }
        }
    }
    let kind = in.params.z;
    if (kind == 4.0) {
        let a = in.color.a * textureSample(tex, samp, in.uv).r;
        return vec4<f32>(in.color.rgb * a, a);
    }
    if (kind == 5.0) {
        let t = textureSample(tex, samp, in.uv) * in.color;
        return vec4<f32>(t.rgb * t.a, t.a);
    }
    let half = in.size * 0.5;
    let p = in.local - half;
    if (kind == 7.0) {
        // Arc stroke: a ring band of `thickness`, masked to the swept
        // angle (compass: 0 = up, clockwise; `from` may be negative).
        let r = min(half.x, half.y);
        let ring = abs(length(p) - (r - in.params.x * 0.5)) - in.params.x * 0.5;
        var ang = atan2(p.x, -p.y);
        var rel = ang - in.params.y;
        rel = rel - floor(rel / 6.2831853) * 6.2831853;
        if (rel > in.params.w) {
            discard;
        }
        let pm = vec4<f32>(in.color.rgb * in.color.a, in.color.a);
        return pm * clamp(0.5 - ring, 0.0, 1.0);
    }
    var corner = in.params.x;
    if (kind == 1.0) {
        corner = min(half.x, half.y);
    }
    if (kind == 8.0) {
        // Per-corner radii by quadrant: band = tl, tr, br, bl.
        if (p.y < 0.0) {
            corner = select(in.band.y, in.band.x, p.x < 0.0);
        } else {
            corner = select(in.band.z, in.band.w, p.x < 0.0);
        }
    }
    corner = min(corner, min(half.x, half.y));
    let d = rounded_rect_sdf(p, half, corner);
    let border = in.params.y;
    // Both colors premultiplied BEFORE any mix, and every mix below stays
    // premultiplied. Needed because a straight mix drags the result toward
    // a transparent color's rgb (black): a dark fringe inside a ring on a
    // light background (the color picker marker), a dark middle in a
    // gradient fading to transparent.
    let fill_pm = vec4<f32>(in.color.rgb * in.color.a, in.color.a);
    let border_pm = vec4<f32>(in.border_color.rgb * in.border_color.a, in.border_color.a);
    var color = fill_pm;
    if (kind == 2.0 || kind == 3.0 || kind == 6.0) {
        // Position along the run, then this quad's band of it: outside the
        // band another stop pair's quad paints (the run's ends belong to
        // the first and last bands).
        var run: f32;
        if (kind == 2.0) {
            let dir = vec2<f32>(cos(in.params.w), sin(in.params.w));
            run = clamp(dot(in.local / in.size - 0.5, dir) + 0.5, 0.0, 1.0);
        } else if (kind == 6.0) {
            // The sweep: 0 at the top, clockwise around the center.
            var ang = atan2(p.x, -p.y) / 6.2831853;
            if (ang < 0.0) {
                ang += 1.0;
            }
            run = ang;
        } else {
            run = clamp(length(p / half), 0.0, 1.0);
        }
        if ((in.band.x > 0.0 && run < in.band.x) || (in.band.y < 1.0 && run >= in.band.y)) {
            discard;
        }
        let span = max(in.band.y - in.band.x, 0.0001);
        let t = clamp((run - in.band.x) / span, 0.0, 1.0);
        color = mix(fill_pm, border_pm, t);
    } else if (border > 0.0) {
        // Blends fill into border over one pixel at the border's inner
        // edge, the same ramp the rim gets below.
        //
        // Needed because a plain per-pixel pick shows as a staircase
        // inside every bordered circle and rounded corner (the gradient
        // mark knob, FanRust/docs/parity/report/report.md).
        let border_coverage = clamp(0.5 + (d + border), 0.0, 1.0);
        color = mix(fill_pm, border_pm, border_coverage);
    }
    // The rim's one-pixel ramp scales the whole premultiplied pixel.
    return color * clamp(0.5 - d, 0.0, 1.0);
}
