// brush_stamp.wgsl — 単点スタンプ（真円ブラシ用）
//
// group(0) binding(0): uniform BrushStampParams
// group(0) binding(1): texture_storage_2d<rgba8unorm, read_write> layer_texture
// dispatch: ceil(layer_width/8) x ceil(layer_height/8) workgroups

struct BrushStampParams {
    color: vec4<f32>,    // offset  0: RGBA normalized
    center: vec2<f32>,   // offset 16: スタンプ中心座標（ピクセル単位）
    radius: f32,         // offset 24: ブラシ半径（ピクセル単位）
    opacity: f32,        // offset 28: 不透明度 0.0-1.0
    antialias: u32,      // offset 32: 0=なし, 1=あり
    layer_width: u32,    // offset 36: テクスチャ幅
    layer_height: u32,   // offset 40: テクスチャ高さ
    _pad: u32,           // offset 44: アライメント用パディング
}

@group(0) @binding(0) var<uniform> params: BrushStampParams;
@group(0) @binding(1) var layer_texture: texture_storage_2d<rgba8unorm, read_write>;

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= params.layer_width || id.y >= params.layer_height {
        return;
    }
    let px = vec2<f32>(f32(id.x) + 0.5, f32(id.y) + 0.5);
    let d = distance(px, params.center + vec2<f32>(0.5, 0.5));
    var coverage: f32;
    if params.antialias != 0u {
        coverage = clamp(params.radius + 0.5 - d, 0.0, 1.0);
    } else {
        coverage = select(0.0, 1.0, d <= params.radius);
    }
    if coverage <= 0.0 {
        return;
    }
    let src_a = params.color.a * params.opacity * coverage;
    let dst = textureLoad(layer_texture, vec2i(id.xy));
    let out_a = src_a + dst.a * (1.0 - src_a);
    let out_rgb = params.color.rgb * src_a + dst.rgb * (1.0 - src_a);
    textureStore(layer_texture, vec2i(id.xy), vec4<f32>(out_rgb, out_a));
}