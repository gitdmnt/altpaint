use crate::command_from_descriptor;
use app_core::{Command, Document};
use panel_api::{PanelPlugin, PanelTree};
use panel_schema::CommandDescriptor;
use serde_json::Value;

use crate::registry::PanelRuntime;

/// コマンド mapping supports ビュー ズーム が期待どおりに動作することを検証する。
#[test]
fn command_mapping_supports_view_zoom() {
    let mut descriptor = CommandDescriptor::new("view.zoom");
    descriptor
        .payload
        .insert("zoom".to_string(), Value::String("1.5".to_string()));

    assert_eq!(
        command_from_descriptor(&descriptor),
        Ok(Command::SetViewZoom { zoom: 1.5 })
    );
}

// ─── dirty panel テスト用モックパネル ───────────────────────────────────────

struct MockPanel {
    id: &'static str,
    counter: u32,
}

impl MockPanel {
    fn new(id: &'static str) -> Self {
        Self { id, counter: 0 }
    }
}

impl PanelPlugin for MockPanel {
    fn id(&self) -> &'static str {
        self.id
    }
    fn title(&self) -> &'static str {
        "Mock"
    }
    fn update(
        &mut self,
        _document: &Document,
        _can_undo: bool,
        _can_redo: bool,
        _active_jobs: usize,
        _snapshot_count: usize,
    ) {
        self.counter += 1;
    }
    fn panel_tree(&self) -> PanelTree {
        PanelTree {
            id: self.id,
            title: self.title(),
            children: vec![panel_api::PanelNode::Text {
                id: format!("counter.{}", self.counter),
                text: format!("{}", self.counter),
            }],
        }
    }
}

/// `mark_dirty` を呼ばなければ `sync_dirty_panels` は何もしない。
#[test]
fn sync_dirty_panels_skips_panels_not_marked_dirty() {
    let mut runtime = PanelRuntime::new();
    runtime.register_panel(Box::new(MockPanel::new("panel.a")));
    // register_panel は自動で dirty にするので、一度 sync して dirty をクリア
    let doc = Document::new(1, 1);
    let _ = runtime.sync_dirty_panels(&doc, false, false, 0, 0);
    assert!(!runtime.has_dirty_panels());

    // dirty にせず sync → 何も変わらない
    let changed = runtime.sync_dirty_panels(&doc, false, false, 0, 0);
    assert!(changed.is_empty());
}

/// `mark_dirty` で指定したパネルだけが sync される。
#[test]
fn mark_dirty_causes_only_that_panel_to_be_synced() {
    let mut runtime = PanelRuntime::new();
    runtime.register_panel(Box::new(MockPanel::new("panel.a")));
    runtime.register_panel(Box::new(MockPanel::new("panel.b")));
    // 初回 sync でクリア
    let doc = Document::new(1, 1);
    let _ = runtime.sync_dirty_panels(&doc, false, false, 0, 0);

    // panel.a のみ dirty にして sync
    runtime.mark_dirty("panel.a");
    assert_eq!(runtime.dirty_panel_count(), 1);
    let changed = runtime.sync_dirty_panels(&doc, false, false, 0, 0);

    // panel.a は counter が変わるので changed に入る（tree が変わる）
    assert!(changed.contains("panel.a"), "panel.a should be in changed");
    // panel.b は sync されていないので changed に入らない
    assert!(!changed.contains("panel.b"), "panel.b should not be in changed");
    // sync 後は dirty クリア
    assert!(!runtime.has_dirty_panels());
}

