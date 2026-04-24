//! `wgpu` を使って UI ベースフレーム・GPU キャンバス・オーバーレイを提示する。
//!
//! 毎フレームの描画手順:
//!   1. CPU 側のピクセルデータを GPU テクスチャへアップロード (queue.write_texture)
//!   2. ユニフォームバッファを更新して描画位置・UV を GPU へ伝える (queue.write_buffer)
//!   3. CommandEncoder でレンダーパスを記録し、draw コールを積む
//!   4. queue.submit でコマンドを GPU へ投入
//!   5. surface_texture.present で画面へ表示
//! ```

use anyhow::{Context, Result};
use desktop_support::APP_BACKGROUND;
use desktop_support::PresentTimings;
use render::RenderFrame;
use std::sync::Arc;
use std::time::{Duration, Instant};
use winit::dpi::PhysicalSize;
use winit::window::Window;

use crate::frame::{Rect, TextureQuad};

/// CPU 側のピクセルデータへの参照を保持する軽量ビュー。
/// GPU へアップロードする直前にこの形で渡す。
#[derive(Debug, Clone, Copy)]
pub struct TextureSource<'a> {
    pub width: u32,
    pub height: u32,
    /// RGBA 各 1 バイト = 1 ピクセル 4 バイトのフラットなバイト列。
    pub pixels: &'a [u8],
}

impl<'a> From<&'a RenderFrame> for TextureSource<'a> {
    fn from(frame: &'a RenderFrame) -> Self {
        Self {
            width: frame.width as u32,
            height: frame.height as u32,
            pixels: frame.pixels.as_slice(),
        }
    }
}

/// GPU へ部分アップロードする矩形領域。
/// dirty rect 最適化のために使う（変化した領域だけを転送して帯域を節約する）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UploadRegion {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// フルスクリーンに貼るレイヤー（ベース / オーバーレイ / UI パネル）の転送仕様。
#[derive(Debug, Clone, Copy)]
pub struct FrameLayer<'a> {
    pub source: TextureSource<'a>,
    /// `None` なら今フレームはアップロードをスキップする（更新なし）。
    pub upload_region: Option<UploadRegion>,
}

/// キャンバスレイヤーのデータソース。CPU ビットマップか GPU テクスチャかを表す。
#[derive(Debug, Clone, Copy)]
pub enum CanvasLayerSource<'a> {
    /// CPU ビットマップから転送する通常パス。
    Cpu(TextureSource<'a>),
    /// GPU テクスチャを直接 Present する高速パス（`gpu` feature 有効時のみ）。
    #[cfg(feature = "gpu")]
    Gpu {
        panel_id: &'a str,
        layer_index: usize,
        width: u32,
        height: u32,
    },
}

impl<'a> CanvasLayerSource<'a> {
    fn width(self) -> u32 {
        match self {
            Self::Cpu(src) => src.width,
            #[cfg(feature = "gpu")]
            Self::Gpu { width, .. } => width,
        }
    }
    fn height(self) -> u32 {
        match self {
            Self::Cpu(src) => src.height,
            #[cfg(feature = "gpu")]
            Self::Gpu { height, .. } => height,
        }
    }
    fn cpu_source(self) -> Option<TextureSource<'a>> {
        match self {
            Self::Cpu(src) => Some(src),
            #[cfg(feature = "gpu")]
            Self::Gpu { .. } => None,
        }
    }
    #[cfg(feature = "gpu")]
    fn is_gpu(self) -> bool {
        matches!(self, Self::Gpu { .. })
    }
}

/// キャンバスレイヤーの転送仕様。
/// `quad` でスクリーン上の描画位置・UV・回転を指定できる点が FrameLayer と異なる。
#[derive(Debug, Clone, Copy)]
pub struct CanvasLayer<'a> {
    pub source: CanvasLayerSource<'a>,
    pub upload_region: Option<UploadRegion>,
    /// 描画先矩形・UV 範囲・回転・反転などのジオメトリ情報。
    pub quad: TextureQuad,
}

/// 1 フレームに必要な全レイヤーをまとめた描画シーン。
/// レイヤーは以下の順番で上から合成される:
///   L1 base_layer        … バックグラウンド（ドキュメント合成結果）
///   L2 canvas_layer      … キャンバス本体（None なら描画しない）
///   L3 temp_overlay_layer … ストローク中の一時オーバーレイ
///   L4 ui_panel_layer    … UI パネル群
#[derive(Debug, Clone, Copy)]
pub struct PresentScene<'a> {
    pub base_layer: FrameLayer<'a>,
    pub canvas_layer: Option<CanvasLayer<'a>>,
    pub temp_overlay_layer: FrameLayer<'a>,
    pub ui_panel_layer: FrameLayer<'a>,
}

/// WGSL（WebGPU Shading Language）で書かれた描画シェーダ。
///
/// # 全体の役割
/// 各レイヤーテクスチャを「四角形（クワッド）」として画面に貼る。
/// 頂点シェーダが 6 頂点（三角形 2 枚）の位置を計算し、
/// フラグメントシェーダが各ピクセルの色をテクスチャからサンプルして返す。
const PRESENT_SHADER: &str = r#"
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

/// ユニフォームバッファの可視ステージ（頂点・フラグメント両方から参照する）。
const LAYER_UNIFORM_VISIBILITY: wgpu::ShaderStages =
    wgpu::ShaderStages::VERTEX.union(wgpu::ShaderStages::FRAGMENT);

/// ユニフォームバッファのバイトサイズ。
/// `LayerUniform` は f32 × 16 = 64 バイト（vec2×4 + vec4×2 = 16 floats）。
const LAYER_UNIFORM_SIZE: u64 = std::mem::size_of::<[f32; 16]>() as u64;

