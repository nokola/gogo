// UI quads: one instanced draw per (clip, texture, plain or not) batch for
// the whole gogo draw list; fs_main draws every kind, fs_plain (below) the
// three cheap ones. Kinds: 0 rounded rect (+border), 1 circle, 2 linear gradient,
// 3 radial gradient, 4 glyph (atlas coverage × color), 5 image (texture ×
// tint), 6 conic gradient (sweep about the center), 7 arc stroke
// (params: thickness, from radians, kind, sweep radians), 8 rect with
// per-corner radii (sent in the uv slot: tl, tr, br, bl).
// Anti-aliased by a signed distance in the fragment stage.

struct Screen {
    size: vec2<f32>,
    _pad: vec2<f32>,
};

@group(0) @binding(0) var<uniform> screen: Screen;
@group(1) @binding(0) var tex: texture_2d<f32>;
@group(1) @binding(1) var samp: sampler;
// The gradient tables, one row per gradient on screen, read by exact
// texel (no sampler, no filtering): `grad_map` holds, per slot along the
// run, WHICH stop pair covers it; `grad_stops` holds the stops, two
// texels each (the premultiplied Oklab color as 16-bit fixed point over
// -1..1, then the position as two 16-bit halves of a 32-bit fraction).
// Integer textures on purpose: a float32 texture bound unfiltered is
// INCOMPLETE on GLES 3.0 without OES_texture_float_linear and samples as
// opaque black (seen on an Adreno 330); integers always fetch.
@group(2) @binding(0) var grad_map: texture_2d<u32>;
@group(2) @binding(1) var grad_stops: texture_2d<u32>;

fn grad_stop_color(pair_texel: i32, row: i32) -> vec4<f32> {
    let raw = textureLoad(grad_stops, vec2<i32>(pair_texel, row), 0);
    return vec4<f32>(raw) / 32767.5 - vec4<f32>(1.0);
}

fn grad_stop_pos(pair_texel: i32, row: i32) -> f32 {
    let raw = textureLoad(grad_stops, vec2<i32>(pair_texel, row), 0);
    return (f32(raw.x) * 65536.0 + f32(raw.y)) / 4294967295.0;
}

