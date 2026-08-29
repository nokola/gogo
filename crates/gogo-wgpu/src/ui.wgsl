// UI quads: one instanced draw per (clip, texture) batch for the whole gogo
// draw list. Kinds: 0 rounded rect (+border), 1 circle, 2 linear gradient,
// 3 radial gradient, 4 glyph (atlas coverage × color), 5 image (texture ×
// tint). Anti-aliased by a signed distance in the fragment stage.

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
    @location(4) uv: vec4<f32>,            // u0, v0, u1, v1 (kinds 4, 5)
};

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) local: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) border_color: vec4<f32>,
    @location(3) params: vec4<f32>,
    @location(4) size: vec2<f32>,
    @location(5) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, q: Quad) -> VsOut {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0), vec2<f32>(0.0, 1.0));
    let c = corners[vi];
    let px = q.rect.xy + c * q.rect.zw;
    let ndc = vec2<f32>(px.x / screen.size.x * 2.0 - 1.0, 1.0 - px.y / screen.size.y * 2.0);
    var out: VsOut;
    out.pos = vec4<f32>(ndc, 0.0, 1.0);
    out.local = c * q.rect.zw;
    out.color = q.color;
    out.border_color = q.border_color;
    out.params = q.params;
    out.size = q.rect.zw;
    out.uv = mix(q.uv.xy, q.uv.zw, c);
    return out;
}

fn rounded_rect_sdf(p: vec2<f32>, half: vec2<f32>, r: f32) -> f32 {
    let q = abs(p) - half + vec2<f32>(r, r);
    return length(max(q, vec2<f32>(0.0, 0.0))) + min(max(q.x, q.y), 0.0) - r;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
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
    var corner = in.params.x;
    if (kind == 1.0) {
        corner = min(half.x, half.y);
    }
    corner = min(corner, min(half.x, half.y));
    let d = rounded_rect_sdf(p, half, corner);
    let border = in.params.y;
    var color = in.color;
    if (kind == 2.0) {
        let dir = vec2<f32>(cos(in.params.w), sin(in.params.w));
        let t = clamp(dot(in.local / in.size - 0.5, dir) + 0.5, 0.0, 1.0);
        color = mix(in.color, in.border_color, t);
    } else if (kind == 3.0) {
        let t = clamp(length(p / half), 0.0, 1.0);
        color = mix(in.color, in.border_color, t);
    } else if (border > 0.0 && d > -border) {
        color = in.border_color;
    }
    let coverage = clamp(0.5 - d, 0.0, 1.0);
    let a = color.a * coverage;
    return vec4<f32>(color.rgb * a, a);
}