/// GPU 側に確保した 1 レイヤー分のリソース群。
/// テクスチャ・バインドグループ・ユニフォームバッファをまとめて管理する。
#[derive(Debug)]
struct UploadedLayerTexture {
    /// GPU テクスチャ本体（ピクセルデータを格納する VRAM 領域）。
    texture: wgpu::Texture,
    /// シェーダへリソースをバインドする束。
    /// texture + sampler + uniform_buffer の 3 つをまとめて shader の @binding に結びつける。
    bind_group: wgpu::BindGroup,
    /// 描画位置・UV・回転などを GPU へ伝えるユニフォームバッファ。
    uniform_buffer: wgpu::Buffer,
    /// テクスチャの幅（ピクセル）。サイズ変更検知に使う。
    width: u32,
    /// テクスチャの高さ（ピクセル）。サイズ変更検知に使う。
    height: u32,
    /// `true` の場合は次回フルアップロードを強制する。
    /// テクスチャを新規作成した直後は中身が未初期化なのでフラグを立てておく。
    needs_full_upload: bool,
}

/// 各レイヤーのアップロード統計（パフォーマンス計測用）。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct LayerUploadStats {
    /// アップロードにかかった時間。
    duration: Duration,
    /// アップロードしたバイト数。
    bytes: u64,
}

/// GPU キャンバステクスチャ用のバインドグループキャッシュ。
///
/// `(panel_id, layer_index, width, height)` が変化したときのみ再生成する。
#[cfg(feature = "gpu")]
struct GpuBindGroupCache {
    bind_group: wgpu::BindGroup,
    uniform_buffer: wgpu::Buffer,
    panel_id: String,
    layer_index: usize,
    width: u32,
    height: u32,
}

/// wgpu を使って複数レイヤーをウィンドウへ合成・表示するプレゼンター。
///
/// wgpu の主要オブジェクトと各レイヤーのテクスチャを保持する。
pub struct WgpuPresenter {
    /// ウィンドウへの描画先サーフェス。OS のスワップチェーンに対応する。
    surface: wgpu::Surface<'static>,
    /// 論理 GPU デバイス。テクスチャやバッファの生成・パイプラインの構築に使う。
    /// Arc でラップして gpu-canvas クレートと共有できるようにする。
    device: Arc<wgpu::Device>,
    /// コマンドキュー。エンコードしたコマンドを GPU へ提出する。
    /// Arc でラップして gpu-canvas クレートと共有できるようにする。
    queue: Arc<wgpu::Queue>,
    /// サーフェス設定（解像度・フォーマット・プレゼントモードなど）。
    config: wgpu::SurfaceConfiguration,
    /// レンダーパイプライン。頂点/フラグメントシェーダとブレンド設定をまとめたもの。
    pipeline: wgpu::RenderPipeline,
    /// テクスチャサンプラー。拡縮フィルタや端のクランプ処理を定義する。
    sampler: wgpu::Sampler,
    /// バインドグループレイアウト。シェーダが期待するバインディング構造を宣言する。
    bind_group_layout: wgpu::BindGroupLayout,
    // 各レイヤーの GPU リソース（None = 未初期化）
    base_layer: Option<UploadedLayerTexture>,
    canvas_layer: Option<UploadedLayerTexture>,
    temp_overlay_layer: Option<UploadedLayerTexture>,
    ui_panel_layer: Option<UploadedLayerTexture>,
    /// GPU キャンバステクスチャのバインドグループキャッシュ。
    #[cfg(feature = "gpu")]
    canvas_gpu_bind_group_cache: Option<GpuBindGroupCache>,
    /// Rgba8Unorm テクスチャを sRGB view で Present できるかどうか。
    /// `false` の場合は GPU キャンバスソースを使わず CPU パスへフォールバックする。
    #[cfg(feature = "gpu")]
    srgb_canvas_view_supported: bool,
}

/// サポートされているプレゼントモードの中から最も低レイテンシなものを選ぶ。
///
/// 優先順位: Mailbox（トリプルバッファリング、低レイテンシ）
///         → Immediate（vsync なし、最低レイテンシ、ティアリングあり）
///         → FifoRelaxed（遅延時はティアリング許容の vsync）
///         → Fifo（完全な vsync、確実にティアリングなし）
fn preferred_present_mode(modes: &[wgpu::PresentMode]) -> wgpu::PresentMode {
    [
        wgpu::PresentMode::Mailbox,
        wgpu::PresentMode::Immediate,
        wgpu::PresentMode::FifoRelaxed,
        wgpu::PresentMode::Fifo,
    ]
    .into_iter()
    .find(|mode| modes.contains(mode))
    .unwrap_or(wgpu::PresentMode::Fifo)
}

