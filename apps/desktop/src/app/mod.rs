//! デスクトップアプリケーションの状態遷移と副作用の窓口を定義する。
//!
//! `DesktopApp` はドキュメント、UI シェル、プロジェクト I/O を束ね、
//! ランタイムから見た「状態付きのアプリ本体」として振る舞う。

mod background_tasks;
mod bootstrap;
pub(crate) mod canvas_frame;
mod command_router;
pub(crate) mod cursor;
mod input;
mod io_state;
mod panel_config_sync;
mod panel_dispatch;
mod present;
mod present_state;
mod services;
mod canvas_state;
mod snapshot_store;
#[cfg(test)]
pub(crate) mod tests;

use std::path::PathBuf;

#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};

use app_core::{CanvasBitmap, EditHistory, PageDirtyRect, PagePoint, Document, KomaId};
use desktop_support::{
    DesktopDialogs, NativeDesktopDialogs, WorkspacePresetCatalog, default_workspace_preset_path,
};
use panel_runtime::PanelRuntime;
use panel_workspace::PanelPresentation;

pub(crate) use self::canvas_frame::CanvasFrame;
use self::io_state::DesktopIoState;
#[cfg(test)]
pub(crate) use self::panel_dispatch::PanelDragState;
use self::panel_dispatch::PanelInteractionState;
use self::present_state::PresentFrameUpdate;
use self::snapshot_store::DocumentSnapshotStore;
use crate::frame::DesktopLayout;
use paint_engine::CanvasInputState;

#[cfg(test)]
static TEST_SESSION_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// canvas_view_geometry のキャッシュエントリ。入力が同じなら再計算を省略するために使う。
struct CachedCanvasViewGeometry {
    viewport: canvas_geometry::PixelRect,
    canvas_width: usize,
    canvas_height: usize,
    transform: app_core::CanvasViewTransform,
    geometry: Option<canvas_geometry::CanvasViewGeometry>,
}

/// ストローク中のビットマップ差分追跡状態。
struct PendingStroke {
    koma_id: KomaId,
    layer_index: usize,
    /// ストローク開始前のレイヤービットマップ全体。
    ///
    /// GPU パスでは `None`（commit 時に CPU bitmap がストローク前状態を保持している）。
    /// CPU パスでは `Some`（ストローク中に CPU bitmap が書き換わるため事前に保存）。
    before_layer: Option<CanvasBitmap>,
    /// ストローク中に蓄積したコマローカル dirty rect の合計。
    dirty: Option<PageDirtyRect>,
}

pub(super) const WORKSPACE_PRESET_PANEL_ID: &str = "builtin.workspace-presets";
pub(super) const TOOL_PALETTE_PANEL_ID: &str = "builtin.tool-palette";

/// HTML パネル上端のホスト描画タイトルバー (chrome) の高さ (px)。
/// hit テーブル更新 (`present.rs`) と GPU 描画 (`runtime.rs`) で共有する。
pub(crate) const HTML_PANEL_CHROME_HEIGHT: u32 = 24;

/// ランタイムから利用されるデスクトップアプリ本体を表す。
pub(crate) struct DesktopApp {
    pub(crate) document: Document,
    pub(crate) panel_runtime: PanelRuntime,
    pub(crate) panel_presentation: PanelPresentation,
    pub(crate) io_state: DesktopIoState,
    workspace_presets: WorkspacePresetCatalog,
    active_workspace_preset_id: String,
    paint_engine: paint_engine::PaintEngine,
    canvas_input: CanvasInputState,
    pub(crate) layout: Option<DesktopLayout>,
    canvas_frame: Option<CanvasFrame>,
    /// Phase 9E-4: ステータスバー (HtmlPanelView GPU 描画)。
    pub(crate) status_panel: crate::frame::status_panel::StatusPanel,
    /// 次フレームで消化される提示無効化状態 (保留 dirty rect・再構築フラグ)。
    pub(crate) invalidation: present_state::PresentInvalidation,
    cached_canvas_view_geometry: Option<CachedCanvasViewGeometry>,
    pub(crate) history: EditHistory,
    pub(crate) snapshots: DocumentSnapshotStore,
    pub(crate) panel_interaction: PanelInteractionState,
    hover_canvas_position: Option<PagePoint>,
    pending_stroke: Option<PendingStroke>,
    /// 進行中のバックグラウンドジョブ (project save 等)。
    pub(crate) background_jobs: Vec<background_tasks::BackgroundJob>,
    /// GPU ペイントリソース一式。`install_gpu_resources` で一括構築される。
    pub(crate) gpu: Option<GpuPaintEngine>,
}

