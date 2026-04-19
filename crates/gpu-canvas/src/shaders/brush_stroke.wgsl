// brush_stroke.wgsl — 複数スタンプ位置を一括処理（ストロークセグメント用）
//
// group(0) binding(0): uniform BrushStrokeParams
// group(0) binding(1): texture_storage_2d<rgba8unorm, read_write> layer_texture
// group(0) binding(2): storage buffer stamp_positions (array<vec2<f32>>)
// dispatch: ceil(layer_width/8) x ceil(layer_height/8) workgroups

struct BrushStrokeParams {
    color: vec4<f32>,     // offset  0: RGBA normalized
    radius: f32,          // offset 16: ブラシ半径（ピクセル単位）
    opacity: f32,         // offset 20: 不透明度 0.0-1.0
    antialias: u32,       // offset 24: 0=なし, 1=あり
    stamp_count: u32,     // offset 28: 有効なスタンプ位置の数（最大 MAX_STAMP_STEPS+1）
    layer_width: u32,     // offset 32: テクスチャ幅
    layer_height: u32,    // offset 36: テクスチャ高さ
    _pad0: u32,           // offset 40: アライメント用
    _pad1: u32,           // offset 44: アライメント用
}

@group(0) @binding(0) var<uniform> params: BrushStrokeParams;
@group(0) @binding(1) var layer_texture: texture_storage_2d<rgba8unorm, read_write>;
@group(0) @binding(2) var<storage, read> stamp_positions: array<vec2<f32>>;

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= params.layer_width || id.y >= params.layer_height {
        return;
    }
    // このピクセル中心座標
    let px = vec2<f32>(f32(id.x) + 0.5, f32(id.y) + 0.5);

    // 全スタンプ位置に対して最大 coverage を求める
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

    // Source-over ブレンド
    let src_a = params.color.a * params.opacity * max_coverage;
    let dst = textureLoad(layer_texture, vec2i(id.xy));
    let out_a = src_a + dst.a * (1.0 - src_a);
    let out_rgb = params.color.rgb * src_a + dst.rgb * (1.0 - src_a);
    textureStore(layer_texture, vec2i(id.xy), vec4<f32>(out_rgb, out_a));
}
