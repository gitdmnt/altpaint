// erase_stamp.wgsl — 消しゴムツール用（alpha を削減する）
//
// brush_stroke.wgsl と同一レイアウトで、ブレンドモードのみ異なる。
// group(0) binding(0): uniform BrushStrokeParams
// group(0) binding(1): texture_storage_2d<rgba8unorm, read_write> layer_texture
// group(0) binding(2): storage buffer stamp_positions (array<vec2<f32>>)

struct BrushStrokeParams {
    color: vec4<f32>,
    radius: f32,
    opacity: f32,
    antialias: u32,
    stamp_count: u32,
    layer_width: u32,
    layer_height: u32,
    _pad0: u32,
    _pad1: u32,
}

@group(0) @binding(0) var<uniform> params: BrushStrokeParams;
@group(0) @binding(1) var layer_texture: texture_storage_2d<rgba8unorm, read_write>;
@group(0) @binding(2) var<storage, read> stamp_positions: array<vec2<f32>>;

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= params.layer_width || id.y >= params.layer_height {
        return;
    }
    let px = vec2<f32>(f32(id.x) + 0.5, f32(id.y) + 0.5);

    var max_coverage = 0.0;
    for (var i = 0u; i < params.stamp_count; i++) {
        let center = stamp_positions[i] + vec2<f32>(0.5, 0.5);
        let d = distance(px, center);
        var cov: f32;
        if params.antialias != 0u {
            cov = clamp(params.radius + 0.5 - d, 0.0, 1.0);
        } else {
            cov = select(0.0, 1.0, d <= params.radius);
        }
        max_coverage = max(max_coverage, cov);
    }
    if max_coverage <= 0.0 {
        return;
    }

    // 消去: alpha を opacity * coverage 分だけ削減する
    let erase_amount = params.opacity * max_coverage;
    let dst = textureLoad(layer_texture, vec2i(id.xy));
    let new_a = dst.a * (1.0 - erase_amount);
    textureStore(layer_texture, vec2i(id.xy), vec4<f32>(dst.rgb, new_a));
}