impl WgpuPresenter {
    /// wgpu の全リソースを初期化して `WgpuPresenter` を生成する。
    ///
    /// # 初期化ステップ
    /// 1. `Instance` 生成 → `Surface` 生成
    /// 2. `Adapter`（物理 GPU）取得
    /// 3. `Device` と `Queue` 取得
    /// 4. サーフェスのピクセルフォーマットとプレゼントモードを選択・設定
    /// 5. バインドグループレイアウト・サンプラー・シェーダ・パイプラインを生成
    pub async fn new(window: Arc<Window>) -> Result<Self> {
        let size = window.inner_size();

        // wgpu の最上位オブジェクト。バックエンド（Vulkan/Metal/DX12/WebGPU）の
        // 選択は Instance がデフォルトで行う。
        let instance = wgpu::Instance::default();

        // winit ウィンドウからサーフェスを作成。
        // サーフェスはスワップチェーン（画面に表示するフレームバッファ）の窓口。
        let surface = instance
            .create_surface(window)
            .context("failed to create surface")?;

        // このサーフェスで使える物理 GPU（アダプター）を非同期で要求する。
        // HighPerformance を指定して省電力 GPU ではなくゲーミング GPU を優先する。
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface), // surface と互換性のある GPU を選ぶ
                force_fallback_adapter: false,      // ソフトウェアレンダラーは使わない
            })
            .await
            .context("failed to acquire adapter")?;

        // 物理 GPU（adapter）から論理デバイスとコマンドキューを取得。
        // device: リソース生成・パイプライン構築に使う。
        // queue: GPU へ命令を投入するための FIFO キュー。
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("altpaint-device"),
                required_features: wgpu::Features::empty(), // 特別な GPU 機能は不要
                experimental_features: Default::default(),
                required_limits: adapter.limits(), // アダプターのデフォルト制限を引き継ぐ
                memory_hints: wgpu::MemoryHints::Performance, // パフォーマンス優先の VRAM 配置
                trace: wgpu::Trace::default(),
            })
            .await
            .context("failed to create device")?;
        // gpu-canvas クレートと共有できるよう Arc でラップする。
        let device = Arc::new(device);
        let queue = Arc::new(queue);

        #[cfg(feature = "gpu")]
        let srgb_canvas_view_supported =
            gpu_canvas::format_check::supports_rgba8unorm_storage(&adapter);

        // サーフェスがサポートするピクセルフォーマットの一覧を取得する。
        let surface_capabilities = surface.get_capabilities(&adapter);

        // sRGB 対応フォーマットを優先して選ぶ（なければ先頭を使う）。
        // sRGB フォーマットを選ぶと GPU がガンマ補正を自動で行う。
        let surface_format = surface_capabilities
            .formats
            .iter()
            .copied()
            .find(|format| format.is_srgb())
            .unwrap_or(surface_capabilities.formats[0]);

        let present_mode = preferred_present_mode(surface_capabilities.present_modes.as_slice());

        // サーフェスの設定を組み立てる。
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT, // レンダリング出力先として使う
            format: surface_format,
            width: size.width.max(1), // 0 は無効なので最低 1
            height: size.height.max(1),
            present_mode,
            alpha_mode: surface_capabilities.alpha_modes[0], // アダプター推奨のアルファモード
            view_formats: Vec::new(),                        // デフォルトのビューフォーマットのみ
            desired_maximum_frame_latency: 2, // 2 にして CPU-GPU パイプラインを重複させ高フレームレートを実現
        };
        // 設定をサーフェスへ適用してスワップチェーンを初期化する。
        surface.configure(&device, &config);

        // ─── バインドグループレイアウト ────────────────────────────────────────
        // シェーダの @group(0) @binding(0/1/2) に何をバインドするかを宣言する。
        // 実際のリソース（テクスチャ・サンプラー・バッファ）はまだ紐付けない。
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("altpaint-present-bind-group-layout"),
            entries: &[
                // binding(0): フラグメントシェーダ用の 2D テクスチャ。
                // filterable = true にしないとバイリニアサンプラーと組み合わせられない。
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false, // マルチサンプリング（MSAA）は使わない
                    },
                    count: None, // テクスチャ配列ではない
                },
                // binding(1): フラグメントシェーダ用のサンプラー（Filtering 対応）。
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                // binding(2): 頂点・フラグメント両方から読む小さなユニフォームバッファ。
                // dynamic_offset = false なので常にバッファの先頭を使う。
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: LAYER_UNIFORM_VISIBILITY,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        // ─── サンプラー ────────────────────────────────────────────────────────
        // テクスチャを拡大・縮小するときのフィルタリング方法を定義する。
        // Linear = バイリニアフィルタ（滑らかにぼかす）。
        // ClampToEdge = テクスチャ外の UV を端のピクセル色でクランプする。
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("altpaint-present-sampler"),
            mag_filter: wgpu::FilterMode::Linear, // 拡大時: バイリニア
            min_filter: wgpu::FilterMode::Linear, // 縮小時: バイリニア
            mipmap_filter: wgpu::MipmapFilterMode::Linear, // ミップマップ間: バイリニア
            address_mode_u: wgpu::AddressMode::ClampToEdge, // U 方向（横）: 端でクランプ
            address_mode_v: wgpu::AddressMode::ClampToEdge, // V 方向（縦）: 端でクランプ
            ..Default::default()
        });

        // ─── シェーダモジュール ────────────────────────────────────────────────
        // WGSL ソースをコンパイルして GPU 上で実行可能なシェーダオブジェクトを作る。
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("altpaint-present-shader"),
            source: wgpu::ShaderSource::Wgsl(PRESENT_SHADER.into()),
        });

        // ─── パイプラインレイアウト ────────────────────────────────────────────
        // どのバインドグループレイアウトをパイプラインが使うかを宣言する。
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("altpaint-present-pipeline-layout"),
            bind_group_layouts: &[&bind_group_layout], // @group(0) に対応
            immediate_size: 0,                         // プッシュ定数は使わない
        });

        // ─── レンダーパイプライン ──────────────────────────────────────────────
        // 頂点シェーダ → ラスタライズ → フラグメントシェーダ の処理チェーンを定義する。
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("altpaint-present-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"), // WGSL 内の @vertex fn 名
                buffers: &[],                 // 頂点バッファなし（vertex_index だけで座標生成）
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"), // WGSL 内の @fragment fn 名
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    // ALPHA_BLENDING: src.rgb * src.a + dst.rgb * (1 - src.a)
                    // 半透明レイヤーを正しく重ねるために必要。
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL, // RGBA すべて書き込む
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState::default(), // 三角形リスト、カリングなし
            depth_stencil: None,                        // 深度バッファは使わない（2D なので不要）
            multisample: wgpu::MultisampleState::default(), // MSAA なし（1 サンプル）
            multiview_mask: None,                       // VR 多視点レンダリングは不使用
            cache: None,
        });

        Ok(Self {
            surface,
            device,
            queue,
            config,
            pipeline,
            sampler,
            bind_group_layout,
            base_layer: None, // 初回フレームで ensure_layer_texture が生成する
            canvas_layer: None,
            temp_overlay_layer: None,
            ui_panel_layer: None,
            #[cfg(feature = "gpu")]
            canvas_gpu_bind_group_cache: None,
            #[cfg(feature = "gpu")]
            srgb_canvas_view_supported,
        })
    }

    /// ウィンドウサイズ変更時にサーフェスを再設定する。
    /// Arc でラップされたデバイスへの参照を返す。
    ///
    /// gpu-canvas クレートの `GpuCanvasPool` / `GpuPenTipCache` と共有するために使う。
    pub fn device(&self) -> Arc<wgpu::Device> {
        Arc::clone(&self.device)
    }

    /// Arc でラップされたキューへの参照を返す。
    ///
    /// gpu-canvas クレートの `GpuCanvasPool` / `GpuPenTipCache` と共有するために使う。
    pub fn queue(&self) -> Arc<wgpu::Queue> {
        Arc::clone(&self.queue)
    }

    /// Rgba8Unorm テクスチャを sRGB view で Present できるかどうかを返す。
    #[cfg(feature = "gpu")]
    pub fn srgb_canvas_view_supported(&self) -> bool {
        self.srgb_canvas_view_supported
    }

    ///
    /// サーフェスの幅・高さを更新して `configure` を再呼び出しする。
    /// 0×0 は無効サイズなので何もしない（最小化時など）。
    pub fn resize(&mut self, size: PhysicalSize<u32>) {
        if size.width == 0 || size.height == 0 {
            return;
        }

        self.config.width = size.width;
        self.config.height = size.height;
        // configure を呼ぶことでスワップチェーンが新サイズで再作成される。
        self.surface.configure(&self.device, &self.config);
    }

    /// 1 フレーム分を GPU へアップロードして画面に表示する。
    ///
    /// # 内部処理の順序
    /// 1. 各レイヤーのテクスチャが適切なサイズで存在するか確認・再生成
    /// 2. 変更のあったレイヤーを GPU へアップロード
    /// 3. 各レイヤーのユニフォームバッファ（描画位置・UV）を更新
    /// 4. スワップチェーンから次フレーム用テクスチャを取得
    /// 5. レンダーパスを開始して全レイヤーを順番に描画
    /// 6. コマンドを submit して GPU へ投入、present で画面表示
    pub fn render(
        &mut self,
        scene: PresentScene<'_>,
        #[cfg(feature = "gpu")] gpu_canvas_pool: Option<&gpu_canvas::GpuCanvasPool>,
    ) -> Result<PresentTimings> {
        // サーフェスが 0 サイズなら描画をスキップ（最小化時など）。
        if self.config.width == 0 || self.config.height == 0 {
            return Ok(PresentTimings::default());
        }

        // ─── ステップ 1: テクスチャの確保 ────────────────────────────────────
        // 各レイヤーについて GPU テクスチャが存在しなければ（または解像度が変わっていれば）
        // 作り直す。テクスチャ生成はコストが高いので必要なときだけ行う。
        Self::ensure_layer_texture(
            &self.device,
            &self.sampler,
            &self.bind_group_layout,
            &mut self.base_layer,
            scene.base_layer.source.width,
            scene.base_layer.source.height,
            "base",
        );
        Self::ensure_layer_texture(
            &self.device,
            &self.sampler,
            &self.bind_group_layout,
            &mut self.temp_overlay_layer,
            scene.temp_overlay_layer.source.width,
            scene.temp_overlay_layer.source.height,
            "temp-overlay",
        );
        Self::ensure_layer_texture(
            &self.device,
            &self.sampler,
            &self.bind_group_layout,
            &mut self.ui_panel_layer,
            scene.ui_panel_layer.source.width,
            scene.ui_panel_layer.source.height,
            "ui-panel",
        );
        // canvas_layer は省略可能。GPU ソース時は ensure をスキップ（gpu-canvas プール管理）。
        if let Some(canvas_layer) = scene.canvas_layer.filter(|c| c.source.cpu_source().is_some()) {
            Self::ensure_layer_texture(
                &self.device,
                &self.sampler,
                &self.bind_group_layout,
                &mut self.canvas_layer,
                canvas_layer.source.width(),
                canvas_layer.source.height(),
                "canvas",
            );
        }

        // ─── ステップ 2: CPU→GPU テクスチャアップロード ──────────────────────
        // upload_region が Some なら dirty rect 範囲だけ転送し、None ならスキップする。
        // needs_full_upload フラグが立っている場合はフルアップロードが優先される。
        let upload_started = Instant::now();
        let base_upload = Self::upload_layer(
            &self.queue,
            self.base_layer.as_mut(),
            scene.base_layer.source,
            scene.base_layer.upload_region,
        );
        let temp_overlay_upload = Self::upload_layer(
            &self.queue,
            self.temp_overlay_layer.as_mut(),
            scene.temp_overlay_layer.source,
            scene.temp_overlay_layer.upload_region,
        );
        let ui_panel_upload = Self::upload_layer(
            &self.queue,
            self.ui_panel_layer.as_mut(),
            scene.ui_panel_layer.source,
            scene.ui_panel_layer.upload_region,
        );
        let canvas_upload = if let Some(canvas_layer) = scene.canvas_layer {
            if let Some(cpu_src) = canvas_layer.source.cpu_source() {
                Self::upload_layer(
                    &self.queue,
                    self.canvas_layer.as_mut(),
                    cpu_src,
                    canvas_layer.upload_region,
                )
            } else {
                LayerUploadStats::default()
            }
        } else {
            LayerUploadStats::default()
        };

        // ─── ステップ 3: ユニフォームバッファ更新 ────────────────────────────
        // ユニフォームバッファに描画先矩形（NDC）・UV 範囲・回転などを書き込む。
        // base / temp_overlay / ui_panel はフルスクリーンクワッドを使う。
        Self::update_quad_uniform(
            &self.queue,
            self.base_layer.as_ref(),
            fullscreen_quad(self.config.width, self.config.height),
            self.config.width,
            self.config.height,
        );
        Self::update_quad_uniform(
            &self.queue,
            self.temp_overlay_layer.as_ref(),
            fullscreen_quad(self.config.width, self.config.height),
            self.config.width,
            self.config.height,
        );
        Self::update_quad_uniform(
            &self.queue,
            self.ui_panel_layer.as_ref(),
            fullscreen_quad(self.config.width, self.config.height),
            self.config.width,
            self.config.height,
        );
        // canvas_layer は quad で位置・回転・スケールが指定される。
        if let Some(canvas_layer) = scene.canvas_layer {
            #[cfg(feature = "gpu")]
            if canvas_layer.source.is_gpu() {
                self.update_gpu_canvas_bind_group(
                    canvas_layer.source,
                    canvas_layer.quad,
                    gpu_canvas_pool,
                    self.config.width,
                    self.config.height,
                );
            } else {
                Self::update_quad_uniform(
                    &self.queue,
                    self.canvas_layer.as_ref(),
                    canvas_layer.quad,
                    self.config.width,
                    self.config.height,
                );
            }
            #[cfg(not(feature = "gpu"))]
            Self::update_quad_uniform(
                &self.queue,
                self.canvas_layer.as_ref(),
                canvas_layer.quad,
                self.config.width,
                self.config.height,
            );
        }
        let upload = upload_started.elapsed();

        // ─── ステップ 4: スワップチェーンから次フレームテクスチャを取得 ──────
        // get_current_texture は GPU が次に表示するバックバッファを返す。
        // Lost/Outdated はウィンドウのリサイズや復帰時に起きるため、
        // サーフェスを再設定してもう一度取得を試みる。
        let surface_texture = match self.surface.get_current_texture() {
            Ok(surface_texture) => surface_texture,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                // サーフェスが無効化されたので再設定してから再取得する。
                self.surface.configure(&self.device, &self.config);
                self.surface
                    .get_current_texture()
                    .context("failed to acquire surface texture after reconfigure")?
            }
            Err(wgpu::SurfaceError::Timeout) => {
                // タイムアウト: このフレームは表示をスキップして次フレームへ。
                return Ok(PresentTimings {
                    upload,
                    base_upload: base_upload.duration,
                    temp_overlay_upload: temp_overlay_upload.duration,
                    canvas_upload: canvas_upload.duration,
                    ui_panel_upload: ui_panel_upload.duration,
                    base_upload_bytes: base_upload.bytes,
                    temp_overlay_upload_bytes: temp_overlay_upload.bytes,
                    canvas_upload_bytes: canvas_upload.bytes,
                    ui_panel_upload_bytes: ui_panel_upload.bytes,
                    ..Default::default()
                });
            }
            Err(wgpu::SurfaceError::OutOfMemory) => {
                anyhow::bail!("out of memory while acquiring surface texture")
            }
            Err(other) => return Err(other).context("failed to acquire surface texture"),
        };

        // ─── ステップ 5: コマンドのエンコード ────────────────────────────────
        // TextureView: テクスチャをレンダーターゲットとして参照するためのビュー。
        let view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        // CommandEncoder: GPU コマンドを記録するレコーダー。
        // ここではまだ GPU は何も実行しない（記録だけ）。
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("altpaint-present-encoder"),
            });

        let encode_started = Instant::now();
        {
            // RenderPass を開始する。begin_render_pass の時点で「クリア」が指定される。
            // スコープを抜けると pass が drop されレンダーパスが終了する。
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("altpaint-present-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,          // 描画先テクスチャビュー
                    resolve_target: None, // MSAA 解決先なし
                    depth_slice: None,
                    ops: wgpu::Operations {
                        // Clear: レンダーパス開始時にアプリ背景色でクリアする。
                        // APP_BACKGROUND は [u8; 4] なので 255 で割って f64 へ。
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: APP_BACKGROUND[0] as f64 / 255.0,
                            g: APP_BACKGROUND[1] as f64 / 255.0,
                            b: APP_BACKGROUND[2] as f64 / 255.0,
                            a: APP_BACKGROUND[3] as f64 / 255.0,
                        }),
                        store: wgpu::StoreOp::Store, // 描画結果を保持してサーフェスへ
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
                multiview_mask: None,
            });

            // レンダーパイプラインをセット（この後の draw コールに使うシェーダを指定）。
            pass.set_pipeline(&self.pipeline);

            // レイヤーを下から順番に描画（後に描くほど手前に表示される）。
            Self::draw_layer(&mut pass, self.base_layer.as_ref()); // L1 背景
            if let Some(canvas_layer) = scene.canvas_layer {
                #[cfg(feature = "gpu")]
                if canvas_layer.source.is_gpu() {
                    if let Some(cache) = &self.canvas_gpu_bind_group_cache {
                        pass.set_bind_group(0, &cache.bind_group, &[]);
                        pass.draw(0..6, 0..1);
                    }
                } else {
                    Self::draw_layer(&mut pass, self.canvas_layer.as_ref());
                }
                #[cfg(not(feature = "gpu"))]
                {
                    let _ = canvas_layer;
                    Self::draw_layer(&mut pass, self.canvas_layer.as_ref());
                }
            }
            Self::draw_layer(&mut pass, self.temp_overlay_layer.as_ref()); // L3 ストロークオーバーレイ
            Self::draw_layer(&mut pass, self.ui_panel_layer.as_ref()); // L4 UI パネル
        } // ← ここで pass が drop され、レンダーパス終了コマンドが記録される

        // ─── ステップ 6: submit → present ────────────────────────────────────
        // encoder.finish() でコマンドバッファを確定し、queue.submit で GPU へ投入する。
        // submit 後は GPU が非同期でコマンドを実行し始める。
        self.queue.submit([encoder.finish()]);
        let encode_and_submit = encode_started.elapsed();

        // surface_texture.present() でスワップチェーンにフレームを提出する。
        // GPU がレンダリングを終えたタイミングでモニターへ表示される。
        let present_started = Instant::now();
        surface_texture.present();

        Ok(PresentTimings {
            upload,
            encode_and_submit,
            present: present_started.elapsed(),
            base_upload: base_upload.duration,
            temp_overlay_upload: temp_overlay_upload.duration,
            canvas_upload: canvas_upload.duration,
            ui_panel_upload: ui_panel_upload.duration,
            base_upload_bytes: base_upload.bytes,
            temp_overlay_upload_bytes: temp_overlay_upload.bytes,
            canvas_upload_bytes: canvas_upload.bytes,
            ui_panel_upload_bytes: ui_panel_upload.bytes,
        })
    }

    /// `slot` に適切なサイズのテクスチャが入っていなければ生成し直す。
    ///
    /// テクスチャのサイズはフレームごとに変わりうる（ウィンドウリサイズなど）。
    /// サイズが一致している場合は何もしない（コストゼロ）。
    fn ensure_layer_texture(
        device: &wgpu::Device,
        sampler: &wgpu::Sampler,
        bind_group_layout: &wgpu::BindGroupLayout,
        slot: &mut Option<UploadedLayerTexture>,
        width: u32,
        height: u32,
        label: &str,
    ) {
        // 既存テクスチャのサイズと要求サイズが一致していれば何もしない。
        let needs_rebuild = slot
            .as_ref()
            .is_none_or(|layer| layer.width != width || layer.height != height);
        if !needs_rebuild {
            return;
        }

        // ─── GPU テクスチャ生成 ───────────────────────────────────────────────
        // RGBA8 sRGB フォーマット（1 ピクセル 4 バイト）の 2D テクスチャ。
        // TEXTURE_BINDING: シェーダから読み取れるようにする。
        // COPY_DST: queue.write_texture でデータを書き込めるようにする。
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(&format!("altpaint-{label}-texture")),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1, // 2D なので深さ 1
            },
            mip_level_count: 1, // ミップマップなし（1 解像度のみ）
            sample_count: 1,    // MSAA なし
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb, // 8bit RGBA + sRGB ガンマ
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        // ─── ユニフォームバッファ生成 ─────────────────────────────────────────
        // 小さな定数バッファ（64 バイト）。毎フレーム write_buffer で更新する。
        // UNIFORM: シェーダの uniform 変数として使う。
        // COPY_DST: CPU からデータを書き込める。
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(&format!("altpaint-{label}-uniform")),
            size: LAYER_UNIFORM_SIZE,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false, // 初期データなし
        });

        // テクスチャをバインドグループに登録するためのビューを作る。
        // TextureView はテクスチャへのアクセス経路（フォーマット・レイヤー範囲など）を定義する。
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        // ─── バインドグループ生成 ─────────────────────────────────────────────
        // シェーダの @binding に実際のリソースを紐付ける。
        // これにより GPU は draw コール時にどのテクスチャ・サンプラー・バッファを使うか分かる。
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(&format!("altpaint-{label}-bind-group")),
            layout: bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view), // @binding(0) にテクスチャ
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler), // @binding(1) にサンプラー
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: uniform_buffer.as_entire_binding(), // @binding(2) にバッファ全体
                },
            ],
        });

        // slot を新しく作ったリソースで置き換える。
        // needs_full_upload = true にしてテクスチャの中身を初回フルアップロードさせる。
        *slot = Some(UploadedLayerTexture {
            texture,
            bind_group,
            uniform_buffer,
            width,
            height,
            needs_full_upload: true, // 新規テクスチャは未初期化なのでフルアップロード必須
        });
    }

    /// CPU ピクセルデータを GPU テクスチャへアップロードする。
    ///
    /// `needs_full_upload` が true ならテクスチャ全体を転送し、
    /// そうでなければ `upload_region` で指定された矩形だけを転送する。
    /// どちらも指定なければスキップしてゼロ統計を返す。
    fn upload_layer(
        queue: &wgpu::Queue,
        layer: Option<&mut UploadedLayerTexture>,
        source: TextureSource<'_>,
        upload_region: Option<UploadRegion>,
    ) -> LayerUploadStats {
        let Some(layer) = layer else {
            return LayerUploadStats::default(); // テクスチャが未初期化なら何もしない
        };

        let started = Instant::now();

        if layer.needs_full_upload {
            // テクスチャを新規作成した直後は中身が未定義なので全面アップロードが必要。
            let bytes = upload_full_texture(queue, layer, source);
            layer.needs_full_upload = false; // 次回からは差分のみで OK
            return LayerUploadStats {
                duration: started.elapsed(),
                bytes,
            };
        }

        if let Some(region) = upload_region {
            // dirty rect 最適化: 変化した矩形だけを転送する。
            let bytes = upload_texture_region(queue, layer, source, region);
            return LayerUploadStats {
                duration: started.elapsed(),
                bytes,
            };
        }

        // upload_region も needs_full_upload もなければ何もしない（更新なし）。
        LayerUploadStats::default()
    }

    /// ユニフォームバッファに `quad` の NDC 座標・UV 範囲・回転などを書き込む。
    ///
    /// `queue.write_buffer` は CPU→GPU の即時転送（サブミット前に反映される）。
    fn update_quad_uniform(
        queue: &wgpu::Queue,
        layer: Option<&UploadedLayerTexture>,
        quad: TextureQuad,
        surface_width: u32,
        surface_height: u32,
    ) {
        let Some(layer) = layer else {
            return; // テクスチャが未初期化なら更新不要
        };
        // quad の情報を f32 16 個 = 64 バイトに詰めてバッファへ書き込む。
        queue.write_buffer(
            &layer.uniform_buffer,
            0, // バッファの先頭から書く
            &quad_uniform_bytes(quad, surface_width, surface_height),
        );
    }

    /// レンダーパスにバインドグループをセットして draw を呼ぶ。
    ///
    /// draw(0..6, 0..1):
    ///   - 頂点インデックス 0〜5 の 6 頂点（= 三角形 2 枚の四角形）
    ///   - インスタンス 0 の 1 インスタンスのみ描画
    fn draw_layer<'a>(pass: &mut wgpu::RenderPass<'a>, layer: Option<&'a UploadedLayerTexture>) {
        let Some(layer) = layer else {
            return; // テクスチャが未初期化なら描画しない
        };
        // バインドグループ @group(0) にこのレイヤーのテクスチャ・サンプラー・バッファを設定。
        pass.set_bind_group(0, &layer.bind_group, &[]);
        // 頂点バッファなしで 6 頂点を描画（vs_main が vertex_index だけで座標を計算する）。
        pass.draw(0..6, 0..1);
    }

    /// GPU キャンバステクスチャのバインドグループを作成/更新してキャッシュに保存する。
    ///
    /// `(panel_id, layer_index, width, height)` が前回と変わった場合のみ再生成する。
    #[cfg(feature = "gpu")]
    fn update_gpu_canvas_bind_group(
        &mut self,
        source: CanvasLayerSource<'_>,
        quad: TextureQuad,
        pool: Option<&gpu_canvas::GpuCanvasPool>,
        surface_width: u32,
        surface_height: u32,
    ) {
        let CanvasLayerSource::Gpu { panel_id, layer_index, width, height } = source else {
            return;
        };
        let Some(pool) = pool else {
            return;
        };
        let needs_rebuild = self.canvas_gpu_bind_group_cache.as_ref().is_none_or(|c| {
            c.panel_id != panel_id
                || c.layer_index != layer_index
                || c.width != width
                || c.height != height
        });
        if needs_rebuild {
            let Some(view) = pool.get_view(panel_id, layer_index) else {
                return;
            };
            let uniform_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("altpaint-canvas-gpu-uniform"),
                size: LAYER_UNIFORM_SIZE,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("altpaint-canvas-gpu-bind-group"),
                layout: &self.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: uniform_buffer.as_entire_binding(),
                    },
                ],
            });
            self.canvas_gpu_bind_group_cache = Some(GpuBindGroupCache {
                bind_group,
                uniform_buffer,
                panel_id: panel_id.to_string(),
                layer_index,
                width,
                height,
            });
        }
        if let Some(cache) = &self.canvas_gpu_bind_group_cache {
            self.queue.write_buffer(
                &cache.uniform_buffer,
                0,
                &quad_uniform_bytes(quad, surface_width, surface_height),
            );
        }
    }
}

