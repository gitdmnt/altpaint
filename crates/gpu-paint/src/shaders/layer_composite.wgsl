// Composite one layer onto the running composite texture. Mirrors
// app_core::document::layer_ops::blend_pixel.

struct CompositeParams {
    dirty_x0: u32,
    dirty_y0: u32,
    dirty_x1: u32,
    dirty_y1: u32,
    layer_width: u32,
    layer_height: u32,
    blend_code: u32,
    has_mask: u32,
}

@group(0) @binding(0) var<uniform> params: CompositeParams;
@group(0) @binding(1) var layer_color: texture_2d<f32>;
@group(0) @binding(2) var mask_tex: texture_2d<f32>;
@group(0) @binding(3) var composite_rw: texture_storage_2d<rgba8unorm, read_write>;

fn blend_channel(d: f32, s: f32, code: u32) -> f32 {
    switch code {
        case 0u: { return s; }
        case 1u: { return s * d; }
        case 2u: { return 1.0 - (1.0 - s) * (1.0 - d); }
        case 3u: { return min(s + d, 1.0); }
        default: { return s; }
    }
}

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let x = id.x + params.dirty_x0;
    let y = id.y + params.dirty_y0;
    if (x >= params.dirty_x1 || y >= params.dirty_y1) {
        return;
    }
    if (x >= params.layer_width || y >= params.layer_height) {
        return;
    }
    let p = vec2<i32>(i32(x), i32(y));
    var src = textureLoad(layer_color, p, 0);
    if (params.has_mask != 0u) {
        let m = textureLoad(mask_tex, p, 0).r;
        src.a = src.a * m;
    }
    if (src.a <= 0.0) {
        return;
    }
    let dst = textureLoad(composite_rw, p);
    let out_a = src.a + dst.a * (1.0 - src.a);
    let s_r = blend_channel(dst.r, src.r, params.blend_code);
    let s_g = blend_channel(dst.g, src.g, params.blend_code);
    let s_b = blend_channel(dst.b, src.b, params.blend_code);
    let out_r = s_r * src.a + dst.r * (1.0 - src.a);
    let out_g = s_g * src.a + dst.g * (1.0 - src.a);
    let out_b = s_b * src.a + dst.b * (1.0 - src.a);
    textureStore(composite_rw, p, vec4<f32>(out_r, out_g, out_b, out_a));
}
