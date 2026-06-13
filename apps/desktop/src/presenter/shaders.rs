//! presenter が使う WGSL シェーダソースとユニフォームバイト数定数を集約する。
//!
//! テクスチャ提示シェーダ (`PRESENT_SHADER`) と 3 種の quad シェーダ
//! (solid / circle / line) を 1 箇所にまとめ、パイプライン定義から分離する。

/// WGSL（WebGPU Shading Language）で書かれた描画シェーダ。
///
/// # 全体の役割
/// 各レイヤーテクスチャを「四角形（クワッド）」として画面に貼る。
/// 頂点シェーダが 6 頂点（三角形 2 枚）の位置を計算し、
/// フラグメントシェーダが各ピクセルの色をテクスチャからサンプルして返す。
pub(super) const PRESENT_SHADER: &str = r#"
/// GPU 側で受け取るユニフォームデータ。
/// Rust 側の `quad_uniform_bytes` で詰めて `write_buffer` で転送する。
struct LayerUniform {
    /// クリップ空間（NDC: -1.0〜+1.0）での描画矩形の左上。
    rect_min: vec2<f32>,
    /// クリップ空間での描画矩形の右下。
    rect_max: vec2<f32>,
    /// テクスチャ UV の左上（通常 0,0）。
    uv_min: vec2<f32>,
    /// テクスチャ UV の右下（通常 1,1）。
    uv_max: vec2<f32>,
    /// transform.x = 回転角度(度), .y = flip_x フラグ, .z = flip_y フラグ, .w = 未使用。
    transform: vec4<f32>,
    /// metrics.x = bbox 幅(px), .y = bbox 高さ(px), .z/.w = 未使用。
    metrics: vec4<f32>,
};

/// バインディング 0: 2D テクスチャ（フラグメントシェーダで参照する画像データ）。
@group(0) @binding(0)
var present_texture: texture_2d<f32>;

/// バインディング 1: サンプラー（テクスチャの拡縮・端処理の方法を指定）。
@group(0) @binding(1)
var present_sampler: sampler;

/// バインディング 2: ユニフォームバッファ（フレームごとに CPU から書き換えられる定数群）。
@group(0) @binding(2)
var<uniform> layer_uniform: LayerUniform;

/// 頂点シェーダの出力。
/// `position` はクリップ空間座標（GPU が画面座標へ変換する）。
/// `unit` は 0〜1 の正規化された矩形内座標で、UV 計算に使う。
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) unit: vec2<f32>,
}

/// 頂点シェーダ: 頂点バッファを使わず vertex_index だけで 6 頂点分の座標を生成する。
///
/// 三角形 2 枚（合計 6 頂点）で四角形を描く。各頂点の「単位矩形内座標」を
/// unit 配列に直書きし、LayerUniform の rect_min/rect_max で NDC 座標へ変換する。
///
/// NDC（Normalized Device Coordinates）:
///   左端 = -1.0, 右端 = +1.0, 上端 = +1.0, 下端 = -1.0
///   ※ wgpu の Y 軸は上が正方向（DirectX 系と同じ）。
@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    // 四角形を形成する 2 つの三角形の頂点を (x, y) の単位座標で列挙。
    // インデックス順: 左上→右上→左下 / 左下→右上→右下
    var unit = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0),  // 左上
        vec2<f32>(1.0, 0.0),  // 右上
        vec2<f32>(0.0, 1.0),  // 左下
        vec2<f32>(0.0, 1.0),  // 左下（2 枚目の三角形）
        vec2<f32>(1.0, 0.0),  // 右上
        vec2<f32>(1.0, 1.0)   // 右下
    );

    // この頂点の単位座標を取得。
    let current = unit[vertex_index];
    var output: VertexOutput;

    // mix(a, b, t) = a + (b - a) * t で線形補間。
    // 単位座標 0〜1 を rect_min〜rect_max の NDC 範囲へスケール変換する。
    output.position = vec4<f32>(
        mix(layer_uniform.rect_min.x, layer_uniform.rect_max.x, current.x),
        mix(layer_uniform.rect_min.y, layer_uniform.rect_max.y, current.y),
        0.0,  // 奥行きは使わない（2D 描画なので常に 0）
        1.0,  // w 成分: 透視除算で 1.0 にするため 1.0 固定
    );
    // 補間のために単位座標をフラグメントシェーダへ渡す。
    output.unit = current;
    return output;
}