/// `mark_all_dirty` は登録済みの全パネルを dirty にする。
#[test]
fn mark_all_dirty_marks_every_registered_panel() {
    let mut runtime = PanelRuntime::new();
    runtime.register_panel(Box::new(MockPanel::new("panel.a")));
    runtime.register_panel(Box::new(MockPanel::new("panel.b")));
    runtime.register_panel(Box::new(MockPanel::new("panel.c")));
    // 初回 sync でクリア
    let doc = Document::new(1, 1);
    let _ = runtime.sync_dirty_panels(&doc, false, false, 0, 0);
    assert!(!runtime.has_dirty_panels());

    runtime.mark_all_dirty();
    assert_eq!(runtime.dirty_panel_count(), 3);
    assert!(runtime.has_dirty_panels());

    let changed = runtime.sync_dirty_panels(&doc, false, false, 0, 0);
    assert_eq!(changed.len(), 3);
    assert!(!runtime.has_dirty_panels());
}

/// 存在しないパネル ID を `mark_dirty` しても dirty 集合に追加されない。
#[test]
fn mark_dirty_unknown_panel_id_is_ignored() {
    let mut runtime = PanelRuntime::new();
    runtime.register_panel(Box::new(MockPanel::new("panel.a")));
    let doc = Document::new(1, 1);
    let _ = runtime.sync_dirty_panels(&doc, false, false, 0, 0);

    runtime.mark_dirty("panel.nonexistent");
    assert!(!runtime.has_dirty_panels());
}

/// S15: install_gpu_context 未呼び出しでも render_panels は空 Vec を返す（パニックしない）
#[cfg(feature = "html-panel")]
#[test]
fn render_panels_returns_empty_when_gpu_not_installed() {
    use crate::html_panel::HtmlPanelPlugin;
    let html = r#"<html><body><button id="x" data-action="command:noop">X</button></body></html>"#;
    let plugin = HtmlPanelPlugin::from_parts("html.test", "T", html, "", None);
    let mut runtime = PanelRuntime::new();
    runtime.register_panel(Box::new(plugin));
    let frames = runtime.render_panels(&[("html.test".to_string(), 100, 50)], 1.0, 0);
    assert!(frames.is_empty(), "no GPU context => no frames");
}

/// Phase 3: panel_measured_sizes が登録された全 GPU パネルの (id, w, h) を返す
#[cfg(feature = "html-panel")]
#[test]
fn panel_measured_sizes_returns_each_panel() {
    use crate::html_panel::HtmlPanelPlugin;
    let plugin_a = HtmlPanelPlugin::from_parts(
        "html.size.a",
        "A",
        r#"<html><body style="margin:0"><div style="width:100px;height:40px"></div></body></html>"#,
        "",
        Some((100, 40)),
    );
    let plugin_b = HtmlPanelPlugin::from_parts(
        "html.size.b",
        "B",
        r#"<html><body><div></div></body></html>"#,
        "",
        Some((222, 333)),
    );
    let mut runtime = PanelRuntime::new();
    runtime.register_panel(Box::new(plugin_a));
    runtime.register_panel(Box::new(plugin_b));
    let sizes = runtime.panel_measured_sizes();
    let a = sizes.iter().find(|(id, _, _)| id == "html.size.a").expect("a present");
    let b = sizes.iter().find(|(id, _, _)| id == "html.size.b").expect("b present");
    assert_eq!((a.1, a.2), (100, 40));
    assert_eq!((b.1, b.2), (222, 333));
}

/// Phase 3: forward_panel_input は対象パネルの engine.on_input を呼び layout_dirty を立てる
#[cfg(feature = "html-panel")]
#[test]
fn forward_panel_input_routes_to_correct_plugin() {
    use crate::html_panel::HtmlPanelPlugin;
    use panel_html_experiment::blitz_traits::events::{
        BlitzPointerEvent, BlitzPointerId, MouseEventButton, MouseEventButtons, PointerCoords,
        PointerDetails, UiEvent,
    };
    let html = r#"<html><body><button id="b" data-action="command:noop" style="display:block;width:80px;height:40px">B</button></body></html>"#;
    let plugin = HtmlPanelPlugin::from_parts("html.input.target", "T", html, "", Some((80, 40)));
    let mut runtime = PanelRuntime::new();
    runtime.register_panel(Box::new(plugin));

    let event = UiEvent::PointerMove(BlitzPointerEvent {
        id: BlitzPointerId::Mouse,
        is_primary: true,
        coords: PointerCoords {
            page_x: 10.0,
            page_y: 10.0,
            client_x: 10.0,
            client_y: 10.0,
            screen_x: 10.0,
            screen_y: 10.0,
        },
        button: MouseEventButton::Main,
        buttons: MouseEventButtons::empty(),
        mods: keyboard_types::Modifiers::empty(),
        details: PointerDetails::default(),
    });
    let routed = runtime.forward_panel_input("html.input.target", event);
    assert!(routed, "forward_panel_input should route to existing panel");
    let routed_unknown = runtime.forward_panel_input("html.input.nonexistent", make_dummy_pointer_move());
    assert!(!routed_unknown, "unknown panel id should not route");
}

