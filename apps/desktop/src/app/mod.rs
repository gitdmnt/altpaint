//! デスクトップアプリケーションの状態遷移と副作用の窓口を定義する。
//!
//! `DesktopApp` はドキュメント、UI シェル、プロジェクト I/O を束ね、
//! ランタイムから見た「状態付きのアプリ本体」として振る舞う。

mod background_tasks;
mod bootstrap;
mod command_router;
mod commands;
mod drawing;
mod input;
mod io_state;
mod panel_config_sync;
mod panel_dispatch;
mod present;
mod present_state;
mod services;
mod snapshot_store;
mod state;
#[cfg(test)]
pub(crate) mod tests;

use std::path::PathBuf;

#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};

use app_core::{CanvasBitmap, CanvasDirtyRect, CanvasPoint, CommandHistory, Document, PanelId};
use desktop_support::{
    DesktopDialogs, NativeDesktopDialogs, WorkspacePresetCatalog, default_workspace_preset_path,
};
use panel_runtime::PanelRuntime;
use render::RenderFrame;
use ui_shell::{PanelPresentation, PanelSurface};

use self::io_state::DesktopIoState;
#[cfg(test)]
pub(crate) use self::panel_dispatch::PanelDragState;
use self::panel_dispatch::PanelInteractionState;
use self::present_state::PresentFrameUpdate;
use self::snapshot_store::SnapshotStore;
use crate::frame::DesktopLayout;
use canvas::CanvasInputState;

#[cfg(test)]
static TEST_SESSION_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// canvas_scene のキャッシュエントリ。入力が同じなら再計算を省略するために使う。
struct CachedCanvasScene {
    viewport: render::PixelRect,
    canvas_width: usize,
    canvas_height: usize,
    transform: app_core::CanvasViewTransform,
    scene: Option<render::CanvasScene>,
}

/// ストローク中のビットマップ差分追跡状態。
struct PendingStroke {
    panel_id: PanelId,
    layer_index: usize,
    /// ストローク開始前のレイヤービットマップ全体。
    ///
    /// GPU パスでは `None`（commit 時に CPU bitmap がストローク前状態を保持している）。
    /// CPU パスでは `Some`（ストローク中に CPU bitmap が書き換わるため事前に保存）。
    before_layer: Option<CanvasBitmap>,
    /// ストローク中に蓄積したパネルローカル dirty rect の合計。
    dirty: Option<CanvasDirtyRect>,
}

pub(super) const WORKSPACE_PRESET_PANEL_ID: &str = "builtin.workspace-presets";
pub(super) const TOOL_PALETTE_PANEL_ID: &str = "builtin.tool-palette";

/// ランタイムから利用されるデスクトップアプリ本体を表す。
pub(crate) struct DesktopApp {
    pub(crate) document: Document,
    pub(crate) panel_runtime: PanelRuntime,
    pub(crate) panel_presentation: PanelPresentation,
    pub(crate) io_state: DesktopIoState,
    workspace_presets: WorkspacePresetCatalog,
    active_workspace_preset_id: String,
    paint_runtime: drawing::CanvasRuntime,
    canvas_input: CanvasInputState,
    pub(crate) panel_surface: Option<PanelSurface>,
    pub(crate) layout: Option<DesktopLayout>,
    canvas_frame: Option<RenderFrame>,
    background_frame: Option<RenderFrame>,
    temp_overlay_frame: Option<RenderFrame>,
    ui_panel_frame: Option<RenderFrame>,
    pending_canvas_dirty_rect: Option<app_core::CanvasDirtyRect>,
    pending_background_dirty_rect: Option<crate::frame::Rect>,
    pending_temp_overlay_dirty_rect: Option<crate::frame::Rect>,
    pending_ui_panel_dirty_rect: Option<crate::frame::Rect>,
    pending_canvas_transform_update: bool,
    cached_canvas_scene: Option<CachedCanvasScene>,
    pub(crate) history: CommandHistory,
    pub(crate) snapshots: SnapshotStore,
    pub(crate) panel_interaction: PanelInteractionState,
    hover_canvas_position: Option<CanvasPoint>,
    pending_stroke: Option<PendingStroke>,
    deferred_view_panel_sync: bool,
    deferred_status_refresh: bool,
    needs_panel_surface_refresh: bool,
    needs_status_refresh: bool,
    needs_full_present_rebuild: bool,
    /// GPU レイヤーテクスチャプール。
    pub(crate) gpu_canvas_pool: Option<gpu_canvas::GpuCanvasPool>,
    /// GPU ペン先テクスチャキャッシュ。
    pub(crate) gpu_pen_tip_cache: Option<gpu_canvas::GpuPenTipCache>,
    /// GPU ブラシ計算シェーダーディスパッチャ。
    pub(crate) gpu_brush: Option<gpu_canvas::GpuBrushDispatch>,
    /// GPU 塗りつぶしディスパッチャ。
    pub(crate) gpu_fill: Option<gpu_canvas::GpuFillDispatch>,
    /// GPU レイヤー合成ディスパッチャ。
    pub(crate) gpu_compositor: Option<gpu_canvas::GpuLayerCompositor>,
}