/// GPU ペイントリソース一式。
///
/// `install_gpu_resources` で全フィールドを同時に構築する (all-or-nothing)。
/// 部分的な初期化状態は存在しないため、各リソースは Option で包まない。
pub(crate) struct GpuPaintEngine {
    /// GPU レイヤーテクスチャプール。
    pub(crate) pool: gpu_paint::LayerTextureStore,
    /// GPU ブラシ計算シェーダーディスパッチャ。
    pub(crate) brush: gpu_paint::BrushPipeline,
    /// GPU 塗りつぶしディスパッチャ。
    pub(crate) fill: gpu_paint::FillPipeline,
    /// GPU レイヤー合成ディスパッチャ。
    pub(crate) compositor: gpu_paint::CompositePipeline,
}

impl DesktopApp {
    pub(crate) fn new(project_path: PathBuf) -> Self {
        Self::new_with_dialogs_session_path_and_workspace_preset_path(
            project_path,
            Box::new(NativeDesktopDialogs),
            default_desktop_session_path(),
            default_workspace_preset_path(),
        )
    }

    pub(crate) fn new_with_dialogs_session_path_and_workspace_preset_path(
        project_path: PathBuf,
        dialogs: Box<dyn DesktopDialogs>,
        session_path: PathBuf,
        workspace_preset_path: PathBuf,
    ) -> Self {
        let bootstrap = Self::bootstrap_state(project_path, &session_path, &workspace_preset_path);

        let mut app = Self {
            document: bootstrap.document,
            panel_runtime: bootstrap.panel_runtime,
            panel_presentation: bootstrap.panel_presentation,
            io_state: DesktopIoState::new(
                bootstrap.project_path,
                session_path,
                workspace_preset_path,
                dialogs,
            ),
            workspace_presets: bootstrap.workspace_presets,
            active_workspace_preset_id: bootstrap.active_workspace_preset_id,
            paint_engine: paint_engine::PaintEngine::default(),
            canvas_input: CanvasInputState::default(),
            layout: None,
            canvas_frame: None,
            status_panel: crate::frame::status_panel::StatusPanel::new(),
            invalidation: present_state::PresentInvalidation::at_startup(),
            cached_canvas_view_geometry: None,
            history: EditHistory::new(),
            snapshots: DocumentSnapshotStore::default(),
            panel_interaction: PanelInteractionState::default(),
            hover_canvas_position: None,
            pending_stroke: None,
            background_jobs: Vec::new(),
            gpu: None,
        };
        app.refresh_canvas_frame();
        app.ensure_workspace_presets_file(&app.io_state.workspace_preset_path);
        app.ensure_canvas_size_presets_file();
        app.refresh_new_document_size_presets();
        app.refresh_workspace_presets();
        app
    }
}

