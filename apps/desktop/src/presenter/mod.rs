//! `wgpu` を使って UI ベースフレーム・GPU キャンバス・オーバーレイを提示する (D2 / BL-116)。
//!
//! 旧 `wgpu_canvas.rs` (2200 行超) を責務別モジュールへ分割したもの:
//!   - `shaders`:   WGSL シェーダソースと uniform サイズ定数
//!   - `pipelines`: テクスチャ提示パイプライン + 共通 quad パイプライン (solid/circle/line 統合)
//!   - `textures`:  CPU/GPU テクスチャ確保・アップロード・bind group キャッシュ
//!   - `frame`:     `PresentFrame` 等の入力 DTO と `render` 本体
//!
//! 毎フレームの描画手順:
//!   1. CPU 側のピクセルデータを GPU テクスチャへアップロード (queue.write_texture)
//!   2. ユニフォームバッファを更新して描画位置・UV を GPU へ伝える (queue.write_buffer)
//!   3. CommandEncoder でレンダーパスを記録し、draw コールを積む
//!   4. queue.submit でコマンドを GPU へ投入
//!   5. surface_texture.present で画面へ表示

mod frame;
mod pipelines;
mod shaders;
mod textures;

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context, Result};
use winit::dpi::PhysicalSize;
use winit::window::Window;

use self::pipelines::{PresentPipeline, QuadPipeline};
use self::shaders::{
    CIRCLE_QUAD_SHADER, CIRCLE_QUAD_UNIFORM_SIZE, LINE_QUAD_SHADER, LINE_QUAD_UNIFORM_SIZE,
    SOLID_QUAD_SHADER, SOLID_QUAD_UNIFORM_SIZE,
};
use self::textures::{GpuBindGroupCache, PanelBindEntry, UploadedLayerTexture};

pub use self::frame::{
    CanvasSurface, CanvasSurfaceSource, GpuPanelQuad, PresentFrame, TextureSource, UploadRegion,
};

