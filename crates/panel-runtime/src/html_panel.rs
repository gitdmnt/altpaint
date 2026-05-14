//! HTML/CSS パネルプラグイン（Blitz + vello GPU 直描画版）。
//!
//! - `panel.meta.json` + `panel.html` + 任意 `panel.css` を読み込み
//! - 既存 `PanelPlugin` トレイトを満たす（`panel_tree()` は空ツリー、UI レンダ経路は別）
//! - `render_gpu()` は外部所有の `wgpu::Device`/`Queue`/`vello::Renderer` を借りて
//!   altpaint 所有の wgpu テクスチャに直接描画する（CPU readback なし）
//! - dirty 判定: `apply_bindings` が DOM mutation を起こした or サイズ変化 or 初回

use app_core::{Command, Document};
use panel_api::{HostAction, PanelNode, PanelPlugin, PanelTree, ServiceRequest};
use panel_html_experiment::blitz_dom::{BaseDocument, LocalName, local_name};
use panel_html_experiment::blitz_dom::node::NodeData;
use panel_html_experiment::{
    ActionDescriptor, HtmlPanelEngine, PanelGpuTarget, RenderOutcome as EngineRenderOutcome,
    RenderedPanelHit, parse_data_action,
};
use panel_html_experiment::blitz_traits::events::UiEvent;
use panel_html_experiment::vello;
use panel_html_experiment::wgpu;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

pub struct HtmlPanelPlugin {
    id: &'static str,
    title: &'static str,
    default_size: (u32, u32),
    engine: HtmlPanelEngine,
}

#[derive(Debug, thiserror::Error)]
pub enum HtmlPanelLoadError {
    #[error("panel metadata missing: {0}")]
    MissingMeta(PathBuf),
    #[error("panel html missing: {0}")]
    MissingHtml(PathBuf),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("meta json error: {0}")]
    MetaJson(#[from] serde_json::Error),
}

// `render_gpu` の戻り値型は Engine の `RenderOutcome` をそのまま使う。
// 上位は `panel_html_experiment::RenderOutcome` から直接 import する。

impl HtmlPanelPlugin {
    /// パネルディレクトリを読み込み、`restored_size` があれば measured_size として復元する。
    /// `restored_size = None` の場合は meta.json の `default_size` で初期化する。
    pub fn load(
        directory: &Path,
        restored_size: Option<(u32, u32)>,
    ) -> Result<Self, HtmlPanelLoadError> {
        let meta_path = directory.join("panel.meta.json");
        let html_path = directory.join("panel.html");
        let css_path = directory.join("panel.css");

        if !meta_path.exists() {
            return Err(HtmlPanelLoadError::MissingMeta(meta_path));
        }
        if !html_path.exists() {
            return Err(HtmlPanelLoadError::MissingHtml(html_path));
        }

        let meta_raw = std::fs::read_to_string(&meta_path)?;
        let meta: crate::meta::PanelMeta = serde_json::from_str(&meta_raw)?;
        let default_size = meta.default_size.as_tuple();

        let html = std::fs::read_to_string(&html_path)?;
        let css = if css_path.exists() {
            std::fs::read_to_string(&css_path)?
        } else {
            String::new()
        };
        let mut engine = HtmlPanelEngine::new(&html, &css);
        engine.on_load(restored_size.unwrap_or(default_size));

        Ok(Self {
            id: Box::leak(meta.id.into_boxed_str()),
            title: Box::leak(meta.title.into_boxed_str()),
            default_size,
            engine,
        })
    }

    pub fn from_parts(
        id: &'static str,
        title: &'static str,
        html: &str,
        css: &str,
        restored_size: Option<(u32, u32)>,
    ) -> Self {
        Self::from_parts_with_default(id, title, html, css, restored_size, (300, 220))
    }

    pub fn from_parts_with_default(
        id: &'static str,
        title: &'static str,
        html: &str,
        css: &str,
        restored_size: Option<(u32, u32)>,
        default_size: (u32, u32),
    ) -> Self {
        let mut engine = HtmlPanelEngine::new(html, css);
        engine.on_load(restored_size.unwrap_or(default_size));
        Self {
            id,
            title,
            default_size,
            engine,
        }
    }