/// テクスチャ全体を GPU へアップロードする。
///
/// `queue.write_texture` は CPU ピクセルバイト列を GPU テクスチャへ直接コピーする。
/// 内部的にはステージングバッファを経由して非同期転送が行われる。
fn upload_full_texture(
    queue: &wgpu::Queue,
    layer: &UploadedLayerTexture,
    source: TextureSource<'_>,
) -> u64 {
    if source.width == 0 || source.height == 0 {
        return 0;
    }

    queue.write_texture(
        // 書き込み先テクスチャの情報。
        wgpu::TexelCopyTextureInfo {
            texture: &layer.texture,
            mip_level: 0,                     // ミップレベル 0（最高解像度）
            origin: wgpu::Origin3d::ZERO,     // テクスチャ左上(0,0,0)から
            aspect: wgpu::TextureAspect::All, // 色・深度・ステンシル全て（2D カラーなら All でよい）
        },
        source.pixels, // アップロードするピクセルバイト列（RGBA 各 1 バイト）
        // ソースバイト列のレイアウト。GPU はこの情報で行の区切りを判断する。
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(source.width * 4), // 1 行のバイト数 = 横幅 × 4(RGBA)
            rows_per_image: Some(source.height),   // 画像の行数（3D テクスチャ用だが 2D でも指定）
        },
        // アップロードする領域のサイズ。
        wgpu::Extent3d {
            width: source.width,
            height: source.height,
            depth_or_array_layers: 1, // 2D なので深さ 1
        },
    );

    // アップロードしたバイト数を返す（計測用）。
    (source.width as u64) * (source.height as u64) * 4
}