#[cfg(feature = "html-panel")]
fn make_dummy_pointer_move() -> panel_html_experiment::blitz_traits::events::UiEvent {
    use panel_html_experiment::blitz_traits::events::{
        BlitzPointerEvent, BlitzPointerId, MouseEventButton, MouseEventButtons, PointerCoords,
        PointerDetails, UiEvent,
    };
    UiEvent::PointerMove(BlitzPointerEvent {
        id: BlitzPointerId::Mouse,
        is_primary: true,
        coords: PointerCoords {
            page_x: 0.0,
            page_y: 0.0,
            client_x: 0.0,
            client_y: 0.0,
            screen_x: 0.0,
            screen_y: 0.0,
        },
        button: MouseEventButton::Main,
        buttons: MouseEventButtons::empty(),
        mods: keyboard_types::Modifiers::empty(),
        details: PointerDetails::default(),
    })
}

/// Phase 3: take_panel_size_changes は変化があった panel_id だけを返し、二回目は空
#[cfg(feature = "html-panel")]
#[test]
fn take_panel_size_changes_yields_changed_panels_then_empty() {
    use crate::html_panel::HtmlPanelPlugin;
    let plugin = HtmlPanelPlugin::from_parts(
        "html.size.changes",
        "T",
        r#"<html><body style="margin:0"><div style="width:50px;height:25px"></div></body></html>"#,
        "",
        Some((300, 200)), // ロード時にコンテンツサイズと違う値を渡す
    );
    let mut runtime = PanelRuntime::new();
    runtime.register_panel(Box::new(plugin));

    // 初期サイズは restored = (300, 200)
    let initial = runtime.panel_measured_sizes();
    assert_eq!(
        initial.iter().find(|(id, _, _)| id == "html.size.changes").map(|s| (s.1, s.2)),
        Some((300, 200))
    );

    // take は GPU render が走らないと変化を検知しない。サイズ変化シミュレートのため
    // forcibly mark size change via テストフック (本実装では on_render 経由で起きる)。
    runtime.test_mark_panel_size_changed("html.size.changes", (50, 25));
    let changes = runtime.take_panel_size_changes();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].0, "html.size.changes");
    assert_eq!(changes[0].1, (50, 25));

    // 二回目は空
    let changes_again = runtime.take_panel_size_changes();
    assert!(changes_again.is_empty());
}

/// Phase 1: HTML パネルは workspace 統合のために `panel_trees()` に id 付きで現れる必要がある。
/// `panel_tree()` は children 空でも tree 自体を返すため、ui-shell の `reconcile_runtime_panels`
/// で workspace_layout エントリが作られる前提が満たされる。
#[cfg(feature = "html-panel")]
#[test]
fn html_panel_appears_in_panel_trees_with_static_id() {
    use crate::html_panel::HtmlPanelPlugin;
    let plugin = HtmlPanelPlugin::from_parts(
        "html.workspace.fixture",
        "T",
        "<html><body></body></html>",
        "",
        None,
    );
    let mut runtime = PanelRuntime::new();
    runtime.register_panel(Box::new(plugin));

    let trees = runtime.panel_trees();
    let tree = trees
        .iter()
        .find(|t| t.id == "html.workspace.fixture")
        .expect("HTML panel must appear in panel_trees() so workspace can register it");
    assert!(
        tree.children.is_empty(),
        "HTML panel tree intentionally has empty children (GPU 直描画)"
    );
}

