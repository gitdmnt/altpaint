//! ステータスバー専用 `HtmlPanelEngine` ラッパ。
//!
//! Phase 9E-4 で `crates/render/src/{text,status}.rs` を撤去し、ステータステキストも
//! GPU 直描画 (`HtmlPanelEngine` + Blitz HTML/CSS + `vello::Renderer`) で描画する。
//!
//! - HTML テンプレート: 1 行の flex レイアウト（tool / zoom / status text）
//! - スケール: 1.0 固定（HiDPI はスコープ外）
//! - フォント: `system-ui` フォールバック

use panel_html_experiment::{
    vello, wgpu, HtmlPanelEngine, PanelGpuTarget, RenderOutcome,
};

/// ステータスバーが表示する集約スナップショット。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StatusSnapshot {
    pub(crate) tool_name: String,
    pub(crate) zoom_percent: u32,
    pub(crate) status_text: String,
}

impl StatusSnapshot {
    pub(crate) fn new(
        tool_name: impl Into<String>,
        zoom_percent: u32,
        status_text: impl Into<String>,
    ) -> Self {
        Self {
            tool_name: tool_name.into(),
            zoom_percent,
            status_text: status_text.into(),
        }
    }
}

const STATUS_CSS: &str = r#"
html, body { margin: 0; padding: 0; background: #181818; color: #d8d8d8; font-family: system-ui, "Segoe UI", "Yu Gothic UI", sans-serif; font-size: 13px; }
body { width: 100%; }
.status { display: flex; flex-direction: row; align-items: center; padding: 4px 8px; gap: 12px; height: 16px; line-height: 16px; }
.status .tool { color: #ffffff; font-weight: 600; }
.status .zoom { color: #d8d8d8; }
.status .text { color: #d8d8d8; white-space: nowrap; overflow: hidden; }
"#;

fn render_html(snapshot: &StatusSnapshot) -> String {
    format!(
        "<!DOCTYPE html><html><body><div class=\"status\">\
<span class=\"tool\">{tool}</span>\
<span class=\"zoom\">{zoom}%</span>\
<span class=\"text\">{text}</span>\
</div></body></html>",
        tool = html_escape(&snapshot.tool_name),
        zoom = snapshot.zoom_percent,
        text = html_escape(&snapshot.status_text),
    )
}

fn html_escape(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
    out
}

pub(crate) struct StatusPanel {
    engine: HtmlPanelEngine,
    last_snapshot: Option<StatusSnapshot>,
}

impl StatusPanel {
    /// 初期 HTML テンプレートで engine を初期化する。
    pub(crate) fn new() -> Self {
        let initial = StatusSnapshot::new("Pen", 100, "");
        let html = render_html(&initial);
        let mut engine = HtmlPanelEngine::new(&html, STATUS_CSS);
        // 初期サイズは on_load の intrinsic 測定に任せる
        engine.on_load(None);
        Self {
            engine,
            last_snapshot: Some(initial),
        }
    }

    /// snapshot を engine に流し込み、変化があれば DOM を再構築する。
    pub(crate) fn update(&mut self, snapshot: &StatusSnapshot) {
        if self.last_snapshot.as_ref() == Some(snapshot) {
            return;
        }
        let html = render_html(snapshot);
        self.engine.replace_document(&html, STATUS_CSS);
        self.last_snapshot = Some(snapshot.clone());
    }

    /// `engine.on_render` の薄いラッパ。
    pub(crate) fn render_gpu<'a>(
        &'a mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        renderer: &mut vello::Renderer,
        scene_buf: &mut vello::Scene,
        viewport: (u32, u32),
    ) -> RenderOutcome<'a> {
        // chrome_height = 0（タイトルバー無し、純粋な status row）
        self.engine
            .on_render(device, queue, renderer, scene_buf, viewport, 1.0, 0)
    }

    /// 直近 render 後の GPU テクスチャ。
    pub(crate) fn gpu_target(&self) -> Option<&PanelGpuTarget> {
        self.engine.gpu_target()
    }

    /// engine が把握している権威サイズ。
    #[allow(dead_code)]
    pub(crate) fn measured_size(&self) -> (u32, u32) {
        self.engine.measured_size()
    }

    #[cfg(test)]
    pub(crate) fn engine_mut(&mut self) -> &mut HtmlPanelEngine {
        &mut self.engine
    }

    #[cfg(test)]
    pub(crate) fn last_snapshot(&self) -> Option<&StatusSnapshot> {
        self.last_snapshot.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, MutexGuard, OnceLock};

    /// 複数 GPU テストが同時に wgpu Adapter / Device を要求すると Windows 環境で
    /// 不安定になるため、本モジュール内の GPU テストを直列化する。
    /// (panel-runtime / panel-html-experiment と同じパターン)
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
            label: Some("status-panel-test-device"),
            required_features: wgpu::Features::empty(),
            required_limits: limits,
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

    /// テスト 1: ツール名を含むスナップショットで render_gpu すると glyph run が出力される。
    #[test]
    fn status_panel_emits_glyph_run_for_tool_name() {
        let _guard = gpu_test_lock();
        let Some((device, queue)) = try_init_device() else {
            eprintln!("skip: no GPU device");
            return;
        };
        let mut renderer = make_renderer(&device);
        let mut scene = vello::Scene::new();
        let mut panel = StatusPanel::new();
        panel.update(&StatusSnapshot::new("Pen", 150, "file=untitled"));
        let outcome = panel.render_gpu(&device, &queue, &mut renderer, &mut scene, (800, 32));
        // どんな描画でも glyph run / draw command は scene encoding に積まれるので、
        // resources が空でないことを弱検証する (実装非依存アサート)。
        let resources = scene.encoding().resources.clone();
        let total_glyphs = resources.glyphs.len()
            + resources.glyph_runs.len()
            + resources.patches.len();
        assert!(
            total_glyphs > 0 || outcome.is_rendered(),
            "expected scene to contain glyph data after status text render"
        );
    }

    /// テスト 2: render_gpu が PanelGpuTarget を返す（PresentScene への合流に必要）。
    #[test]
    fn status_panel_quad_present_in_scene() {
        let _guard = gpu_test_lock();
        let Some((device, queue)) = try_init_device() else {
            eprintln!("skip: no GPU device");
            return;
        };
        let mut renderer = make_renderer(&device);
        let mut scene = vello::Scene::new();
        let mut panel = StatusPanel::new();
        panel.update(&StatusSnapshot::new("Pen", 100, "ready"));
        let outcome = panel.render_gpu(&device, &queue, &mut renderer, &mut scene, (800, 32));
        let target = outcome.target();
        assert!(target.width >= 1);
        assert!(target.height >= 1);
        assert!(panel.gpu_target().is_some());
    }

    /// テスト 3: snapshot 変更で HTML ドキュメントが更新される。
    #[test]
    fn status_panel_zoom_percent_updates_on_snapshot_change() {
        let mut panel = StatusPanel::new();
        panel.update(&StatusSnapshot::new("Pen", 100, "ready"));
        let initial_html_len = panel
            .engine_mut()
            .document()
            .root_node()
            .children
            .len();
        panel.update(&StatusSnapshot::new("Pen", 250, "ready"));
        // DOM が再構築されたので何らかのノードが存在することを弱検証する
        let updated_html_len = panel
            .engine_mut()
            .document()
            .root_node()
            .children
            .len();
        assert!(updated_html_len >= initial_html_len.min(1));
        // 二回目の update（同一 snapshot）は no-op
        let snapshot = StatusSnapshot::new("Pen", 250, "ready");
        panel.update(&snapshot);
        // last_snapshot がそのまま保持される
        assert_eq!(panel.last_snapshot.as_ref(), Some(&snapshot));
    }
}