/// テクスチャの部分領域だけを GPU へアップロードする（dirty rect 最適化）。
///
/// `write_texture` の `bytes_per_row` にソース行の全幅ストライドを渡すことで、
/// scratch バッファへの行詰め直しを省略し CPU メモリコピーをゼロにする。
/// wgpu 内部のステージングバッファへの転送は `bytes_per_row` ストライドで行われる。
fn upload_texture_region(
    queue: &wgpu::Queue,
    layer: &UploadedLayerTexture,
    source: TextureSource<'_>,
    region: UploadRegion,
) -> u64 {
    if region.width == 0 || region.height == 0 || source.width == 0 || source.height == 0 {
        return 0;
    }

    // region がソース画像の外に出ていた場合はクランプして実際にコピーできる範囲を求める。
    let max_width = source.width.saturating_sub(region.x);
    let max_height = source.height.saturating_sub(region.y);
    let copy_width = region.width.min(max_width);
    let copy_height = region.height.min(max_height);
    if copy_width == 0 || copy_height == 0 {
        return 0;
    }

    // ソースバッファの (region.y, region.x) からの先頭オフセット。
    // bytes_per_row に source.width * 4 を渡すことで、
    // wgpu 内部が各行の読み取りオフセットを自動計算して region だけを転送する。
    let start_offset = (region.y as usize * source.width as usize + region.x as usize) * 4;

    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &layer.texture,
            mip_level: 0,
            origin: wgpu::Origin3d {
                x: region.x, // テクスチャ上の書き込み開始 X（ピクセル単位）
                y: region.y, // テクスチャ上の書き込み開始 Y
                z: 0,
            },
            aspect: wgpu::TextureAspect::All,
        },
        &source.pixels[start_offset..], // scratch コピー不要: ソースを直接渡す
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(source.width * 4), // ソース行の全幅ストライド
            rows_per_image: Some(copy_height),
        },
        wgpu::Extent3d {
            width: copy_width,
            height: copy_height,
            depth_or_array_layers: 1,
        },
    );

    (copy_width as u64) * (copy_height as u64) * 4
}