// ───── Phase 9E-3: DSL→HTML GPU 統合経路のテスト ─────────────────────────

#[cfg(feature = "html-panel")]
mod gpu_unified {
    use super::*;
    use crate::html_panel::HtmlPanelPlugin;
    use panel_html_experiment::{vello, wgpu};
    use std::sync::{Mutex, MutexGuard, OnceLock};

    /// HtmlPanelPlugin と同居する複数 GPU テストを直列化する (Windows 環境の Adapter 競合回避)。
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
            label: Some("panel-runtime-test-device"),
            required_features: wgpu::Features::empty(),
            required_limits: limits,
            memory_hints: wgpu::MemoryHints::default(),
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
            trace: wgpu::Trace::Off,
        }))
        .ok()?;
        Some((device, queue))
    }

    /// 9E-3 step3 #3: 統一 API `render_panels` が DSL/HTML 両方の panel_id を含むこと。
    /// 「DSL」の代理として、DSL→HTML 翻訳器の出力 HTML を HtmlPanelPlugin に詰めて使う
    /// (DslPanelPlugin の真の翻訳は wasmtime が必要なため統合テスト外)。
    #[test]
    fn panel_runtime_render_panels_includes_dsl_and_html() {
        use panel_api::{PanelNode, PanelTree};
        use crate::dsl_to_html;

        let _guard = gpu_test_lock();
        let Some((device, queue)) = try_init_device() else {
            eprintln!("skip: no GPU device");
            return;
        };

        // DSL 相当の翻訳 HTML を持つ HtmlPanelPlugin (id を DSL 側っぽい naming にする)
        let dsl_tree = PanelTree {
            id: "altpaint.color-palette",
            title: "Color Palette",
            children: vec![PanelNode::Button {
                id: "swatch.0".to_string(),
                label: "Pick".to_string(),
                action: panel_api::HostAction::DispatchCommand(app_core::Command::Noop),
                active: false,
                fill_color: None,
            }],
        };
        let (dsl_html, dsl_css) = dsl_to_html::translate_panel_tree(&dsl_tree);
        let dsl_panel = HtmlPanelPlugin::from_parts(
            "altpaint.color-palette",
            "Color Palette",
            &dsl_html,
            &dsl_css,
            None,
        );

        // 純 HTML パネル
        let html_panel = HtmlPanelPlugin::from_parts(
            "altpaint.app-actions",
            "App Actions",
            r#"<html><body style="margin:0"><div style="width:80px;height:30px;background:#ffffff"></div></body></html>"#,
            "",
            None,
        );

        let mut runtime = PanelRuntime::new();
        runtime.register_panel(Box::new(dsl_panel));
        runtime.register_panel(Box::new(html_panel));
        runtime.install_gpu_context(
            std::sync::Arc::new(device),
            std::sync::Arc::new(queue),
        );

        let ids = runtime.panel_ids_with_gpu();
        assert!(
            ids.iter().any(|id| id == "altpaint.color-palette"),
            "DSL panel id missing: {ids:?}"
        );
        assert!(
            ids.iter().any(|id| id == "altpaint.app-actions"),
            "HTML panel id missing: {ids:?}"
        );

        let sized: Vec<(String, u32, u32)> =
            ids.iter().map(|id| (id.clone(), 320u32, 240u32)).collect();
        let frames = runtime.render_panels(&sized, 1.0, 0);
        assert!(
            frames.iter().any(|f| f.panel_id == "altpaint.color-palette"),
            "DSL frame missing"
        );
        assert!(
            frames.iter().any(|f| f.panel_id == "altpaint.app-actions"),
            "HTML frame missing"
        );
    }

    /// 9E-3 step3 #1: 翻訳した color-palette 相当 DSL ツリーを engine.on_render で
    /// vello scene に流すと glyph run が少なくとも 1 個出ること。
    /// (代理 HtmlPanelPlugin で検証 — DSL→HTML 翻訳器の出力経路を覆う)
    #[test]
    fn dsl_plugin_render_gpu_emits_glyph_runs() {
        use panel_api::{PanelNode, PanelTree};
        use crate::dsl_to_html;

        let _guard = gpu_test_lock();
        let Some((device, queue)) = try_init_device() else {
            eprintln!("skip: no GPU device");
            return;
        };

        let dsl_tree = PanelTree {
            id: "altpaint.color-palette",
            title: "Color Palette",
            children: vec![PanelNode::Text {
                id: "label".to_string(),
                text: "Hello".to_string(),
            }],
        };
        let (html, css) = dsl_to_html::translate_panel_tree(&dsl_tree);

        let mut renderer = vello::Renderer::new(
            &device,
            vello::RendererOptions {
                use_cpu: false,
                num_init_threads: None,
                antialiasing_support: vello::AaSupport::area_only(),
                pipeline_cache: None,
            },
        )
        .expect("vello renderer");
        let mut scene = vello::Scene::new();
        let mut plugin = HtmlPanelPlugin::from_parts(
            "altpaint.color-palette",
            "Color Palette",
            &html,
            &css,
            None,
        );
        let outcome = plugin.render_gpu(
            &device,
            &queue,
            &mut renderer,
            &mut scene,
            (320, 240),
            1.0,
            0,
        );
        assert!(outcome.is_rendered(), "first render should produce frame");

        // glyph run が 1 個以上含まれていること。vello::Scene には glyph_runs() 公開 API が
        // ないため、readback で「不透明・非真っ黒な (=描画された)」ピクセルが存在することで
        // 弱代用する。base_color = TRANSPARENT のため、未描画は alpha=0。
        let target = outcome.target();
        let pixels =
            readback_rgba(&device, &queue, &target.texture, target.width, target.height);
        let painted = pixels.chunks_exact(4).filter(|p| p[3] > 0).count();
        assert!(
            painted > 0,
            "expected at least one painted pixel from translated DSL text"
        );
    }

    /// 9E-3 step3 #2: state 不変なフレームでは 2 回目の render_gpu が Skipped を返す。
    #[test]
    fn dsl_plugin_state_unchanged_returns_skipped() {
        use panel_api::{PanelNode, PanelTree};
        use crate::dsl_to_html;

        let _guard = gpu_test_lock();
        let Some((device, queue)) = try_init_device() else {
            eprintln!("skip: no GPU device");
            return;
        };

        let tree = PanelTree {
            id: "altpaint.idle",
            title: "Idle",
            children: vec![PanelNode::Text {
                id: "x".to_string(),
                text: "Stable".to_string(),
            }],
        };
        let (html, css) = dsl_to_html::translate_panel_tree(&tree);
        let mut plugin = HtmlPanelPlugin::from_parts("altpaint.idle", "Idle", &html, &css, None);
        let mut renderer = vello::Renderer::new(
            &device,
            vello::RendererOptions {
                use_cpu: false,
                num_init_threads: None,
                antialiasing_support: vello::AaSupport::area_only(),
                pipeline_cache: None,
            },
        )
        .expect("vello renderer");
        let mut scene = vello::Scene::new();

        let first = plugin.render_gpu(
            &device,
            &queue,
            &mut renderer,
            &mut scene,
            (200, 100),
            1.0,
            0,
        );
        assert!(first.is_rendered(), "first call should render");

        let second = plugin.render_gpu(
            &device,
            &queue,
            &mut renderer,
            &mut scene,
            (200, 100),
            1.0,
            0,
        );
        assert!(
            !second.is_rendered(),
            "idle re-render must be Skipped (RenderOutcome)"
        );
    }

    pub(super) fn readback_rgba(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        texture: &wgpu::Texture,
        width: u32,
        height: u32,
    ) -> Vec<u8> {
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
