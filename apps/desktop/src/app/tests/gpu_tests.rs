//! GPU 機能（install_gpu_resources / sync_all_layers_to_gpu / should_use_gpu_canvas_source）の
//! 統合テスト。`gpu` feature が有効な場合のみコンパイルされる。

use std::sync::Arc;

use app_core::Command;

use super::{TestDialogs, unique_test_path};
use super::super::DesktopApp;

/// wgpu デバイスとキューを生成するテスト用ヘルパー。GPU がない CI では None を返す。
async fn try_init_device() -> Option<(Arc<wgpu::Device>, Arc<wgpu::Queue>)> {
    let instance = wgpu::Instance::default();
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::None,
            compatible_surface: None,
            force_fallback_adapter: false,
        })
        .await
        .ok()?;
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("desktop-gpu-test-device"),
            required_features: wgpu::Features::empty(),
            experimental_features: Default::default(),
            required_limits: adapter.limits(),
            memory_hints: wgpu::MemoryHints::default(),
            trace: wgpu::Trace::default(),
        })
        .await
        .ok()?;
    Some((Arc::new(device), Arc::new(queue)))
}

fn make_test_app() -> DesktopApp {
    use std::path::PathBuf;
    DesktopApp::new_with_dialogs_session_path_and_workspace_preset_path(
        PathBuf::from("/tmp/altpaint-gpu-test.altp.json"),
        Box::new(TestDialogs::default()),
        unique_test_path("gpu-session"),
        unique_test_path("gpu-workspace"),
    )
}

/// install_gpu_resources 後にすべての GPU フィールドが Some になること。
#[test]
fn install_gpu_resources_sets_all_gpu_fields_to_some() {
    pollster::block_on(async {
        let Some((device, queue)) = try_init_device().await else {
            return;
        };
        let mut app = make_test_app();
        app.install_gpu_resources(device, queue, true);
        assert!(app.gpu_canvas_pool.is_some());
        assert!(app.gpu_pen_tip_cache.is_some());
        assert!(app.gpu_brush.is_some());
    });
}

/// sync_all_layers_to_gpu がすべてのパネル×レイヤーのテクスチャを生成すること。
#[test]
fn sync_all_layers_to_gpu_creates_textures_for_all_layers() {
    pollster::block_on(async {
        let Some((device, queue)) = try_init_device().await else {
            return;
        };
        let mut app = make_test_app();
        app.install_gpu_resources(device, queue, true);

        // 2 番目のレイヤーを追加
        app.execute_document_command(Command::AddRasterLayer);

        // pool が全レイヤーのテクスチャを持つことを確認
        let pool = app.gpu_canvas_pool.as_ref().unwrap();
        for page in &app.document.work.pages {
            for panel in &page.panels {
                let panel_id_str = panel.id.0.to_string();
                for layer_index in 0..panel.layers.len() {
                    assert!(
                        pool.get(&panel_id_str, layer_index).is_some(),
                        "panel={panel_id_str} layer={layer_index} should have a texture"
                    );
                }
            }
        }
    });
}

/// should_use_gpu_canvas_source は GPU リソースなしで false を返す。
#[test]
fn should_use_gpu_canvas_source_false_without_resources() {
    let app = make_test_app();
    assert!(!app.should_use_gpu_canvas_source());
}

/// should_use_gpu_canvas_source は srgb_view_supported=false で false を返す。
#[test]
fn should_use_gpu_canvas_source_false_if_srgb_not_supported() {
    pollster::block_on(async {
        let Some((device, queue)) = try_init_device().await else {
            return;
        };
        let mut app = make_test_app();
        app.install_gpu_resources(device, queue, false);
        assert!(!app.should_use_gpu_canvas_source());
    });
}

/// should_use_gpu_canvas_source は単一レイヤー + リソース + sRGB サポートで true を返す。
#[test]
fn should_use_gpu_canvas_source_true_for_single_layer_with_resources() {
    pollster::block_on(async {
        let Some((device, queue)) = try_init_device().await else {
            return;
        };
        let mut app = make_test_app();
        app.install_gpu_resources(device, queue, true);
        assert!(app.should_use_gpu_canvas_source());
    });
}

/// should_use_gpu_canvas_source は複数レイヤーで false を返す。
#[test]
fn should_use_gpu_canvas_source_false_for_multi_layer() {
    pollster::block_on(async {
        let Some((device, queue)) = try_init_device().await else {
            return;
        };
        let mut app = make_test_app();
        app.install_gpu_resources(device.clone(), queue.clone(), true);
        assert!(app.should_use_gpu_canvas_source());

        app.execute_document_command(Command::AddRasterLayer);
        assert!(!app.should_use_gpu_canvas_source());
    });
}

/// AddRasterLayer 後に should_use_gpu_canvas_source が false になること。
#[test]
fn layer_count_change_updates_gpu_source_decision() {
    pollster::block_on(async {
        let Some((device, queue)) = try_init_device().await else {
            return;
        };
        let mut app = make_test_app();
        app.install_gpu_resources(device, queue, true);
        assert!(app.should_use_gpu_canvas_source());

        app.execute_document_command(Command::AddRasterLayer);
        assert!(!app.should_use_gpu_canvas_source(), "multi-layer should disable gpu source");
    });
}