/// 回転済み UV を元の（未回転）テクスチャ UV へ逆変換するヘルパー。
///
/// キャンバスの回転表示に対応するため、スクリーン上の UV 座標を
/// 回転前のテクスチャ空間へ戻す逆回転を行う。
/// 逆回転なので angle に負号を付けている（-rotation_degrees）。
fn rotated_to_source_uv(rotated_uv: vec2<f32>, rotation_degrees: f32) -> vec2<f32> {
    // 度数法をラジアンへ変換。π/180 ≈ 0.017453292519943295
    let radians = -rotation_degrees * 0.017453292519943295;
    let cos_theta = cos(radians);
    let sin_theta = sin(radians);
    // 2D 回転行列の適用: [cos θ, -sin θ; sin θ, cos θ] × [x; y]
    return vec2<f32>(
        rotated_uv.x * cos_theta - rotated_uv.y * sin_theta,
        rotated_uv.x * sin_theta + rotated_uv.y * cos_theta,
    );
}

/// フラグメントシェーダ: ピクセルごとにテクスチャ色を返す。
///
/// 処理の流れ:
///   1. 単位座標 → UV 座標へ変換
///   2. UV を中心原点のピクセル座標へ変換
///   3. flip_x / flip_y フラグで反転
///   4. 回転を逆適用してソースのテクスチャ座標へ変換
///   5. テクスチャ外なら透明を返し、内なら textureSample でサンプリング
@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    // 単位座標 (0〜1) を uv_min〜uv_max の UV 範囲へ線形補間。
    // 部分テクスチャ表示（UV 範囲を絞る）に対応するための変換。
    var rotated_uv = vec2<f32>(
        mix(layer_uniform.uv_min.x, layer_uniform.uv_max.x, input.unit.x),
        mix(layer_uniform.uv_min.y, layer_uniform.uv_max.y, input.unit.y),
    );

    // UV (0〜1) を中心原点のピクセル座標へ変換。
    // (uv - 0.5) * size で [-size/2, +size/2] の範囲になる。
    // 回転をピクセル空間で行うことでアスペクト比の歪みを防ぐ。
    var rotated_point = vec2<f32>(
        (rotated_uv.x - 0.5) * layer_uniform.metrics.x,
        (rotated_uv.y - 0.5) * layer_uniform.metrics.y,
    );

    // flip_x が 1.0 なら X 軸を反転（左右ミラー）。
    if layer_uniform.transform.y > 0.5 {
        rotated_point.x = -rotated_point.x;
    }
    // flip_y が 1.0 なら Y 軸を反転（上下ミラー）。
    if layer_uniform.transform.z > 0.5 {
        rotated_point.y = -rotated_point.y;
    }

    // 回転済みピクセル座標をソーステクスチャのピクセル座標へ逆変換。
    let source_point = rotated_to_source_uv(rotated_point, layer_uniform.transform.x);

    // テクスチャの実際のピクセルサイズを取得（ミップレベル 0）。
    let source_size = vec2<f32>(textureDimensions(present_texture));

    // ピクセル座標（中心原点）を UV（左上原点 0〜1）へ戻す。
    let uv = vec2<f32>(
        (source_point.x + source_size.x * 0.5) / source_size.x,
        (source_point.y + source_size.y * 0.5) / source_size.y,
    );

    // テクスチャ範囲外に出たピクセルは完全透明にする（クリッピング）。
    if uv.x < 0.0 || uv.y < 0.0 || uv.x >= 1.0 || uv.y >= 1.0 {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }

    // present_sampler でバイリニアフィルタリングしながらテクスチャ色を取得。
    return textureSample(present_texture, present_sampler, uv);
}
"#;

/// ユニフォームバッファのバイトサイズ。
/// `LayerUniform` は f32 × 16 = 64 バイト（vec2×4 + vec4×2 = 16 floats）。
pub(super) const LAYER_UNIFORM_SIZE: u64 = std::mem::size_of::<[f32; 16]>() as u64;

/// solid quad 用シェーダ。各 quad は NDC 矩形と RGBA 色だけで指定される。
pub(super) const SOLID_QUAD_SHADER: &str = r#"
struct SolidQuadUniform {
    rect_ndc: vec4<f32>,
    color: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: SolidQuadUniform;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var unit = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(1.0, 1.0)
    );
    let p = unit[vertex_index];
    var output: VertexOutput;
    output.position = vec4<f32>(
        mix(u.rect_ndc.x, u.rect_ndc.z, p.x),
        mix(u.rect_ndc.y, u.rect_ndc.w, p.y),
        0.0,
        1.0,
    );
    return output;
}

@fragment
fn fs_main(_in: VertexOutput) -> @location(0) vec4<f32> {
    return u.color;
}
"#;

/// solid quad uniform のバイト数 (vec4 × 2 = 32 バイト)。
pub(super) const SOLID_QUAD_UNIFORM_SIZE: u64 = std::mem::size_of::<[f32; 8]>() as u64;