    /// panel.meta.json の `default_size` を返す。
    /// 起動時に workspace に未記録のパネルへ初期サイズとして注入される。
    pub fn default_size(&self) -> (u32, u32) {
        self.default_size
    }

    pub fn engine(&self) -> &HtmlPanelEngine {
        &self.engine
    }

    /// 9E-3: `PanelRuntime::render_panels` から共通アクセサ経由で呼ばれる。
    pub(crate) fn engine_mut(&mut self) -> &mut HtmlPanelEngine {
        &mut self.engine
    }

    pub fn gpu_target(&self) -> Option<&PanelGpuTarget> {
        self.engine.gpu_target()
    }

    /// パネルの権威サイズ (chrome 含む全体)。
    pub fn measured_size(&self) -> (u32, u32) {
        self.engine.measured_size()
    }

    /// 永続化されたサイズで measured_size を上書きする（起動時 restore 経路）。
    pub fn restore_size(&mut self, size: (u32, u32)) {
        self.engine.on_load(size);
    }

    /// UI 入力イベントを Blitz に転送する。`:hover`/`<details>` 等の動的レイアウトを動かす。
    pub fn forward_input(&mut self, event: UiEvent) -> bool {
        self.engine.on_input(event)
    }

    /// `data-action` 要素の絶対矩形を返す。`render_gpu` 後に呼ぶこと（layout 解決済み前提）。
    pub fn collect_action_rects(&self) -> Vec<RenderedPanelHit> {
        self.engine.collect_action_rects()
    }

    /// Engine の `on_render` への薄いラッパ。
    /// `viewport` は画面側上限 (パネルがそれを超えないようクランプされる)。
    #[allow(clippy::too_many_arguments)]
    pub fn render_gpu<'a>(
        &'a mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        renderer: &mut vello::Renderer,
        scene_buf: &mut vello::Scene,
        viewport: (u32, u32),
        scale: f32,
        chrome_height: u32,
    ) -> EngineRenderOutcome<'a> {
        self.engine
            .on_render(device, queue, renderer, scene_buf, viewport, scale, chrome_height)
    }

    fn build_host_snapshot(
        &self,
        can_undo: bool,
        can_redo: bool,
        active_jobs: usize,
        snapshot_count: usize,
    ) -> Value {
        json!({
            "host": { "can_undo": can_undo, "can_redo": can_redo },
            "jobs": { "active": active_jobs },
            "snapshots": { "count": snapshot_count },
        })
    }

    /// `<button data-action="...">` を `PanelNode::Button` 列に翻訳する。
    /// パネル GPU 描画は別経路だが、`PanelEvent::Activate` の解決には panel-api の既定実装が
    /// 翻訳ツリーを辿るため、最小限の Button ノード列を提供する必要がある。
    fn translate_buttons(&self) -> Vec<PanelNode> {
        let document = self.engine.document();
        let Ok(ids) = document.query_selector_all("[data-action]") else {
            return Vec::new();
        };
        ids.into_iter()
            .filter_map(|node_id| translate_button_node(document, node_id))
            .collect()
    }
}

fn translate_button_node(document: &BaseDocument, node_id: usize) -> Option<PanelNode> {
    let node = document.get_node(node_id)?;
    let NodeData::Element(element) = &node.data else {
        return None;
    };
    let id = element.attr(local_name!("id"))?.to_string();
    let raw_action = element.attr(LocalName::from("data-action"))?;
    let raw_args = element.attr(LocalName::from("data-args"));
    let descriptor = parse_data_action(raw_action, raw_args).ok()?;
    let action = descriptor_to_host_action(descriptor)?;
    let label = collect_text_content(document, node_id);
    Some(PanelNode::Button {
        id,
        label,
        action,
        active: false,
        fill_color: None,
    })
}