impl DesktopApp {
    /// GPU リソースを初期化してフィールドへ代入し、全レイヤーを同期する。
    ///
    /// `supports_rgba8unorm_storage` 未対応のアダプターでは、compute pipeline 生成が
    /// panic する可能性がある。alpha 期間としては未対応 GPU を切り捨てる方針 (Phase 9A)。
    pub(crate) fn install_gpu_resources(
        &mut self,
        device: std::sync::Arc<wgpu::Device>,
        queue: std::sync::Arc<wgpu::Queue>,
    ) {
        self.gpu = Some(GpuPaintEngine {
            pool: gpu_paint::LayerTextureStore::new(device.clone(), queue.clone()),
            brush: gpu_paint::BrushPipeline::new(device.clone(), queue.clone()),
            fill: gpu_paint::FillPipeline::new(device.clone(), queue.clone()),
            compositor: gpu_paint::CompositePipeline::new(device, queue),
        });
        self.sync_all_layers_to_gpu();
        self.recomposite_all_komas();
    }

    /// 全ページ・全パネル・全レイヤーの CPU ビットマップを GPU テクスチャへ同期する。
    /// マスクと composite テクスチャも同期する。
    ///
    /// レイヤー追加/削除/並べ替えで古いエントリがずれるのを防ぐため、
    /// 各コマのレイヤー/マスクエントリを先にクリアしてから再登録する。
    pub(crate) fn sync_all_layers_to_gpu(&mut self) {
        if self.gpu.is_none() {
            return;
        }
        let koma_ids: Vec<String> = self
            .document
            .work
            .pages
            .iter()
            .flat_map(|page| page.komas.iter().map(|p| p.id.0.to_string()))
            .collect();
        if let Some(gpu) = self.gpu.as_mut() {
            for pid in &koma_ids {
                gpu.pool.clear_layers_for_koma(pid);
            }
        }
        #[derive(Clone)]
        struct LayerSync {
            koma_id: String,
            koma_w: u32,
            koma_h: u32,
            layer_index: usize,
            w: u32,
            h: u32,
            pixels: Vec<u8>,
            mask: Option<(u32, u32, Vec<u8>)>,
        }
        let mut entries: Vec<LayerSync> = Vec::new();
        for page in &self.document.work.pages {
            for koma in &page.komas {
                let koma_id = koma.id.0.to_string();
                let koma_w = koma.composite_cache.width as u32;
                let koma_h = koma.composite_cache.height as u32;
                for (idx, layer) in koma.layers.iter().enumerate() {
                    entries.push(LayerSync {
                        koma_id: koma_id.clone(),
                        koma_w,
                        koma_h,
                        layer_index: idx,
                        w: layer.bitmap.width as u32,
                        h: layer.bitmap.height as u32,
                        pixels: layer.bitmap.pixels.clone(),
                        mask: layer.mask.as_ref().map(|m| {
                            (m.width as u32, m.height as u32, m.alpha.clone())
                        }),
                    });
                }
            }
        }
        let pool = &mut self.gpu.as_mut().unwrap().pool;
        for entry in entries {
            pool.ensure_composite_texture(&entry.koma_id, entry.koma_w, entry.koma_h);
            pool.create_layer_texture(&entry.koma_id, entry.layer_index, entry.w, entry.h);
            pool.upload_cpu_bitmap(&entry.koma_id, entry.layer_index, &entry.pixels);
            match entry.mask {
                Some((mw, mh, alpha)) => {
                    pool.upload_mask(&entry.koma_id, entry.layer_index, mw, mh, &alpha);
                }
                None => {
                    pool.remove_mask(&entry.koma_id, entry.layer_index);
                }
            }
        }
    }

    /// GPU テクスチャをキャンバスの表示正本として使えるかどうかを返す。
    ///
    /// `true` のとき: GPU リソースが揃っており、`canvas_layer_source_kind` が
    /// `Gpu` / `GpuComposite` のいずれかを返せる状態。
    /// 現在はテストからのみ参照される (production コードは `canvas_layer_source_kind()` を直接使う)。
    #[cfg(test)]
    pub(crate) fn should_use_gpu_canvas_source(&self) -> bool {
        self.canvas_layer_source_kind().is_some()
    }