/// 円リング SDF シェーダ。
///
/// uniform:
///   rect_ndc: bbox を NDC 化した矩形
///   bbox_min_px / bbox_max_px: bbox のピクセル座標 (フラグメントで再構成)
///   center_px: 円の中心 (px)
///   radius_thickness: x = 半径(px), y = 線幅(px), z/w = 未使用
///   color: 線色
pub(super) const CIRCLE_QUAD_SHADER: &str = r#"
struct CircleUniform {
    rect_ndc: vec4<f32>,
    bbox_min_px: vec2<f32>,
    bbox_max_px: vec2<f32>,
    center_px: vec2<f32>,
    /// .x = 半径(px), .y = 線幅(px)
    radius_thickness: vec2<f32>,
    color: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: CircleUniform;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) unit: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var unit = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(1.0, 1.0)
    );
    let p = unit[vertex_index];
    var output: VertexOutput;
    output.position = vec4<f32>(
        mix(u.rect_ndc.x, u.rect_ndc.z, p.x),
        mix(u.rect_ndc.y, u.rect_ndc.w, p.y),
        0.0,
        1.0,
    );
    output.unit = p;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let pixel = vec2<f32>(
        mix(u.bbox_min_px.x, u.bbox_max_px.x, input.unit.x),
        mix(u.bbox_min_px.y, u.bbox_max_px.y, input.unit.y),
    );
    let dist = distance(pixel, u.center_px);
    let radius = u.radius_thickness.x;
    let thickness = u.radius_thickness.y;
    let edge = abs(dist - radius);
    // 1 px 幅のフェザリングでアンチエイリアス。edge <= thickness の領域に色を載せる。
    let alpha = 1.0 - smoothstep(thickness - 0.5, thickness + 0.5, edge);
    if alpha <= 0.0 {
        discard;
    }
    return vec4<f32>(u.color.rgb, u.color.a * alpha);
}
"#;

/// 円リング uniform バイト数。
/// レイアウト (WGSL std140):
///   vec4 rect_ndc        (offset 0,  16B)
///   vec2 bbox_min_px     (offset 16, 8B)
///   vec2 bbox_max_px     (offset 24, 8B)
///   vec2 center_px       (offset 32, 8B)
///   vec2 radius_thickness (offset 40, 8B)
///   vec4 color           (offset 48, 16B)
/// 合計 64 バイト。
pub(super) const CIRCLE_QUAD_UNIFORM_SIZE: u64 = std::mem::size_of::<[f32; 16]>() as u64;

/// 線分カプセル SDF シェーダ。
///
/// uniform:
///   rect_ndc: bbox を NDC 化した矩形
///   bbox_min_px / bbox_max_px: bbox のピクセル座標
///   start_px / end_px: 線分端点 (px)
///   thickness: x = カプセル半径 (px), y/z/w = 未使用
///   color: 線色
pub(super) const LINE_QUAD_SHADER: &str = r#"
struct LineUniform {
    rect_ndc: vec4<f32>,
    bbox_min_px: vec2<f32>,
    bbox_max_px: vec2<f32>,
    start_px: vec2<f32>,
    end_px: vec2<f32>,
    // .x のみ使用。WGSL std140 アライメントのため vec4 として宣言。
    thickness: vec4<f32>,
    color: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: LineUniform;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) unit: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var unit = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(1.0, 1.0)
    );
    let p = unit[vertex_index];
    var output: VertexOutput;
    output.position = vec4<f32>(
        mix(u.rect_ndc.x, u.rect_ndc.z, p.x),
        mix(u.rect_ndc.y, u.rect_ndc.w, p.y),
        0.0,
        1.0,
    );
    output.unit = p;
    return output;
}

fn distance_to_segment(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> f32 {
    let ab = b - a;
    let length_sq = max(dot(ab, ab), 1e-6);
    let t = clamp(dot(p - a, ab) / length_sq, 0.0, 1.0);
    let closest = a + ab * t;
    return distance(p, closest);
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let pixel = vec2<f32>(
        mix(u.bbox_min_px.x, u.bbox_max_px.x, input.unit.x),
        mix(u.bbox_min_px.y, u.bbox_max_px.y, input.unit.y),
    );
    let dist = distance_to_segment(pixel, u.start_px, u.end_px);
    let r = u.thickness.x;
    let alpha = 1.0 - smoothstep(r - 0.5, r + 0.5, dist);
    if alpha <= 0.0 {
        discard;
    }
    return vec4<f32>(u.color.rgb, u.color.a * alpha);
}
"#;

/// 線分 uniform バイト数。
/// レイアウト (WGSL std140):
///   vec4 rect_ndc        (offset 0,  16B)
///   vec2 bbox_min_px     (offset 16, 8B)
///   vec2 bbox_max_px     (offset 24, 8B)
///   vec2 start_px        (offset 32, 8B)
///   vec2 end_px          (offset 40, 8B)
///   vec4 thickness       (offset 48, 16B; .x のみ使用)
///   vec4 color           (offset 64, 16B)
/// 合計 80 バイト。
pub(super) const LINE_QUAD_UNIFORM_SIZE: u64 = std::mem::size_of::<[f32; 20]>() as u64;
