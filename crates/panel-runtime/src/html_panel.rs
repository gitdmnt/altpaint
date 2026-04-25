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
    ActionDescriptor, HtmlPanelEngine, PanelGpuTarget, RenderedPanelHit, parse_data_action,
};
use panel_html_experiment::vello;
use panel_html_experiment::wgpu;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

pub struct HtmlPanelPlugin {
    id: &'static str,
    title: &'static str,
    engine: HtmlPanelEngine,
    target: Option<PanelGpuTarget>,
    render_dirty: bool,
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
    #[error("meta missing required field: {0}")]
    MetaField(&'static str),
}

/// `render_gpu` の結果。dirty なら `Rendered`、再利用なら `Skipped`。テスタブル化のための enum。
pub enum RenderOutcome<'a> {
    Rendered(&'a PanelGpuTarget),
    Skipped(&'a PanelGpuTarget),
}

impl<'a> RenderOutcome<'a> {
    pub fn target(&self) -> &PanelGpuTarget {
        match self {
            RenderOutcome::Rendered(t) | RenderOutcome::Skipped(t) => t,
        }
    }
    pub fn is_rendered(&self) -> bool {
        matches!(self, RenderOutcome::Rendered(_))
    }
}

impl HtmlPanelPlugin {
    pub fn load(directory: &Path) -> Result<Self, HtmlPanelLoadError> {
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
        let meta: Value = serde_json::from_str(&meta_raw)?;
        let id = meta
            .get("id")
            .and_then(Value::as_str)
            .ok_or(HtmlPanelLoadError::MetaField("id"))?;
        let title = meta
            .get("title")
            .and_then(Value::as_str)
            .ok_or(HtmlPanelLoadError::MetaField("title"))?;

        let html = std::fs::read_to_string(&html_path)?;
        let css = if css_path.exists() {
            std::fs::read_to_string(&css_path)?
        } else {
            String::new()
        };
        let engine = HtmlPanelEngine::new(&html, &css);

        Ok(Self {
            id: Box::leak(id.to_string().into_boxed_str()),
            title: Box::leak(title.to_string().into_boxed_str()),
            engine,
            target: None,
            render_dirty: true,
        })
    }

    pub fn from_parts(id: &'static str, title: &'static str, html: &str, css: &str) -> Self {
        let engine = HtmlPanelEngine::new(html, css);
        Self {
            id,
            title,
            engine,
            target: None,
            render_dirty: true,
        }
    }

    pub fn engine(&self) -> &HtmlPanelEngine {
        &self.engine
    }

    pub fn gpu_target(&self) -> Option<&PanelGpuTarget> {
        self.target.as_ref()
    }

    pub fn mark_render_dirty(&mut self) {
        self.render_dirty = true;
    }

    /// `data-action` 要素の絶対矩形を返す。`render_gpu` 後に呼ぶこと（layout 解決済み前提）。
    pub fn collect_action_rects(&self) -> Vec<RenderedPanelHit> {
        self.engine.collect_action_rects()
    }

    /// 共有 wgpu/vello で altpaint 所有の target テクスチャに描画する。
    #[allow(clippy::too_many_arguments)]
    pub fn render_gpu<'a>(
        &'a mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        renderer: &mut vello::Renderer,
        scene_buf: &mut vello::Scene,
        width: u32,
        height: u32,
        scale: f32,
    ) -> RenderOutcome<'a> {
        let size_changed = self
            .target
            .as_ref()
            .map(|t| t.width != width || t.height != height)
            .unwrap_or(true);
        if size_changed {
            self.target = Some(PanelGpuTarget::create(device, width, height));
            self.render_dirty = true;
        }
        if !self.render_dirty {
            return RenderOutcome::Skipped(self.target.as_ref().expect("target ensured"));
        }

        scene_buf.reset();
        self.engine.build_scene(scene_buf, width, height, scale);

        let target = self.target.as_ref().expect("target ensured");
        let view = target.create_render_view();
        renderer
            .render_to_texture(
                device,
                queue,
                scene_buf,
                &view,
                &vello::RenderParams {
                    base_color: vello::peniko::Color::TRANSPARENT,
                    width,
                    height,
                    antialiasing_method: vello::AaConfig::Area,
                },
            )
            .expect("vello render_to_texture failed");

        self.render_dirty = false;
        RenderOutcome::Rendered(self.target.as_ref().expect("target ensured"))
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
        self.engine.apply_bindings(&snapshot);
        if self.engine.document_dirty() {
            self.render_dirty = true;
        }
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

    fn make_plugin(html: &str, css: &str) -> HtmlPanelPlugin {
        HtmlPanelPlugin::from_parts("test.html_panel", "Test", html, css)
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
        let Some((device, queue)) = try_init_device() else {
            eprintln!("skip: no GPU device");
            return;
        };
        let mut renderer = make_renderer(&device);
        let mut scene = vello::Scene::new();
        let html = r#"<html><body><div style="width:60px;height:30px;background:#ff0000;"></div></body></html>"#;
        let mut plugin = make_plugin(html, "");
        let outcome = plugin.render_gpu(&device, &queue, &mut renderer, &mut scene, 100, 50, 1.0);
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
        let Some((device, queue)) = try_init_device() else {
            eprintln!("skip: no GPU device");
            return;
        };
        let mut renderer = make_renderer(&device);
        let mut scene = vello::Scene::new();
        let html = r#"<html><body><div style="width:40px;height:20px;background:#00ff00;"></div></body></html>"#;
        let mut plugin = make_plugin(html, "");
        let first = plugin.render_gpu(&device, &queue, &mut renderer, &mut scene, 80, 40, 1.0);
        assert!(first.is_rendered());
        let second = plugin.render_gpu(&device, &queue, &mut renderer, &mut scene, 80, 40, 1.0);
        assert!(!second.is_rendered(), "second call should be Skipped");
    }

    /// S7: サイズ変更で target が再作成される
    #[test]
    fn gpu_html_panel_target_recreated_on_resize() {
        let Some((device, queue)) = try_init_device() else {
            eprintln!("skip: no GPU device");
            return;
        };
        let mut renderer = make_renderer(&device);
        let mut scene = vello::Scene::new();
        let html = r#"<html><body><div style="width:30px;height:15px;background:#ffffff;"></div></body></html>"#;
        let mut plugin = make_plugin(html, "");
        let first = plugin.render_gpu(&device, &queue, &mut renderer, &mut scene, 80, 40, 1.0);
        assert_eq!(first.target().width, 80);
        let second = plugin.render_gpu(&device, &queue, &mut renderer, &mut scene, 120, 60, 1.0);
        assert!(second.is_rendered(), "resize should re-render");
        assert_eq!(second.target().width, 120);
        assert_eq!(second.target().height, 60);
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
