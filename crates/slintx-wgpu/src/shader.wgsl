// One pipeline draws every primitive. Shapes are signed distance fields
// evaluated per pixel, so edges get analytic coverage; path triangles rely on
// MSAA instead. All positions are device pixels, origin top-left.

const KIND_FILL: u32 = 0u;
const KIND_RING: u32 = 1u;
const KIND_SHADOW: u32 = 2u;
const KIND_IMAGE: u32 = 3u;
const KIND_GLYPH: u32 = 4u;
const KIND_SOLID: u32 = 5u;

const FLAG_TILE_X: u32 = 1u;
const FLAG_TILE_Y: u32 = 2u;
const FLAG_COLORIZE: u32 = 4u;

const BRUSH_SOLID: u32 = 0u;
const BRUSH_LINEAR: u32 = 1u;
const BRUSH_RADIAL: u32 = 2u;
const BRUSH_CONIC: u32 = 3u;

const TAU: f32 = 6.283185307179586;

struct Globals {
    viewport: vec2<f32>,
    ramp_rows: f32,
    _pad: f32,
}

struct Prim {
    rect: vec4<f32>,
    radii: vec4<f32>,
    a: vec4<f32>,
    b: vec4<f32>,
    c: vec4<f32>,
    kind: u32,
    brush: u32,
    flags: u32,
    clip: u32,
    opacity: f32,
    _p0: f32,
    _p1: f32,
    _p2: f32,
}

struct Brush {
    color: vec4<f32>,
    geom: vec4<f32>,
    kind: u32,
    ramp: u32,
    _p0: u32,
    _p1: u32,
}

struct Clip {
    rect: vec4<f32>,
    rrect: vec4<f32>,
    radii: vec4<f32>,
}

@group(0) @binding(0) var<uniform> globals: Globals;
@group(0) @binding(1) var<storage, read> prims: array<Prim>;
@group(0) @binding(2) var<storage, read> brushes: array<Brush>;
@group(0) @binding(3) var<storage, read> clips: array<Clip>;
@group(0) @binding(4) var ramp_tex: texture_2d<f32>;
@group(0) @binding(5) var ramp_sampler: sampler;
@group(0) @binding(6) var atlas_tex: texture_2d<f32>;
@group(1) @binding(0) var image_tex: texture_2d<f32>;
@group(1) @binding(1) var image_sampler: sampler;

struct VsIn {
    @location(0) pos: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) prim: u32,
}

struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) @interpolate(flat) prim: u32,
}

@vertex
fn vs_main(v: VsIn) -> VsOut {
    var out: VsOut;
    let ndc = vec2<f32>(
        v.pos.x / globals.viewport.x * 2.0 - 1.0,
        1.0 - v.pos.y / globals.viewport.y * 2.0,
    );
    out.position = vec4<f32>(ndc, 0.0, 1.0);
    out.uv = v.uv;
    out.prim = v.prim;
    return out;
}

// Radii are top-left, top-right, bottom-right, bottom-left.
fn sd_rrect(p: vec2<f32>, rect: vec4<f32>, radii: vec4<f32>) -> f32 {
    let center = (rect.xy + rect.zw) * 0.5;
    let half = (rect.zw - rect.xy) * 0.5;
    if (half.x <= 0.0 || half.y <= 0.0) {
        return 1e9;
    }
    let q0 = p - center;
    var r: f32;
    if (q0.x < 0.0) {
        r = select(radii.w, radii.x, q0.y < 0.0);
    } else {
        r = select(radii.z, radii.y, q0.y < 0.0);
    }
    r = min(r, min(half.x, half.y));
    let q = abs(q0) - half + vec2<f32>(r);
    return min(max(q.x, q.y), 0.0) + length(max(q, vec2<f32>(0.0))) - r;
}

fn coverage(d: f32) -> f32 {
    return clamp(0.5 - d, 0.0, 1.0);
}

fn ramp(row: u32, t: f32) -> vec4<f32> {
    let u = (clamp(t, 0.0, 1.0) * 255.0 + 0.5) / 256.0;
    let v = (f32(row) + 0.5) / globals.ramp_rows;
    return textureSampleLevel(ramp_tex, ramp_sampler, vec2<f32>(u, v), 0.0);
}

fn brush_color(index: u32, p: vec2<f32>) -> vec4<f32> {
    let b = brushes[index];
    switch b.kind {
        case BRUSH_LINEAR: {
            let d = b.geom.zw - b.geom.xy;
            let t = dot(p - b.geom.xy, d) / max(dot(d, d), 1e-6);
            return ramp(b.ramp, t);
        }
        case BRUSH_RADIAL: {
            return ramp(b.ramp, length(p - b.geom.xy) / max(b.geom.z, 1e-6));
        }
        case BRUSH_CONIC: {
            // Zero points up and grows clockwise, as in CSS `conic-gradient`.
            let d = p - b.geom.xy;
            var angle = atan2(d.x, -d.y);
            if (angle < 0.0) {
                angle += TAU;
            }
            return ramp(b.ramp, angle / TAU);
        }
        default: {
            return b.color;
        }
    }
}

