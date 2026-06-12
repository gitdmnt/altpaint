// Apply a mark texture to the active layer by source-over blending fill_color
// at each marked pixel. Shared between flood fill and lasso fill.

struct FillApplyParams {
    fill_color: vec4<f32>,
    layer_width: u32,
    layer_height: u32,
    _pad0: u32,
    _pad1: u32,
}

@group(0) @binding(0) var<uniform> params: FillApplyParams;
@group(0) @binding(1) var mark_tex: texture_2d<f32>;
@group(0) @binding(2) var layer_rw: texture_storage_2d<rgba8unorm, read_write>;

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= params.layer_width || id.y >= params.layer_height) {
        return;
    }
    let p = vec2<i32>(i32(id.x), i32(id.y));
    let m = textureLoad(mark_tex, p, 0).r;
    if (m < 0.5) {
        return;
    }

    let dst = textureLoad(layer_rw, p);
    // Straight-alpha source-over, matching CPU app_core::document::layer_ops::blend_pixel
    // with BlendMode::Normal:
    //   out_c = src_c * src_a + dst_c * (1 - src_a)
    //   out_a = src_a + dst_a * (1 - src_a)
    let sa = params.fill_color.a;
    let out_r = params.fill_color.r * sa + dst.r * (1.0 - sa);
    let out_g = params.fill_color.g * sa + dst.g * (1.0 - sa);
    let out_b = params.fill_color.b * sa + dst.b * (1.0 - sa);
    let out_a = sa + dst.a * (1.0 - sa);
    textureStore(layer_rw, p, vec4<f32>(out_r, out_g, out_b, out_a));
}