struct Quad {
    @location(0) rect: vec4<f32>,          // x, y, w, h (device px)
    @location(1) color: vec4<f32>,         // fill (glyph/image: tint; a gradient's colors are in its table row)
    @location(2) border_color: vec4<f32>,  // border
    @location(3) params: vec4<f32>,        // corner, border, kind, angle (radians)
    @location(4) uv: vec4<f32>,            // u0, v0, u1, v1 (kinds 4, 5); table row, slot count, period start, period length (kinds 2, 3, 6); dash offset, dash, period (kind 7)
    @location(5) rot: vec4<f32>,           // sin, cos, center x, center y (device px)
    @location(6) wedge: vec4<f32>,         // mask: center x, y (device px), from rad, span rad (<0 = none)
    @location(7) swing: vec4<f32>,         // 3D swing: sin, cos, hinge x (device px), camera distance (device px; 0 = none)
    @location(8) eye: vec4<f32>,           // the swing's camera: on x, y (device px)
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
    // The 3D swing (the WP7 turnstile): the point turns about the vertical
    // hinge at x = swing.z, going AWAY from the viewer on the positive
    // side (z = dx·sin), then a camera `swing.w` in front of `eye` sees
    // it in perspective — Silverlight's PlaneProjection. The clip w is
    // the depth ratio, so every varying interpolates perspective-correct
    // across the turned quad (a rounded corner or a glyph stays true on
    // the door).
    var w = 1.0;
    if (q.swing.w > 0.0) {
        let dx = px.x - q.swing.z;
        let x3 = q.swing.z + dx * q.swing.y;
        let z3 = dx * q.swing.x;
        w = max((q.swing.w + z3) / q.swing.w, 0.05);
        px = q.eye.xy + (vec2<f32>(x3, px.y) - q.eye.xy) / w;
    }
    let ndc = vec2<f32>(px.x / screen.size.x * 2.0 - 1.0, 1.0 - px.y / screen.size.y * 2.0);
    var out: VsOut;
    out.pos = vec4<f32>(ndc * w, 0.0, w);
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

// A premultiplied-Oklab value back to premultiplied sRGB (Ottosson's
// inverse + the sRGB encode). Out-of-gamut mixes clamp per channel; zero
// alpha has no hue to divide out and is transparent black.
fn srgb_pm_from_oklab_pm(lab_pm: vec4<f32>) -> vec4<f32> {
    let alpha = lab_pm.w;
    if (alpha <= 0.0) {
        return vec4<f32>(0.0);
    }
    let lab = lab_pm.xyz / alpha;
    let l_ = lab.x + 0.3963377774 * lab.y + 0.2158037573 * lab.z;
    let m_ = lab.x - 0.1055613458 * lab.y - 0.0638541728 * lab.z;
    let s_ = lab.x - 0.0894841775 * lab.y - 1.2914855480 * lab.z;
    let l = l_ * l_ * l_;
    let m = m_ * m_ * m_;
    let s = s_ * s_ * s_;
    let lin = clamp(vec3<f32>(
        4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
        -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
        -0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s), vec3<f32>(0.0), vec3<f32>(1.0));
    let enc = select(1.055 * pow(lin, vec3<f32>(1.0 / 2.4)) - 0.055, lin * 12.92, lin <= vec3<f32>(0.0031308));
    return vec4<f32>(enc * alpha, alpha);
}

// The fragment stage of a pipeline pair's `plain` pipeline (`PipelinePair`,
// ui.rs): an image, a glyph, or a rect without a border, drawn with the
// SAME math as fs_main's arms for those kinds. ui.rs `quad_is_plain` sends
// nothing else here.
//
// Needed because a phone GPU of 2014 runs ALL of fs_main for every pixel,
// the kinds a pixel does not use included: a full-screen picture and flat
// panels, 97 % and more of a screen's pixels, cost about five times less
// here (OnePlus One: the screen pass 31-49 ms through fs_main alone,
// 6-10 ms through the pair).
//
// No branch on purpose: one texture read, the three results, a pick. A
// rect reads the batch's texture at uv 0 and drops the result.
@fragment
fn fs_plain(in: VsOut) -> @location(0) vec4<f32> {
    let t = textureSample(tex, samp, in.uv);
    let image = t * in.color;
    let image_pm = vec4<f32>(image.rgb * image.a, image.a);
    let glyph_a = in.color.a * t.r;
    let glyph_pm = vec4<f32>(in.color.rgb * glyph_a, glyph_a);
    let half = in.size * 0.5;
    let corner = min(in.params.x, min(half.x, half.y));
    let d = rounded_rect_sdf(in.local - half, half, corner);
    let rect_pm = vec4<f32>(in.color.rgb * in.color.a, in.color.a) * clamp(0.5 - d, 0.0, 1.0);
    let kind = in.params.z;
    return select(select(rect_pm, glyph_pm, kind == 4.0), image_pm, kind == 5.0);
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    // Arc-wedge mask: only fragments whose angle about the wedge center
    // sits within [from, from + span] survive (the shared wedge formula:
    // gogo-ui's `wedge_contains` in draw.rs; hits use the same test).
    // The angle is on the compass (0 = up, clockwise), as the Arc
    // stroke's below.
    if (in.wedge.w >= 0.0) {
        let d = in.pos.xy - in.wedge.xy;
        if (in.wedge.w <= 0.0) {
            discard;
        }
        if (in.wedge.w < 6.2831853 && dot(d, d) > 0.000001) {
            let tau = 6.28318530718;
            var a = (atan2(d.x, -d.y) - in.wedge.z) % tau;
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
        // The dash mask (the quad's raw uv, `band` here: offset, dash,
        // period in radians; period 0 = solid): where the angle past the
        // offset falls in its period, the stroke shows for the first
        // `dash`. The dash ends get the same one-pixel ramp as the ring's
        // rim, measured along the centerline.
        var dash_cov = 1.0;
        if (in.band.z > 0.0) {
            let ph = fract((rel - in.band.x) / in.band.z) * in.band.z;
            let along = min(ph, in.band.y - ph) * (r - in.params.x * 0.5);
            dash_cov = clamp(0.5 + along, 0.0, 1.0);
        }
        let pm = vec4<f32>(in.color.rgb * in.color.a, in.color.a);
        return pm * clamp(0.5 - ring, 0.0, 1.0) * dash_cov;
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
    // light background, a dark middle in a gradient fading to transparent.
    let fill_pm = vec4<f32>(in.color.rgb * in.color.a, in.color.a);
    let border_pm = vec4<f32>(in.border_color.rgb * in.border_color.a, in.border_color.a);
    var color = fill_pm;
    if (kind == 2.0 || kind == 3.0 || kind == 6.0) {
        // Where this pixel sits along the gradient's run, 0..1.
        var run: f32;
        if (kind == 2.0) {
            let dir = vec2<f32>(cos(in.params.w), sin(in.params.w));
            let raw = dot(in.local / in.size - 0.5, dir) + 0.5;
            if (in.band.w > 0.0) {
                // Tiled (`repeat=` with a `from=`/`to=` span): the span is
                // one period and the band is 0..1 of it.
                run = fract((raw - in.band.z) / in.band.w);
            } else {
                run = clamp(raw, 0.0, 1.0);
            }
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
        // The lookup: the run's slot names the stop pair — the CPU
        // filled the map with the SAME floor (slot = floor(run * n)), so
        // the two agree at every slot edge — then the pair's two stops
        // blend by where the run sits between them. Before the first stop
        // or past the last, t clamps and the edge color holds.
        // The row and the slot count are whole numbers interpolated across
        // the quad: rounded, so a varying that lands at 2.9999 on a lower-
        // precision GPU still names row 3.
        let n = floor(in.band.y + 0.5);
        let row = i32(in.band.x + 0.5);
        let slot = clamp(i32(floor(run * n)), 0, i32(n) - 1);
        let pair = i32(textureLoad(grad_map, vec2<i32>(slot, row), 0).r);
        let c0 = grad_stop_color(pair * 2, row);
        let p0 = grad_stop_pos(pair * 2 + 1, row);
        let c1 = grad_stop_color(pair * 2 + 2, row);
        let p1 = grad_stop_pos(pair * 2 + 3, row);
        let t = clamp((run - p0) / max(p1 - p0, 0.0001), 0.0, 1.0);
        // Authored colors mix in premultiplied Oklab (gogo's one rule for
        // mixing colors; gogo-ui's color.rs is the CPU twin of this math):
        // the table holds each stop as (L,a,b)*alpha + alpha, so the plain
        // mix IS the Oklab lerp; only the mixed result converts back to
        // the premultiplied sRGB the blend expects.
        color = srgb_pm_from_oklab_pm(mix(c0, c1, t));
    } else if (border > 0.0) {
        // Blends fill into border over one pixel at the border's inner
        // edge, the same ramp the rim gets below.
        //
        // Needed because a plain per-pixel pick shows as a staircase
        // inside every bordered circle and rounded corner.
        let border_coverage = clamp(0.5 + (d + border), 0.0, 1.0);
        color = mix(fill_pm, border_pm, border_coverage);
    }
    // The rim's one-pixel ramp scales the whole premultiplied pixel.
    return color * clamp(0.5 - d, 0.0, 1.0);
}