// The approximation of the error function from Abramowitz and Stegun (7.1.27).
fn erf2(x: vec2<f32>) -> vec2<f32> {
    let s = sign(x);
    let a = abs(x);
    var r = 1.0 + (0.278393 + (0.230389 + 0.078108 * (a * a)) * a) * a;
    r = r * r;
    return s - s / (r * r);
}

fn gaussian(x: f32, sigma: f32) -> f32 {
    return exp(-(x * x) / (2.0 * sigma * sigma)) / (sqrt(TAU) * sigma);
}

// Blur of a rounded box along x for one row: the box's horizontal extent at
// that row, integrated against the Gaussian in closed form.
fn shadow_row(x: f32, y: f32, sigma: f32, corner: f32, half: vec2<f32>) -> f32 {
    let delta = min(half.y - corner - abs(y), 0.0);
    let curved = half.x - corner + sqrt(max(0.0, corner * corner - delta * delta));
    let integral = 0.5 + 0.5 * erf2((x + vec2<f32>(-curved, curved)) * (sqrt(0.5) / sigma));
    return integral.y - integral.x;
}

// The y direction has no closed form for a rounded box, so it is sampled.
fn shadow(p: vec2<f32>, rect: vec4<f32>, corner: f32, sigma: f32) -> f32 {
    let center = (rect.xy + rect.zw) * 0.5;
    let half = (rect.zw - rect.xy) * 0.5;
    let q = p - center;
    let low = q.y - half.y;
    let high = q.y + half.y;
    let start = clamp(-3.0 * sigma, low, high);
    let end = clamp(3.0 * sigma, low, high);
    let step = (end - start) / 8.0;
    var y = start + step * 0.5;
    var value = 0.0;
    for (var i = 0; i < 8; i++) {
        value += shadow_row(q.x, q.y - y, sigma, corner, half) * gaussian(y, sigma) * step;
        y += step;
    }
    return clamp(value, 0.0, 1.0);
}

fn rem_euclid(x: f32, m: f32) -> f32 {
    return x - m * floor(x / m);
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let pr = prims[in.prim];
    let p = in.position.xy;
    let cl = clips[pr.clip];
    let clip_cov = coverage(sd_rrect(p, cl.rect, vec4<f32>(0.0)))
        * coverage(sd_rrect(p, cl.rrect, cl.radii));

    var color = vec4<f32>(0.0);
    switch pr.kind {
        case KIND_FILL: {
            color = brush_color(pr.brush, p) * coverage(sd_rrect(p, pr.rect, pr.radii));
        }
        case KIND_RING: {
            let outer = coverage(sd_rrect(p, pr.rect, pr.radii));
            let inner = coverage(sd_rrect(p, pr.a, pr.b));
            color = brush_color(pr.brush, p) * max(outer - inner, 0.0);
        }
        case KIND_SHADOW: {
            let sigma = pr.a.x;
            var cov: f32;
            if (sigma < 0.1) {
                cov = coverage(sd_rrect(p, pr.rect, pr.radii));
            } else {
                cov = shadow(p, pr.rect, pr.radii.x, sigma);
            }
            color = brush_color(pr.brush, p) * cov;
        }
        case KIND_IMAGE: {
            // a: target origin, source pixels per device pixel. b: source clip
            // rect (x, y, w, h). c: tile phase, source image size.
            var rel = (p - pr.a.xy) * pr.a.zw;
            if ((pr.flags & FLAG_TILE_X) != 0u) {
                rel.x = rem_euclid(rel.x + pr.c.x, pr.b.z);
            } else {
                rel.x = clamp(rel.x, 0.0, pr.b.z);
            }
            if ((pr.flags & FLAG_TILE_Y) != 0u) {
                rel.y = rem_euclid(rel.y + pr.c.y, pr.b.w);
            } else {
                rel.y = clamp(rel.y, 0.0, pr.b.w);
            }
            let uv = (pr.b.xy + rel) / pr.c.zw;
            let texel = textureSampleLevel(image_tex, image_sampler, uv, 0.0);
            let area = coverage(sd_rrect(p, pr.rect, vec4<f32>(0.0)));
            if ((pr.flags & FLAG_COLORIZE) != 0u) {
                color = brush_color(pr.brush, p) * texel.a * area;
            } else {
                color = texel * area;
            }
        }
        case KIND_GLYPH: {
            let mask = textureLoad(atlas_tex, vec2<i32>(floor(in.uv)), 0).r;
            color = brush_color(pr.brush, p) * mask;
        }
        default: {
            color = brush_color(pr.brush, p);
        }
    }
    return color * clip_cov * pr.opacity;
}