/// wgpu を使って複数レイヤーをウィンドウへ合成・表示するプレゼンター。
///
/// wgpu の主要オブジェクトと各レイヤーのテクスチャを保持する。
pub struct WgpuPresenter {
    /// ウィンドウへの描画先サーフェス。OS のスワップチェーンに対応する。
    surface: wgpu::Surface<'static>,
    /// 論理 GPU デバイス。テクスチャやバッファの生成・パイプラインの構築に使う。
    /// Arc でラップして gpu-paint クレートと共有できるようにする。
    device: Arc<wgpu::Device>,
    /// コマンドキュー。エンコードしたコマンドを GPU へ提出する。
    /// Arc でラップして gpu-paint クレートと共有できるようにする。
    queue: Arc<wgpu::Queue>,
    /// サーフェス設定（解像度・フォーマット・プレゼントモードなど）。
    config: wgpu::SurfaceConfiguration,
    /// レンダーパス開始時のクリア色 (アプリ背景色)。
    clear_color: wgpu::Color,
    /// テクスチャ提示パイプライン (キャンバス / パネル / ステータス共用)。
    present: PresentPipeline,
    // 各レイヤーの GPU リソース（None = 未初期化）
    canvas_surface: Option<UploadedLayerTexture>,
    /// GPU キャンバステクスチャのバインドグループキャッシュ。
    canvas_gpu_bind_group_cache: Option<GpuBindGroupCache>,
    /// HTML パネル毎の bind_group キャッシュ。panel_id をキーにし、
    /// 紐づくテクスチャのサイズ変化で再生成する。
    panel_bind_groups: HashMap<String, PanelBindEntry>,
    /// 単色矩形 (背景・キャンバス枠・アクティブパネル枠・L3 overlay AABB) 用 GPU パイプライン。
    solid_quad_pipeline: QuadPipeline,
    /// L3 ブラシプレビュー円リング用 GPU パイプライン (SDF)。
    overlay_circle_pipeline: QuadPipeline,
    /// L3 ラッソ線分用 GPU パイプライン (カプセル SDF)。
    overlay_line_pipeline: QuadPipeline,
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
    /// 5. パイプライン・サンプラー・シェーダを生成
    pub async fn new(window: Arc<Window>) -> Result<Self> {
        let size = window.inner_size();

        // wgpu の最上位オブジェクト。バックエンド（Vulkan/Metal/DX12/WebGPU）の
        // 選択は Instance がデフォルトで行う。
        let instance = wgpu::Instance::default();

        // winit ウィンドウからサーフェスを作成。
        let surface = instance
            .create_surface(window)
            .context("failed to create surface")?;

        // このサーフェスで使える物理 GPU（アダプター）を非同期で要求する。
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface), // surface と互換性のある GPU を選ぶ
                force_fallback_adapter: false,      // ソフトウェアレンダラーは使わない
            })
            .await
            .context("failed to acquire adapter")?;

        // 物理 GPU（adapter）から論理デバイスとコマンドキューを取得。
        // Rgba8Unorm の STORAGE_READ_WRITE（gpu-paint composite shader が要求）を
        // 利用可能な場合は opt-in する。
        let storage_format_features =
            adapter.features() & wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("altpaint-device"),
                required_features: storage_format_features,
                experimental_features: Default::default(),
                required_limits: adapter.limits(), // アダプターのデフォルト制限を引き継ぐ
                memory_hints: wgpu::MemoryHints::Performance, // パフォーマンス優先の VRAM 配置
                trace: wgpu::Trace::default(),
            })
            .await
            .context("failed to create device")?;
        // gpu-paint クレートと共有できるよう Arc でラップする。
        let device = Arc::new(device);
        let queue = Arc::new(queue);

        let srgb_canvas_view_supported =
            gpu_paint::format_check::supports_rgba8unorm_storage(&adapter);

        let adapter_info = adapter.get_info();
        if srgb_canvas_view_supported {
            eprintln!(
                "[altpaint] canvas backend: GPU (adapter='{}', backend={:?}, Rgba8Unorm STORAGE_READ_WRITE supported)",
                adapter_info.name, adapter_info.backend
            );
        } else {
            eprintln!(
                "[altpaint] canvas backend: GPU (adapter='{}', backend={:?}, Rgba8Unorm STORAGE_READ_WRITE NOT supported — compute pipeline may panic)",
                adapter_info.name, adapter_info.backend
            );
        }

        // サーフェスがサポートするピクセルフォーマットの一覧を取得する。
        let surface_capabilities = surface.get_capabilities(&adapter);

        // sRGB 対応フォーマットを優先して選ぶ（なければ先頭を使う）。
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

        let present = PresentPipeline::new(&device, config.format);

        let solid_quad_pipeline = QuadPipeline::new(
            &device,
            config.format,
            SOLID_QUAD_SHADER,
            SOLID_QUAD_UNIFORM_SIZE,
            "solid-quad",
        );
        let overlay_circle_pipeline = QuadPipeline::new(
            &device,
            config.format,
            CIRCLE_QUAD_SHADER,
            CIRCLE_QUAD_UNIFORM_SIZE,
            "circle-quad",
        );
        let overlay_line_pipeline = QuadPipeline::new(
            &device,
            config.format,
            LINE_QUAD_SHADER,
            LINE_QUAD_UNIFORM_SIZE,
            "line-quad",
        );

        let clear_color = wgpu::Color {
            r: crate::theme::APP_BACKGROUND[0] as f64 / 255.0,
            g: crate::theme::APP_BACKGROUND[1] as f64 / 255.0,
            b: crate::theme::APP_BACKGROUND[2] as f64 / 255.0,
            a: crate::theme::APP_BACKGROUND[3] as f64 / 255.0,
        };

        Ok(Self {
            surface,
            device,
            queue,
            config,
            clear_color,
            present,
            canvas_surface: None, // 初回フレームで ensure が生成する
            canvas_gpu_bind_group_cache: None,
            panel_bind_groups: HashMap::new(),
            solid_quad_pipeline,
            overlay_circle_pipeline,
            overlay_line_pipeline,
        })
    }

    /// Arc でラップされたデバイスへの参照を返す。
    ///
    /// gpu-paint クレートの `LayerTextureStore` と共有するために使う。
    pub fn device(&self) -> Arc<wgpu::Device> {
        Arc::clone(&self.device)
    }

    /// Arc でラップされたキューへの参照を返す。
    ///
    /// gpu-paint クレートの `LayerTextureStore` と共有するために使う。
    pub fn queue(&self) -> Arc<wgpu::Queue> {
        Arc::clone(&self.queue)
    }

    /// ウィンドウサイズ変更時にサーフェスを再設定する。
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use self::shaders::{LAYER_UNIFORM_SIZE, PRESENT_SHADER};
    use self::textures::{fullscreen_quad, quad_uniform_bytes};

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
