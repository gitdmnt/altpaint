//! デスクトップアプリケーションの状態遷移と副作用の窓口を定義する。
//!
//! `DesktopApp` はドキュメント、UI シェル、プロジェクト I/O を束ね、
//! ランタイムから見た「状態付きのアプリ本体」として振る舞う。

mod background_tasks;
mod bootstrap;
pub(crate) mod cpu_canvas_snapshot;
mod command_effects;
mod command_router;
mod frame;
pub(crate) mod cursor;
mod host_request_router;
mod input;
mod panel_config_sync;
mod present;
mod present_api;
mod project_paths;
mod invalidation;
mod services;
mod canvas_state;
#[cfg(test)]
pub(crate) mod tests;

use std::path::PathBuf;

#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};

use document_model::{Document, KomaId};
use geometry::{PageDirtyRect, PagePoint};
use crate::features::workspace::WorkspaceState;
use crate::platform::{DesktopDialogs, NativeDesktopDialogs, default_workspace_preset_path};
use panel_runtime::PanelRuntime;
use panel_workspace::PanelWorkspace;

pub(crate) use self::cpu_canvas_snapshot::CpuCanvasSnapshot;
use self::project_paths::ProjectPaths;
#[cfg(test)]
pub(crate) use crate::features::panel_interaction::PanelDragState;
use crate::features::panel_interaction::PanelInteractionState;
use self::invalidation::PresentFrameUpdate;
use crate::features::koma::KomaGesture;
use crate::features::paint::{EditHistory, PendingStroke};
use crate::features::snapshots::DocumentSnapshotStore;
use crate::present_quads::DesktopLayout;
use paint_engine::CanvasInputState;

#[cfg(test)]
static TEST_SESSION_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// canvas_view_geometry のキャッシュエントリ。入力が同じなら再計算を省略するために使う。
pub(crate) struct CachedCanvasViewGeometry {
    viewport: geometry::WindowRect,
    canvas_width: usize,
    canvas_height: usize,
    transform: editor_state::CanvasViewTransform,
    geometry: Option<canvas_geometry::CanvasViewGeometry>,
}

/// paint feature のサブ状態 (BL-110)。
///
/// DesktopApp に散在していたペイント実行・履歴・CPU 表示キャッシュの各フィールドを
/// まとめる。`cpu_canvas_snapshot` / `hover_canvas_position` / `cached_canvas_view_geometry`
/// は CPU 表示経路 (BL-136) の派生キャッシュであり paint feature が正本を所有する。
pub(crate) struct PaintState {
    /// ペイント計算機 (状態を持たない純計算)。
    pub(crate) paint_engine: paint_engine::PaintEngine,
    /// ペイント系ジェスチャの進行中状態。
    pub(crate) canvas_input: CanvasInputState,
    /// ストローク中のビットマップ差分追跡状態。
    pub(crate) pending_stroke: Option<PendingStroke>,
    /// 型付き patch (Cpu/Gpu) を積む編集履歴 (BL-076)。
    pub(crate) history: EditHistory,
    /// CPU 表示用のキャンバス合成スナップショット (BL-136)。
    pub(crate) cpu_canvas_snapshot: Option<CpuCanvasSnapshot>,
    /// ブラシプレビュー描画用の現在のホバー位置 (ページ座標)。
    pub(crate) hover_canvas_position: Option<PagePoint>,
    /// canvas_view_geometry の再計算を省くキャッシュ。
    pub(crate) cached_canvas_view_geometry: Option<CachedCanvasViewGeometry>,
}

impl PaintState {
    fn new() -> Self {
        Self {
            paint_engine: paint_engine::PaintEngine::new(),
            canvas_input: CanvasInputState::default(),
            pending_stroke: None,
            history: EditHistory::new(),
            cpu_canvas_snapshot: None,
            hover_canvas_position: None,
            cached_canvas_view_geometry: None,
        }
    }
}

/// HTML パネル上端のホスト描画タイトルバー (chrome) の高さ (px)。
/// hit テーブル更新 (`present.rs`) と GPU 描画 (`event_loop.rs`) で共有する。
pub(crate) const PANEL_CHROME_HEIGHT: u32 = 24;