fn collect_text_content(document: &BaseDocument, node_id: usize) -> String {
    let mut out = String::new();
    fn walk(doc: &BaseDocument, id: usize, out: &mut String) {
        let Some(node) = doc.get_node(id) else { return };
        match &node.data {
            NodeData::Text(t) => out.push_str(&t.content),
            _ => {
                for child_id in &node.children {
                    walk(doc, *child_id, out);
                }
            }
        }
    }
    walk(document, node_id, &mut out);
    out
}

fn descriptor_to_host_action(descriptor: ActionDescriptor) -> Option<HostAction> {
    match descriptor {
        ActionDescriptor::Command { id, .. } => command_from_id(&id).map(HostAction::DispatchCommand),
        ActionDescriptor::Service { name, payload } => {
            let mut request = ServiceRequest::new(name);
            for (k, v) in payload {
                request = request.with_value(k, v);
            }
            Some(HostAction::RequestService(request))
        }
        // HtmlPanelPlugin (純 HTML パネル) では altp: descriptor は使われない。
        // DSL 翻訳結果が出力するもので、`DslPanelPlugin` 側で解決する。
        ActionDescriptor::Altp { .. } => None,
    }
}

fn command_from_id(command_id: &str) -> Option<Command> {
    match command_id {
        "noop" => Some(Command::Noop),
        _ => None,
    }
}