    /// アクティブパネルに対してどの GPU ソースを使うべきかを返す。
    ///
    /// - 単一レイヤー: `Single` → `CanvasLayerSource::Gpu { layer_index: 0 }`
    /// - 複数レイヤー: `Composite` → `CanvasLayerSource::GpuComposite`
    /// - GPU 非対応: `None`
    pub(crate) fn canvas_layer_source_kind(&self) -> Option<GpuCanvasSourceKind> {
        let pool = &self.gpu.as_ref()?.pool;
        let koma = self.document.active_koma()?;
        let pid = koma.id.0.to_string();
        if koma.layers.len() == 1 {
            if pool.get(&pid, 0).is_some() {
                Some(GpuCanvasSourceKind::Single)
            } else {
                None
            }
        } else if pool.get_composite(&pid).is_some() {
            Some(GpuCanvasSourceKind::Composite)
        } else {
            None
        }
    }

    /// GPU レイヤーテクスチャプールへの参照を返す。
    pub(crate) fn layer_texture_store(&self) -> Option<&gpu_paint::LayerTextureStore> {
        self.gpu.as_ref().map(|gpu| &gpu.pool)
    }

    /// 指定コマに対し、現在のレイヤー構成を composite テクスチャへ再合成する。
    ///
    /// `dirty` はコマローカル座標系の矩形。None の場合はコマ全体。
    pub(crate) fn recomposite_koma(
        &self,
        koma_id: KomaId,
        dirty: Option<PageDirtyRect>,
    ) {
        let Some(gpu) = self.gpu.as_ref() else {
            return;
        };
        let pid_str = koma_id.0.to_string();
        let Some(koma) = self
            .document
            .work
            .pages
            .iter()
            .flat_map(|p| &p.komas)
            .find(|p| p.id == koma_id)
        else {
            return;
        };
        let Some(composite) = gpu.pool.get_composite(&pid_str) else {
            return;
        };
        let (pw, ph) = (koma.composite_cache.width as u32, koma.composite_cache.height as u32);
        let rect = dirty.unwrap_or(PageDirtyRect {
            x: 0,
            y: 0,
            width: koma.composite_cache.width,
            height: koma.composite_cache.height,
        });
        let x0 = (rect.x as u32).min(pw);
        let y0 = (rect.y as u32).min(ph);
        let x1 = ((rect.x + rect.width) as u32).min(pw);
        let y1 = ((rect.y + rect.height) as u32).min(ph);
        if x0 >= x1 || y0 >= y1 {
            return;
        }

        let entries: Vec<gpu_paint::CompositeLayerEntry<'_>> = koma
            .layers
            .iter()
            .enumerate()
            .filter_map(|(idx, layer)| {
                let color = gpu.pool.get(&pid_str, idx)?;
                let mask = gpu.pool.get_mask(&pid_str, idx);
                Some(gpu_paint::CompositeLayerEntry {
                    color,
                    mask,
                    blend_code: layer.blend_mode.gpu_code(),
                    visible: layer.visible,
                })
            })
            .collect();

        gpu.compositor.recomposite(composite, &entries, (x0, y0, x1, y1));
    }

    /// 全コマの composite テクスチャを再合成する。`install_gpu_resources` や
    /// `sync_all_layers_to_gpu` 後に呼ぶ。
    pub(crate) fn recomposite_all_komas(&self) {
        let ids: Vec<KomaId> = self
            .document
            .work
            .pages
            .iter()
            .flat_map(|page| page.komas.iter().map(|p| p.id))
            .collect();
        for id in ids {
            self.recomposite_koma(id, None);
        }
    }
}

/// GPU キャンバスソースの種別。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GpuCanvasSourceKind {
    Single,
    Composite,
}

fn default_desktop_session_path() -> PathBuf {
    #[cfg(test)]
    {
        let unique = TEST_SESSION_COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "altpaint-test-session-{}-{unique}.json",
            std::process::id()
        ))
    }

    #[cfg(not(test))]
    {
        desktop_support::default_session_path()
    }
}