/// デスクトップアプリ本体 (composition root)。
///
/// 各 feature サブ状態 (PaintState / WorkspaceState 等) と subsystem (panel-runtime /
/// panel-workspace / GPU リソース) を保持し配線するだけの薄い構造 (BL-110)。
/// フィールドの可視性は所有境界を表す:
/// - `pub(crate)` のフィールドは対応する feature ハンドラ (`features/*` の `impl DesktopApp`)
///   が自スライスの状態として直接触れる co-owned 状態。
/// - 可視性指定なし (`app` モジュール内のみ) のフィールドは feature から参照されない
///   app 内部状態。
///
/// 消費者 (`event_loop`) はいずれのフィールドにも直接触れず、`present_api` などの
/// メソッド境界のみを通じてアクセスする。
pub(crate) struct DesktopApp {
    pub(crate) document: Document,
    pub(crate) panel_runtime: PanelRuntime,
    pub(crate) panel_workspace: PanelWorkspace,
    /// プロジェクト/セッション/ワークスペースプリセットの保存先パス (D12)。
    pub(crate) paths: ProjectPaths,
    /// ファイルダイアログの依存ポート (D12: パス状態から分離)。
    pub(crate) dialogs: Box<dyn DesktopDialogs>,
    pub(crate) workspace: WorkspaceState,
    /// paint feature のサブ状態 (ペイント実行・履歴・CPU 表示キャッシュ)。
    pub(crate) paint: PaintState,
    /// コマ作成 (KomaRect) ジェスチャの進行中状態 (BL-081)。app 内部状態。
    koma_gesture: KomaGesture,
    pub(crate) layout: Option<DesktopLayout>,
    /// Phase 9E-4: ステータスバー (HtmlPanelView GPU 描画)。app 内部状態。
    status_bar: crate::features::status_bar::StatusBar,
    /// 次フレームで消化される提示無効化状態 (保留 dirty rect・再構築フラグ)。app 内部状態。
    invalidation: invalidation::PresentInvalidation,
    pub(crate) snapshots: DocumentSnapshotStore,
    pub(crate) panel_interaction: PanelInteractionState,
    /// 進行中のバックグラウンドジョブ (project save 等)。app 内部状態。
    background_jobs: Vec<background_tasks::BackgroundJob>,
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

/// `DesktopApp` 構築の依存ポートとパスをまとめた options (D13)。
///
/// 旧 telescoping constructor
/// (`new_with_dialogs_session_path_and_workspace_preset_path`) を置換し、
/// 引数順序への依存を排除する。
pub(crate) struct DesktopAppOptions {
    pub(crate) project_path: PathBuf,
    pub(crate) dialogs: Box<dyn DesktopDialogs>,
    pub(crate) session_path: PathBuf,
    pub(crate) workspace_preset_path: PathBuf,
    pub(crate) canvas_size_preset_path: PathBuf,
}

impl DesktopApp {
    pub(crate) fn new(project_path: PathBuf) -> Self {
        Self::with_options(DesktopAppOptions {
            project_path,
            dialogs: Box::new(NativeDesktopDialogs),
            session_path: default_desktop_session_path(),
            workspace_preset_path: default_workspace_preset_path(),
            canvas_size_preset_path: crate::platform::default_canvas_size_preset_path(),
        })
    }

    pub(crate) fn with_options(options: DesktopAppOptions) -> Self {
        let DesktopAppOptions {
            project_path,
            dialogs,
            session_path,
            workspace_preset_path,
            canvas_size_preset_path,
        } = options;
        let bootstrap = Self::bootstrap_state(project_path, &session_path, &workspace_preset_path);

        let mut app = Self {
            document: bootstrap.document,
            panel_runtime: bootstrap.panel_runtime,
            panel_workspace: bootstrap.panel_workspace,
            paths: ProjectPaths::new(
                bootstrap.project_path,
                session_path,
                workspace_preset_path,
                canvas_size_preset_path,
            ),
            dialogs,
            workspace: WorkspaceState {
                presets: bootstrap.workspace_presets,
                active_preset_id: bootstrap.active_workspace_preset_id,
            },
            paint: PaintState::new(),
            koma_gesture: KomaGesture::default(),
            layout: None,
            status_bar: crate::features::status_bar::StatusBar::new(),
            invalidation: invalidation::PresentInvalidation::at_startup(),
            snapshots: DocumentSnapshotStore::default(),
            panel_interaction: PanelInteractionState::default(),
            background_jobs: Vec::new(),
            gpu: None,
        };
        app.refresh_cpu_canvas_snapshot();
        app.ensure_workspace_presets_file(&app.paths.workspace_preset_path);
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
        let ctx = gpu_paint::GpuCanvasContext::new(device.clone(), queue.clone());
        self.gpu = Some(GpuPaintEngine {
            pool: gpu_paint::LayerTextureStore::new(device, queue),
            brush: gpu_paint::BrushPipeline::new(&ctx),
            fill: gpu_paint::FillPipeline::new(&ctx),
            compositor: gpu_paint::CompositePipeline::new(&ctx),
        });
        self.sync_all_layers_to_gpu();
        self.recomposite_all_komas();
    }