impl PanelPlugin for HtmlPanelPlugin {
    fn id(&self) -> &'static str {
        self.id
    }

    fn title(&self) -> &'static str {
        self.title
    }

    fn update(
        &mut self,
        _document: &Document,
        can_undo: bool,
        can_redo: bool,
        active_jobs: usize,
        snapshot_count: usize,
    ) {
        let snapshot = self.build_host_snapshot(can_undo, can_redo, active_jobs, snapshot_count);
        self.engine.on_host_snapshot(&snapshot);
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }

    /// HTML パネルは GPU 直描画なので DSL レンダリング経路に乗せない。
    /// `children` 空ツリーを返し、ui-shell 側で「空ツリーは rasterize スキップ」させる。
    fn panel_tree(&self) -> PanelTree {
        PanelTree {
            id: self.id,
            title: self.title,
            children: Vec::new(),
        }
    }

    /// `Activate` イベントを HTML 内 `<button id=...>` の `data-action` で解決する。
    /// 既定実装は `panel_tree` を辿るが本パネルでは空ツリーなので独自実装が必要。
    fn handle_event(&mut self, event: &panel_api::PanelEvent) -> Vec<HostAction> {
        match event {
            panel_api::PanelEvent::Activate { panel_id, node_id } if panel_id == self.id => {
                for button in self.translate_buttons() {
                    if let PanelNode::Button { id, action, .. } = button
                        && id == *node_id
                    {
                        return vec![action];
                    }
                }
                Vec::new()
            }
            _ => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use panel_api::services::names;
    use std::sync::{Mutex, MutexGuard, OnceLock};

    fn make_plugin(html: &str, css: &str) -> HtmlPanelPlugin {
        HtmlPanelPlugin::from_parts("test.html_panel", "Test", html, css, None)
    }

    /// 複数 GPU テストが同時に wgpu Adapter / Device を要求すると Windows 環境で
    /// 不安定になるため、本モジュール内の GPU テストを直列化する。
    fn gpu_test_lock() -> MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|e| e.into_inner())
    }

    fn try_init_device() -> Option<(wgpu::Device, wgpu::Queue)> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .ok()?;
        let limits = adapter.limits();
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("html-panel-test-device"),
            required_features: wgpu::Features::empty(),
            required_limits: limits, // adapter のデフォルトを使う（vello が複数 storage buffer を必要とするため）
            memory_hints: wgpu::MemoryHints::default(),
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
            trace: wgpu::Trace::Off,
        }))
        .ok()?;
        Some((device, queue))
    }

    fn make_renderer(device: &wgpu::Device) -> vello::Renderer {
        vello::Renderer::new(
            device,
            vello::RendererOptions {
                use_cpu: false,
                num_init_threads: None,
                antialiasing_support: vello::AaSupport::area_only(),
                pipeline_cache: None,
            },
        )
        .expect("vello renderer")
    }

    /// S9: data-action="service:..." が Activate イベントで RequestService として解決される
    #[test]
    fn data_action_service_translates_to_request_service_with_payload() {
        let html = r#"<html><body><button id="save" data-action="service:project_io.save_current" data-args='{"attempt":1}'>Save</button></body></html>"#;
        let mut plugin = make_plugin(html, "");
        let actions = plugin.handle_event(&panel_api::PanelEvent::Activate {
            panel_id: "test.html_panel".to_string(),
            node_id: "save".to_string(),
        });
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            HostAction::RequestService(request) => {
                assert_eq!(request.name, names::PROJECT_SAVE_CURRENT);
                assert_eq!(request.u64("attempt"), Some(1));
            }
            other => panic!("expected RequestService, got {other:?}"),
        }
    }

    #[test]
    fn data_action_command_noop_translates() {
        let html = r#"<html><body><button id="x" data-action="command:noop">X</button></body></html>"#;
        let mut plugin = make_plugin(html, "");
        let actions = plugin.handle_event(&panel_api::PanelEvent::Activate {
            panel_id: "test.html_panel".to_string(),
            node_id: "x".to_string(),
        });
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            HostAction::DispatchCommand(cmd) => {
                assert_eq!(*cmd, Command::Noop);
            }
            _ => panic!("expected DispatchCommand"),
        }
    }

    /// S11': panel_tree は HTML パネルでは空ツリー（DSL レンダ経路に乗せない契約）
    #[test]
    fn panel_tree_is_empty_for_html_panel() {
        let html = r#"<html><body><button id="x" data-action="command:noop">X</button></body></html>"#;
        let plugin = make_plugin(html, "");
        let tree = plugin.panel_tree();
        assert!(tree.children.is_empty());
    }

    /// 既定の handle_event が button id でアクション解決できる
    #[test]
    fn activate_event_resolves_button_action() {
        let html = r#"<html><body><button id="app.save" data-action="service:project_io.save_current">Save</button></body></html>"#;
        let mut plugin = make_plugin(html, "");
        let actions = plugin.handle_event(&panel_api::PanelEvent::Activate {
            panel_id: "test.html_panel".to_string(),
            node_id: "app.save".to_string(),
        });
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            HostAction::RequestService(req) => {
                assert_eq!(req.name, names::PROJECT_SAVE_CURRENT);
            }
            _ => panic!("expected RequestService"),
        }
    }

    /// S5: red CSS を render_gpu すると texture に red pixel が書き込まれる
    #[test]
    fn gpu_html_panel_renders_red_pixel_when_css_red_background() {
        let _guard = gpu_test_lock();
        let Some((device, queue)) = try_init_device() else {
            eprintln!("skip: no GPU device");
            return;
        };
        let mut renderer = make_renderer(&device);
        let mut scene = vello::Scene::new();
        let html = r#"<html><body style="margin:0"><div style="width:60px;height:30px;background:#ff0000;"></div></body></html>"#;
        let mut plugin = make_plugin(html, "");
        let outcome = plugin.render_gpu(&device, &queue, &mut renderer, &mut scene, (200, 100), 1.0, 0);
        assert!(outcome.is_rendered());
        let target = outcome.target();
        // texture から readback して red pixel を検出
        let pixels = readback_rgba(&device, &queue, &target.texture, target.width, target.height);
        let red = pixels
            .chunks_exact(4)
            .any(|p| p[0] > 200 && p[1] < 50 && p[2] < 50 && p[3] > 200);
        assert!(red, "expected red pixel from CSS background");
    }

    /// S6 + S12: dirty でない 2 回目の render_gpu は Skipped
    #[test]
    fn gpu_html_panel_render_outcome_is_skipped_when_not_dirty() {
        let _guard = gpu_test_lock();
        let Some((device, queue)) = try_init_device() else {
            eprintln!("skip: no GPU device");
            return;
        };
        let mut renderer = make_renderer(&device);
        let mut scene = vello::Scene::new();
        let html = r#"<html><body style="margin:0"><div style="width:40px;height:20px;background:#00ff00;"></div></body></html>"#;
        let mut plugin = make_plugin(html, "");
        let first = plugin.render_gpu(&device, &queue, &mut renderer, &mut scene, (200, 100), 1.0, 0);
        assert!(first.is_rendered());
        let second = plugin.render_gpu(&device, &queue, &mut renderer, &mut scene, (200, 100), 1.0, 0);
        assert!(!second.is_rendered(), "second call should be Skipped");
    }

    /// D2: 白背景に黒で 'A' を描画 → readback で暗いピクセルが存在することを確認。
    /// 失敗 → 原因 (C) AA / blend / clip 経路。
    #[test]
    fn ascii_text_renders_dark_pixels_in_text_rect() {
        let _guard = gpu_test_lock();
        let Some((device, queue)) = try_init_device() else {
            eprintln!("skip: no GPU device");
            return;
        };
        let mut renderer = make_renderer(&device);
        let mut scene = vello::Scene::new();
        let html = r#"<html><body style="margin:0;background:#ffffff;color:#000000;font-size:48px"><span>A</span></body></html>"#;
        let mut plugin = make_plugin(html, "");
        let outcome = plugin.render_gpu(&device, &queue, &mut renderer, &mut scene, (256, 256), 1.0, 0);
        assert!(outcome.is_rendered());
        let target = outcome.target();
        let pixels = readback_rgba(&device, &queue, &target.texture, target.width, target.height);
        let dark = pixels
            .chunks_exact(4)
            .filter(|p| (p[0] as u32 + p[1] as u32 + p[2] as u32) < 150 && p[3] > 100)
            .count();
        assert!(
            dark >= 50,
            "expected at least 50 dark pixels for ASCII glyph, got {dark}"
        );
    }

    /// D3: 日本語 'あ' を描画 → 暗いピクセル検出。
    /// ASCII (D2) は通るが D3 が失敗する場合 → 原因 (B) 日本語フォントフォールバック未設定。
    #[test]
    fn japanese_text_renders_dark_pixels() {
        let _guard = gpu_test_lock();
        let Some((device, queue)) = try_init_device() else {
            eprintln!("skip: no GPU device");
            return;
        };
        let mut renderer = make_renderer(&device);
        let mut scene = vello::Scene::new();
        let html = r#"<html><body style="margin:0;background:#ffffff;color:#000000;font-size:48px"><span>あ</span></body></html>"#;
        let mut plugin = make_plugin(html, "");
        let outcome = plugin.render_gpu(&device, &queue, &mut renderer, &mut scene, (256, 256), 1.0, 0);
        assert!(outcome.is_rendered());
        let target = outcome.target();
        let pixels = readback_rgba(&device, &queue, &target.texture, target.width, target.height);
        let dark = pixels
            .chunks_exact(4)
            .filter(|p| (p[0] as u32 + p[1] as u32 + p[2] as u32) < 150 && p[3] > 100)
            .count();
        assert!(
            dark >= 50,
            "expected at least 50 dark pixels for Japanese glyph, got {dark}"
        );
    }

    /// D4: 実 panel.html + panel.css を描画して、ボタンのテキスト色 (`#f3f7ff` 系) が
    /// `app.undo` 矩形の中央付近で観測されることを確認する。
    #[test]
    fn full_panel_html_renders_visible_text() {
        let _guard = gpu_test_lock();
        let Some((device, queue)) = try_init_device() else {
            eprintln!("skip: no GPU device");
            return;
        };
        let html = include_str!("../../builtin-panels/app-actions/panel.html");
        let css = include_str!("../../builtin-panels/app-actions/panel.css");
        let mut renderer = make_renderer(&device);
        let mut scene = vello::Scene::new();
        let mut plugin = make_plugin(html, css);
        // 大きな viewport を渡して HTML の自然サイズで target が作られる
        let outcome = plugin.render_gpu(&device, &queue, &mut renderer, &mut scene, (1024, 1024), 1.0, 0);
        assert!(outcome.is_rendered());
        let target_width = outcome.target().width;
        let target_height = outcome.target().height;
        let pixels = readback_rgba(&device, &queue, &outcome.target().texture, target_width, target_height);

        let undo_rect = plugin
            .collect_action_rects()
            .into_iter()
            .find(|r| r.element_id.as_deref() == Some("app.undo"))
            .expect("expected app.undo data-action element");
        let cx = undo_rect.rect.x + undo_rect.rect.width / 2;
        let cy = undo_rect.rect.y + undo_rect.rect.height / 2;
        let half = 16i32;
        let bright = (cx as i32 - half..=cx as i32 + half)
            .flat_map(|x| (cy as i32 - half..=cy as i32 + half).map(move |y| (x, y)))
            .filter(|&(x, y)| x >= 0 && y >= 0 && (x as u32) < target_width && (y as u32) < target_height)
            .filter(|&(x, y)| {
                let i = ((y as u32 * target_width + x as u32) * 4) as usize;
                pixels[i] > 200 && pixels[i + 1] > 200 && pixels[i + 2] > 200 && pixels[i + 3] > 100
            })
            .count();
        assert!(
            bright >= 30,
            "expected at least 30 bright text pixels around app.undo center, got {bright}"
        );
    }

    /// 非回帰: 既存の枠（`.panel` 背景色 #181c24）が描画されたままであることを確認する。
    #[test]
    fn panel_background_color_is_preserved() {
        let _guard = gpu_test_lock();
        let Some((device, queue)) = try_init_device() else {
            eprintln!("skip: no GPU device");
            return;
        };
        let html = include_str!("../../builtin-panels/app-actions/panel.html");
        let css = include_str!("../../builtin-panels/app-actions/panel.css");
        let mut renderer = make_renderer(&device);
        let mut scene = vello::Scene::new();
        let mut plugin = make_plugin(html, css);
        let outcome = plugin.render_gpu(&device, &queue, &mut renderer, &mut scene, (1024, 1024), 1.0, 0);
        assert!(outcome.is_rendered());
        let target = outcome.target();
        let pixels = readback_rgba(&device, &queue, &target.texture, target.width, target.height);

        // .panel の背景 #181c24 (24, 28, 36) がパネル内のどこかに観測できることを確認する。
        // 自然サイズに合わせて target が作られるため、サンプル座標は %相対で取り直す。
        let expected = (0x18u8, 0x1cu8, 0x24u8);
        let w = target.width;
        let h = target.height;
        let sample_points: Vec<(u32, u32)> = [
            (10, 20),       // 左上付近
            (w / 2, h / 4), // 中央上
            (w / 4, h / 2), // 中央左
        ]
        .into_iter()
        .filter(|&(x, y)| x < w && y < h)
        .collect();
        let matches = sample_points
            .iter()
            .filter(|&&(x, y)| {
                let i = ((y * w + x) * 4) as usize;
                let (r, g, b) = (pixels[i], pixels[i + 1], pixels[i + 2]);
                let dr = (r as i32 - expected.0 as i32).abs();
                let dg = (g as i32 - expected.1 as i32).abs();
                let db = (b as i32 - expected.2 as i32).abs();
                dr < 20 && dg < 20 && db < 20
            })
            .count();
        assert!(
            matches >= 1,
            "expected panel background ~(0x18,0x1c,0x24) at one of {sample_points:?} (target {w}x{h})",
        );
    }

    /// Phase 11: 永続化サイズ復元時に target がそのサイズで作られる
    #[test]
    fn gpu_html_panel_target_uses_restored_size() {
        let _guard = gpu_test_lock();
        let Some((device, queue)) = try_init_device() else {
            eprintln!("skip: no GPU device");
            return;
        };
        let mut renderer = make_renderer(&device);
        let mut scene = vello::Scene::new();
        let html = r#"<html><body style="margin:0"><div style="width:30px;height:15px;background:#ffffff;"></div></body></html>"#;
        let mut plugin = HtmlPanelPlugin::from_parts(
            "test.html_panel",
            "Test",
            html,
            "",
            Some((120, 60)),
        );
        // viewport は十分大きく取り、restored = (120, 60) がそのまま target サイズになることを確認。
        let first = plugin.render_gpu(&device, &queue, &mut renderer, &mut scene, (1024, 1024), 1.0, 0);
        assert!(first.is_rendered());
        let target = first.target();
        assert_eq!(
            (target.width, target.height),
            (120, 60),
            "target size should match restored_size, not content-driven"
        );
    }

    /// Phase 11 回帰防止: measured_size はコンテンツ自然サイズに追従しない。
    /// 自動サイズ追従撤去後、measured_size は on_load の引数で確定し、render 後も変化しない。
    #[test]
    fn gpu_html_panel_measured_size_does_not_drift_after_render() {
        let _guard = gpu_test_lock();
        let Some((device, queue)) = try_init_device() else {
            eprintln!("skip: no GPU device");
            return;
        };
        let mut renderer = make_renderer(&device);
        let mut scene = vello::Scene::new();
        // body コンテンツは (50, 25) しかないが measured_size は (320, 240) で確定する
        let html = r#"<html><body style="margin:0"><div style="width:50px;height:25px;background:#ff00ff;"></div></body></html>"#;
        let mut plugin = HtmlPanelPlugin::from_parts(
            "test.html_panel",
            "Test",
            html,
            "",
            Some((320, 240)),
        );
        for _ in 0..10 {
            let _ = plugin.render_gpu(&device, &queue, &mut renderer, &mut scene, (2048, 2048), 1.0, 0);
        }
        assert_eq!(
            plugin.measured_size(),
            (320, 240),
            "measured_size must not drift after repeated on_render",
        );
    }

    /// Phase 11 回帰防止: viewport クランプは描画用 local のみで行われ measured_size を変更しない。
    #[test]
    fn gpu_html_panel_viewport_clamp_does_not_mutate_measured_size() {
        let _guard = gpu_test_lock();
        let Some((device, queue)) = try_init_device() else {
            eprintln!("skip: no GPU device");
            return;
        };
        let mut renderer = make_renderer(&device);
        let mut scene = vello::Scene::new();
        let html = r#"<html><body style="margin:0"><div style="width:20px;height:10px;"></div></body></html>"#;
        let mut plugin = HtmlPanelPlugin::from_parts(
            "test.html_panel",
            "Test",
            html,
            "",
            Some((1000, 800)),
        );
        // viewport (400, 300) は measured_size (1000, 800) より小さい → 描画は (400, 300) に clamp
        let _ = plugin.render_gpu(&device, &queue, &mut renderer, &mut scene, (400, 300), 1.0, 0);
        assert_eq!(
            plugin.measured_size(),
            (1000, 800),
            "measured_size must not shrink during viewport clamp",
        );
    }

    fn readback_rgba(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        texture: &wgpu::Texture,
        width: u32,
        height: u32,
    ) -> Vec<u8> {
        // bytes per row must be multiple of 256
        let unpadded_bpr = width * 4;
        let align = 256u32;
        let padded_bpr = unpadded_bpr.div_ceil(align) * align;
        let buffer_size = (padded_bpr * height) as u64;
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: buffer_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_bpr),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        );
        queue.submit(std::iter::once(encoder.finish()));
        let slice = buffer.slice(..);
        let (sender, receiver) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| sender.send(r).unwrap());
        device.poll(wgpu::PollType::wait_indefinitely()).expect("poll");
        receiver.recv().unwrap().expect("map_async");
        let data = slice.get_mapped_range();
        let mut out = Vec::with_capacity((unpadded_bpr * height) as usize);
        for row in 0..height {
            let start = (row * padded_bpr) as usize;
            out.extend_from_slice(&data[start..start + unpadded_bpr as usize]);
        }
        out
    }
}