/// ウィンドウ全体を覆うフルスクリーンクワッドを作る。
///
/// destination は左上 (0,0) から (width, height) までのピクセル矩形。
/// uv_min/uv_max は (0,0)〜(1,1) でテクスチャ全体を使う。
fn fullscreen_quad(width: u32, height: u32) -> TextureQuad {
    TextureQuad {
        destination: Rect {
            x: 0,
            y: 0,
            width: width as usize,
            height: height as usize,
        },
        uv_min: [0.0, 0.0], // テクスチャ左上
        uv_max: [1.0, 1.0], // テクスチャ右下
        rotation_degrees: 0.0,
        bbox_size: [width as f32, height as f32],
        flip_x: false,
        flip_y: false,
    }
}

/// `TextureQuad` の情報をシェーダが受け取れる 64 バイトのユニフォームデータに変換する。
///
/// # NDC 変換
/// ピクセル座標 px を NDC 座標に変換する式:
///   ndc_x = px / surface_width  * 2.0 - 1.0  (左 = -1, 右 = +1)
///   ndc_y = 1.0 - px / surface_height * 2.0  (上 = +1, 下 = -1; Y 反転)
///
/// # バッファレイアウト（f32 × 16 = 64 バイト）
/// ```text
/// [0]  rect_min.x (NDC left)
/// [1]  rect_min.y (NDC top)     ← wgpu Y: 上が +1
/// [2]  rect_max.x (NDC right)
/// [3]  rect_max.y (NDC bottom)
/// [4]  uv_min.x
/// [5]  uv_min.y
/// [6]  uv_max.x
/// [7]  uv_max.y
/// [8]  transform.x = rotation_degrees
/// [9]  transform.y = flip_x (0.0 or 1.0)
/// [10] transform.z = flip_y (0.0 or 1.0)
/// [11] transform.w = 0 (未使用)
/// [12] metrics.x = bbox_size.x (px)
/// [13] metrics.y = bbox_size.y (px)
/// [14] metrics.z = 0 (未使用)
/// [15] metrics.w = 0 (未使用)
/// ```
fn quad_uniform_bytes(quad: TextureQuad, surface_width: u32, surface_height: u32) -> [u8; 64] {
    let surface_width = surface_width.max(1) as f32;
    let surface_height = surface_height.max(1) as f32;

    // ピクセル座標 → NDC 座標 への変換。
    // wgpu の NDC は Y 上向きなのでピクセル Y を反転させる（1.0 - ...）。
    let left = quad.destination.x as f32 / surface_width * 2.0 - 1.0;
    let top = 1.0 - quad.destination.y as f32 / surface_height * 2.0;
    let right = (quad.destination.x + quad.destination.width) as f32 / surface_width * 2.0 - 1.0;
    let bottom = 1.0 - (quad.destination.y + quad.destination.height) as f32 / surface_height * 2.0;

    let values = [
        left,
        top,
        right,
        bottom, // rect_min / rect_max (NDC)
        quad.uv_min[0],
        quad.uv_min[1], // uv_min
        quad.uv_max[0],
        quad.uv_max[1],                      // uv_max
        quad.rotation_degrees,               // transform.x: 回転角度(度)
        if quad.flip_x { 1.0 } else { 0.0 }, // transform.y: 左右反転フラグ
        if quad.flip_y { 1.0 } else { 0.0 }, // transform.z: 上下反転フラグ
        0.0,                                 // transform.w: 未使用パディング
        quad.bbox_size[0],                   // metrics.x: バウンディングボックス幅(px)
        quad.bbox_size[1],                   // metrics.y: バウンディングボックス高さ(px)
        0.0,                                 // metrics.z: 未使用パディング
        0.0,                                 // metrics.w: 未使用パディング
    ];

    // f32 の配列をリトルエンディアンのバイト列に変換してバッファへ詰める。
    let mut bytes = [0u8; 64];
    for (index, value) in values.into_iter().enumerate() {
        bytes[index * 4..index * 4 + 4].copy_from_slice(&value.to_le_bytes());
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preferred_present_mode_prefers_low_latency_modes() {
        let mode = preferred_present_mode(&[wgpu::PresentMode::Fifo, wgpu::PresentMode::Immediate]);

        assert_eq!(mode, wgpu::PresentMode::Immediate);
    }

    #[test]
    fn preferred_present_mode_uses_mailbox_when_available() {
        let mode = preferred_present_mode(&[
            wgpu::PresentMode::Fifo,
            wgpu::PresentMode::Mailbox,
            wgpu::PresentMode::Immediate,
        ]);

        assert_eq!(mode, wgpu::PresentMode::Mailbox);
    }

    #[test]
    fn presenter_shader_mentions_uniform_quad_mapping() {
        assert!(PRESENT_SHADER.contains("LayerUniform"));
        assert!(PRESENT_SHADER.contains("rect_min"));
        assert!(PRESENT_SHADER.contains("textureSample"));
    }

    #[test]
    fn layer_uniform_visibility_covers_vertex_and_fragment_stages() {
        assert!(LAYER_UNIFORM_VISIBILITY.contains(wgpu::ShaderStages::VERTEX));
        assert!(LAYER_UNIFORM_VISIBILITY.contains(wgpu::ShaderStages::FRAGMENT));
    }

    #[test]
    fn quad_uniform_bytes_maps_fullscreen_quad_to_ndc() {
        let bytes = quad_uniform_bytes(fullscreen_quad(640, 480), 640, 480);
        let mut values = [0.0f32; 16];
        for (index, chunk) in bytes.chunks_exact(4).enumerate() {
            values[index] = f32::from_le_bytes(chunk.try_into().expect("chunk size"));
        }

        assert_eq!(values[0], -1.0); // NDC left
        assert_eq!(values[1], 1.0); // NDC top
        assert_eq!(values[2], 1.0); // NDC right
        assert_eq!(values[3], -1.0); // NDC bottom
        assert_eq!(values[8], 0.0); // rotation_degrees
        assert_eq!(values[9], 0.0); // flip_x
        assert_eq!(values[10], 0.0); // flip_y
        assert_eq!(values[11], 0.0); // padding
        assert_eq!(values[12], 640.0); // bbox_size.x
        assert_eq!(values[13], 480.0); // bbox_size.y
    }

    #[test]
    fn quad_uniform_buffer_size_matches_written_bytes() {
        assert_eq!(
            LAYER_UNIFORM_SIZE as usize,
            quad_uniform_bytes(fullscreen_quad(1, 1), 1, 1).len()
        );
    }
}