impl DesktopApp {
    /// 既定値を使って新しいインスタンスを生成する。
    pub(crate) fn new(project_path: PathBuf) -> Self {
        Self::new_with_dialogs_session_path_and_workspace_preset_path(
            project_path,
            Box::new(NativeDesktopDialogs),
            default_desktop_session_path(),
            default_workspace_preset_path(),
        )
    }

    /// 既定値を使って新しいインスタンスを生成する。
    #[allow(dead_code)]
    pub(crate) fn new_with_dialogs(
        project_path: PathBuf,
        dialogs: Box<dyn DesktopDialogs>,
    ) -> Self {
        Self::new_with_dialogs_session_path_and_workspace_preset_path(
            project_path,
            dialogs,
            default_desktop_session_path(),
            default_workspace_preset_path(),
        )
    }

    /// 既定値を使って新しいインスタンスを生成する。
    ///
    /// 必要に応じて dirty 状態も更新します。
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
            paint_runtime: drawing::CanvasRuntime::default(),
            canvas_input: CanvasInputState::default(),
            panel_surface: None,
            layout: None,
            canvas_frame: None,
            background_frame: None,
            temp_overlay_frame: None,
            ui_panel_frame: None,
            pending_canvas_dirty_rect: None,
            pending_background_dirty_rect: None,
            pending_temp_overlay_dirty_rect: None,
            pending_ui_panel_dirty_rect: None,
            pending_canvas_transform_update: false,
            cached_canvas_scene: None,
            history: CommandHistory::new(),
            snapshots: SnapshotStore::default(),
            panel_interaction: PanelInteractionState::default(),
            hover_canvas_position: None,
            pending_stroke: None,
            deferred_view_panel_sync: false,
            deferred_status_refresh: false,
            needs_panel_surface_refresh: true,
            needs_status_refresh: false,
            needs_full_present_rebuild: true,
            gpu_canvas_pool: None,
            gpu_pen_tip_cache: None,
            gpu_brush: None,
            gpu_fill: None,
            gpu_compositor: None,
        };
        app.refresh_canvas_frame();
        app.ensure_workspace_presets_file(&app.io_state.workspace_preset_path);
        app.ensure_canvas_templates_file();
        app.refresh_new_document_templates();
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
        self.gpu_canvas_pool = Some(gpu_canvas::GpuCanvasPool::new(
            device.clone(),
            queue.clone(),
        ));
        self.gpu_pen_tip_cache = Some(gpu_canvas::GpuPenTipCache::new(
            device.clone(),
            queue.clone(),
        ));
        self.gpu_brush = Some(gpu_canvas::GpuBrushDispatch::new(
            device.clone(),
            queue.clone(),
        ));
        self.gpu_fill = Some(gpu_canvas::GpuFillDispatch::new(
            device.clone(),
            queue.clone(),
        ));
        self.gpu_compositor = Some(gpu_canvas::GpuLayerCompositor::new(device, queue));
        self.sync_all_layers_to_gpu();
        self.upload_active_pen_tip_to_gpu_cache();
        self.recomposite_all_panels();
    }

    /// 全ページ・全パネル・全レイヤーの CPU ビットマップを GPU テクスチャへ同期する。
    /// マスクと composite テクスチャも同期する。
    ///
    /// レイヤー追加/削除/並べ替えで古いエントリがずれるのを防ぐため、
    /// 各パネルのレイヤー/マスクエントリを先にクリアしてから再登録する。
    pub(crate) fn sync_all_layers_to_gpu(&mut self) {
        if self.gpu_canvas_pool.is_none() {
            return;
        }
        let panel_ids: Vec<String> = self
            .document
            .work
            .pages
            .iter()
            .flat_map(|page| page.panels.iter().map(|p| p.id.0.to_string()))
            .collect();
        if let Some(pool) = self.gpu_canvas_pool.as_mut() {
            for pid in &panel_ids {
                pool.clear_layers_for_panel(pid);
            }
        }
        #[derive(Clone)]
        struct LayerSync {
            panel_id: String,
            panel_w: u32,
            panel_h: u32,
            layer_index: usize,
            w: u32,
            h: u32,
            pixels: Vec<u8>,
            mask: Option<(u32, u32, Vec<u8>)>,
        }
        let mut entries: Vec<LayerSync> = Vec::new();
        for page in &self.document.work.pages {
            for panel in &page.panels {
                let panel_id = panel.id.0.to_string();
                let panel_w = panel.bitmap.width as u32;
                let panel_h = panel.bitmap.height as u32;
                for (idx, layer) in panel.layers.iter().enumerate() {
                    entries.push(LayerSync {
                        panel_id: panel_id.clone(),
                        panel_w,
                        panel_h,
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
        let pool = self.gpu_canvas_pool.as_mut().unwrap();
        for entry in entries {
            pool.ensure_composite_texture(&entry.panel_id, entry.panel_w, entry.panel_h);
            pool.create_layer_texture(&entry.panel_id, entry.layer_index, entry.w, entry.h);
            pool.upload_cpu_bitmap(&entry.panel_id, entry.layer_index, &entry.pixels);
            match entry.mask {
                Some((mw, mh, alpha)) => {
                    pool.upload_mask(&entry.panel_id, entry.layer_index, mw, mh, &alpha);
                }
                None => {
                    pool.remove_mask(&entry.panel_id, entry.layer_index);
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
        let pool = self.gpu_canvas_pool.as_ref()?;
        let panel = self.document.active_panel()?;
        let pid = panel.id.0.to_string();
        if panel.layers.len() == 1 {
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
    pub(crate) fn gpu_canvas_pool(&self) -> Option<&gpu_canvas::GpuCanvasPool> {
        self.gpu_canvas_pool.as_ref()
    }

    /// 指定パネルに対し、現在のレイヤー構成を composite テクスチャへ再合成する。
    ///
    /// `dirty` はパネルローカル座標系の矩形。None の場合はパネル全体。
    pub(crate) fn recomposite_panel(
        &self,
        panel_id: PanelId,
        dirty: Option<CanvasDirtyRect>,
    ) {
        let Some(pool) = self.gpu_canvas_pool.as_ref() else {
            return;
        };
        let Some(compositor) = self.gpu_compositor.as_ref() else {
            return;
        };
        let pid_str = panel_id.0.to_string();
        let Some(panel) = self
            .document
            .work
            .pages
            .iter()
            .flat_map(|p| &p.panels)
            .find(|p| p.id == panel_id)
        else {
            return;
        };
        let Some(composite) = pool.get_composite(&pid_str) else {
            return;
        };
        let (pw, ph) = (panel.bitmap.width as u32, panel.bitmap.height as u32);
        let rect = dirty.unwrap_or(CanvasDirtyRect {
            x: 0,
            y: 0,
            width: panel.bitmap.width,
            height: panel.bitmap.height,
        });
        let x0 = (rect.x as u32).min(pw);
        let y0 = (rect.y as u32).min(ph);
        let x1 = ((rect.x + rect.width) as u32).min(pw);
        let y1 = ((rect.y + rect.height) as u32).min(ph);
        if x0 >= x1 || y0 >= y1 {
            return;
        }

        let entries: Vec<gpu_canvas::CompositeLayerEntry<'_>> = panel
            .layers
            .iter()
            .enumerate()
            .filter_map(|(idx, layer)| {
                let color = pool.get(&pid_str, idx)?;
                let mask = pool.get_mask(&pid_str, idx);
                Some(gpu_canvas::CompositeLayerEntry {
                    color,
                    mask,
                    blend_code: layer.blend_mode.gpu_code(),
                    visible: layer.visible,
                })
            })
            .collect();

        compositor.recomposite(composite, &entries, (x0, y0, x1, y1));
    }

    /// 全パネルの composite テクスチャを再合成する。`install_gpu_resources` や
    /// `sync_all_layers_to_gpu` 後に呼ぶ。
    pub(crate) fn recomposite_all_panels(&self) {
        let ids: Vec<PanelId> = self
            .document
            .work
            .pages
            .iter()
            .flat_map(|page| page.panels.iter().map(|p| p.id))
            .collect();
        for id in ids {
            self.recomposite_panel(id, None);
        }
    }
}

/// GPU キャンバスソースの種別。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GpuCanvasSourceKind {
    Single,
    Composite,
}

/// 既定の desktop セッション パス を返す。
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