    /// 全ページ・全パネル・全レイヤーの CPU ビットマップを GPU テクスチャへ同期する。
    /// マスクと composite テクスチャも同期する。
    ///
    /// コマ集合が変わった場合 (新規ドキュメント・コマ追加削除・ロード) にのみ呼ぶ。
    /// アクティブコマのレイヤー構成だけが変わった場合は差分同期
    /// (`sync_active_koma_layers_to_gpu`) を使い、全転送を避ける (BL-117)。
    pub(crate) fn sync_all_layers_to_gpu(&mut self) {
        if self.gpu.is_none() {
            return;
        }
        let koma_ids: Vec<KomaId> = self
            .document
            .work
            .pages
            .iter()
            .flat_map(|page| page.komas.iter().map(|koma| koma.id))
            .collect();
        for koma_id in koma_ids {
            self.sync_koma_layers_to_gpu(koma_id);
        }
    }

    /// 指定コマのレイヤー/マスク/合成テクスチャを GPU へ差分同期する (BL-117)。
    ///
    /// 当該コマの既存エントリのみをクリアしてから再登録するため、別コマの
    /// テクスチャには触れない。レイヤー追加/削除/並べ替えで古いインデックスが
    /// ずれるのを防ぐ。
    pub(crate) fn sync_koma_layers_to_gpu(&mut self, koma_id: KomaId) {
        if self.gpu.is_none() {
            return;
        }
        let koma_id_str = koma_id.0.to_string();
        let Some(koma) = self
            .document
            .work
            .pages
            .iter()
            .flat_map(|page| &page.komas)
            .find(|koma| koma.id == koma_id)
        else {
            return;
        };
        let composite_size = (
            koma.composite_cache.width as u32,
            koma.composite_cache.height as u32,
        );
        let layers: Vec<gpu_paint::LayerUpload<'_>> = koma
            .layers
            .iter()
            .map(|layer| gpu_paint::LayerUpload {
                width: layer.bitmap.width as u32,
                height: layer.bitmap.height as u32,
                pixels: &layer.bitmap.pixels,
                mask: layer
                    .mask
                    .as_ref()
                    .map(|m| (m.width as u32, m.height as u32, m.alpha.as_slice())),
            })
            .collect();
        let pool = &mut self.gpu.as_mut().unwrap().pool;
        pool.sync_koma_layers(&koma_id_str, composite_size, &layers);
    }

    /// アクティブコマのレイヤーを GPU へ差分同期し、当該コマを再合成する (BL-117)。
    ///
    /// レイヤー追加/削除/並べ替えのようにアクティブコマのレイヤー構成のみが
    /// 変わった場合に使い、`sync_all_layers_to_gpu` の全ページ全コマ全転送を避ける。
    pub(crate) fn sync_active_koma_layers_to_gpu(&mut self) {
        let Some(koma_id) = self.document.active_koma().map(|koma| koma.id) else {
            return;
        };
        self.sync_koma_layers_to_gpu(koma_id);
        self.recomposite_koma(koma_id, None);
    }

    /// GPU テクスチャをキャンバスの表示正本として使えるかどうかを返す。
    ///
    /// `true` のとき: GPU リソースが揃っており、`canvas_surface_source_kind` が
    /// `Gpu` / `GpuComposite` のいずれかを返せる状態。
    /// 現在はテストからのみ参照される (production コードは `canvas_surface_source_kind()` を直接使う)。
    #[cfg(test)]
    pub(crate) fn should_use_gpu_canvas_source(&self) -> bool {
        self.canvas_surface_source_kind().is_some()
    }

    /// アクティブパネルに対してどの GPU ソースを使うべきかを返す。
    ///
    /// - 単一レイヤー: `Single` → `CanvasSurfaceSource::Gpu { layer_index: 0 }`
    /// - 複数レイヤー: `Composite` → `CanvasSurfaceSource::GpuComposite`
    /// - GPU 非対応: `None`
    pub(crate) fn canvas_surface_source_kind(&self) -> Option<GpuCanvasSourceKind> {
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
        // クランプ後に空矩形なら entries 収集と dispatch を省略する
        // (recomposite 側でも再クランプ + 空チェックされるが、ここで早期 return すると
        //  無駄な entries collect を避けられる)。
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

        gpu.compositor.recomposite(composite, &entries, rect);
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
        crate::platform::default_session_path()
    }
}
