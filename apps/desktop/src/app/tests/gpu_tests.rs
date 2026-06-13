//! GPU 機能（install_gpu_resources / sync_all_layers_to_gpu / should_use_gpu_canvas_source）の
//! 統合テスト。

use std::sync::Arc;

use document_model::DocumentCommand;

use super::{TestDialogs, unique_test_path};
use super::super::{DesktopApp, DesktopAppOptions};

/// wgpu デバイスとキューを生成するテスト用ヘルパー。
/// GPU がない CI、または Rgba8Unorm STORAGE_READ_WRITE 非対応のアダプターでは `None` を返す。
async fn try_init_device() -> Option<(Arc<wgpu::Device>, Arc<wgpu::Queue>)> {
    let instance = wgpu::Instance::default();
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        })
        .await
        .ok()?;
    if !gpu_paint::format_check::supports_rgba8unorm_storage(&adapter) {
        return None;
    }
    let storage_format_features =
        adapter.features() & wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES;
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("desktop-gpu-test-device"),
            required_features: storage_format_features,
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
    DesktopApp::with_options(DesktopAppOptions {
        project_path: PathBuf::from("/tmp/altpaint-gpu-test.altp.json"),
        dialogs: Box::new(TestDialogs::default()),
        session_path: unique_test_path("gpu-session"),
        workspace_preset_path: unique_test_path("gpu-workspace"),
        canvas_size_preset_path: unique_test_path("gpu-canvas-size"),
    })
}

/// install_gpu_resources 後にすべての GPU フィールドが Some になること。
#[test]
fn install_gpu_resources_sets_all_gpu_fields_to_some() {
    pollster::block_on(async {
        let Some((device, queue)) = try_init_device().await else {
            return;
        };
        let mut app = make_test_app();
        app.install_gpu_resources(device, queue);
        assert!(app.gpu.is_some());
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
        app.install_gpu_resources(device, queue);

        // 2 番目のレイヤーを追加
        app.apply_document_command(&DocumentCommand::AddRasterLayer);

        // pool が全レイヤーのテクスチャを持つことを確認
        let pool = app.layer_texture_store().unwrap();
        for page in &app.document.work.pages {
            for koma in &page.komas {
                let koma_key = gpu_paint::KomaTextureId(koma.id.0);
                for layer_index in 0..koma.layers.len() {
                    assert!(
                        pool.get(koma_key, layer_index).is_some(),
                        "koma={koma_key:?} layer={layer_index} should have a texture"
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

/// should_use_gpu_canvas_source は単一レイヤー + リソースで true を返す。
#[test]
fn should_use_gpu_canvas_source_true_for_single_layer_with_resources() {
    pollster::block_on(async {
        let Some((device, queue)) = try_init_device().await else {
            return;
        };
        let mut app = make_test_app();
        app.install_gpu_resources(device, queue);
        assert!(app.should_use_gpu_canvas_source());
    });
}

/// should_use_gpu_canvas_source は複数レイヤー時も composite 経由で true を維持する。
#[test]
fn should_use_gpu_canvas_source_true_for_multi_layer_via_composite() {
    pollster::block_on(async {
        let Some((device, queue)) = try_init_device().await else {
            return;
        };
        let mut app = make_test_app();
        app.install_gpu_resources(device.clone(), queue.clone());
        assert!(app.should_use_gpu_canvas_source());

        app.apply_document_command(&DocumentCommand::AddRasterLayer);
        assert!(
            app.should_use_gpu_canvas_source(),
            "multi-layer should fall back to composite GPU source"
        );
    });
}

/// BL-117 回帰: コマ選択変更は GPU テクスチャを再アップロードせず、既存ピクセルを保つ。
///
/// アクティブコマのレイヤーテクスチャに識別可能なピクセルを直接書き込み、
/// 別コマへ選択を移して戻したあと、テクスチャが byte 一致で不変であることを確認する。
/// 旧実装ではコマ選択ごとに全ページ全コマ全転送が走り、CPU bitmap (全 0) で
/// 上書きされてピクセルが消えていた。
#[test]
fn selecting_koma_preserves_gpu_layer_pixels() {
    pollster::block_on(async {
        let Some((device, queue)) = try_init_device().await else {
            return;
        };
        let mut app = make_test_app();
        app.install_gpu_resources(device, queue);

        // 2 コマ目を追加 (Full 同期でテクスチャ再構築)。
        app.apply_document_command(&DocumentCommand::AddKoma);
        // コマ 0 を選択し直してアクティブにする。
        app.apply_document_command(&DocumentCommand::SelectKoma { index: 0 });

        let koma0_key = gpu_paint::KomaTextureId(app.document.active_koma().unwrap().id.0);

        // アクティブコマ (0) のレイヤー 0 テクスチャへ識別ピクセルを書き込む。
        let marker = vec![123u8; 2 * 2 * 4];
        {
            let pool = app.layer_texture_store().unwrap();
            pool.upload_region(
                koma0_key,
                0,
                geometry::PageDirtyRect::new(0, 0, 2, 2),
                &marker,
            );
        }
        let before = app
            .layer_texture_store()
            .unwrap()
            .read_back_full(koma0_key, 0)
            .expect("readback before");

        // 別コマへ移って戻す (どちらも GpuSyncGranularity::None のはず)。
        app.apply_document_command(&DocumentCommand::SelectNextKoma);
        app.apply_document_command(&DocumentCommand::SelectKoma { index: 0 });

        let after = app
            .layer_texture_store()
            .unwrap()
            .read_back_full(koma0_key, 0)
            .expect("readback after");

        assert_eq!(
            before, after,
            "コマ選択変更で GPU レイヤーテクスチャのピクセルが変化してはならない"
        );
        // マーカーが残っていること (= 全 0 で上書きされていない)。
        assert_eq!(after.2[0], 123, "識別ピクセルが消えている");
    });
}

/// BL-117 回帰: レイヤー選択変更も GPU テクスチャを保つ (GpuSyncGranularity::None)。
#[test]
fn selecting_layer_preserves_gpu_layer_pixels() {
    pollster::block_on(async {
        let Some((device, queue)) = try_init_device().await else {
            return;
        };
        let mut app = make_test_app();
        app.install_gpu_resources(device, queue);
        // 2 レイヤーにする (ActiveKomaLayers 同期)。
        app.apply_document_command(&DocumentCommand::AddRasterLayer);

        let koma_key = gpu_paint::KomaTextureId(app.document.active_koma().unwrap().id.0);
        let marker = vec![77u8; 2 * 2 * 4];
        {
            let pool = app.layer_texture_store().unwrap();
            pool.upload_region(koma_key, 0, geometry::PageDirtyRect::new(0, 0, 2, 2), &marker);
        }
        let before = app
            .layer_texture_store()
            .unwrap()
            .read_back_full(koma_key, 0)
            .expect("readback before");

        // レイヤー選択を動かして戻す (None 同期)。
        app.apply_document_command(&DocumentCommand::SelectLayer { index: 1 });
        app.apply_document_command(&DocumentCommand::SelectLayer { index: 0 });

        let after = app
            .layer_texture_store()
            .unwrap()
            .read_back_full(koma_key, 0)
            .expect("readback after");
        assert_eq!(before, after, "レイヤー選択変更でテクスチャが変化してはならない");
        assert_eq!(after.2[0], 77);
    });
}

/// AddRasterLayer (ActiveKomaLayers 差分同期) 後もアクティブコマの全レイヤーに
/// テクスチャが揃うこと。差分同期が全レイヤー登録を漏らさないことを確認する。
#[test]
fn add_layer_differential_sync_creates_all_active_koma_textures() {
    pollster::block_on(async {
        let Some((device, queue)) = try_init_device().await else {
            return;
        };
        let mut app = make_test_app();
        app.install_gpu_resources(device, queue);
        app.apply_document_command(&DocumentCommand::AddRasterLayer);
        app.apply_document_command(&DocumentCommand::AddRasterLayer);

        let koma = app.document.active_koma().unwrap();
        let koma_key = gpu_paint::KomaTextureId(koma.id.0);
        let layer_count = koma.layers.len();
        let pool = app.layer_texture_store().unwrap();
        assert_eq!(pool.layer_count_for_koma(koma_key), layer_count);
        for idx in 0..layer_count {
            assert!(pool.get(koma_key, idx).is_some(), "layer {idx} missing");
        }
    });
}

/// AddRasterLayer の差分同期はアクティブコマ以外のテクスチャに触れないこと。
#[test]
fn add_layer_differential_sync_leaves_other_komas_untouched() {
    pollster::block_on(async {
        let Some((device, queue)) = try_init_device().await else {
            return;
        };
        let mut app = make_test_app();
        app.install_gpu_resources(device, queue);
        // 2 コマ目を追加 (Full)。コマ 1 がアクティブになる。
        app.apply_document_command(&DocumentCommand::AddKoma);

        // コマ 0 のレイヤー 0 テクスチャへマーカーを書き込む。
        let koma0_key = gpu_paint::KomaTextureId(app.document.work.pages[0].komas[0].id.0);
        let marker = vec![200u8; 2 * 2 * 4];
        {
            let pool = app.layer_texture_store().unwrap();
            pool.upload_region(koma0_key, 0, geometry::PageDirtyRect::new(0, 0, 2, 2), &marker);
        }

        // アクティブコマ (1) にレイヤー追加 → ActiveKomaLayers 差分同期。
        app.apply_document_command(&DocumentCommand::AddRasterLayer);

        // コマ 0 のテクスチャは不変。
        let after = app
            .layer_texture_store()
            .unwrap()
            .read_back_full(koma0_key, 0)
            .expect("readback");
        assert_eq!(after.2[0], 200, "別コマのテクスチャが差分同期で変化した");
    });
}

/// AddRasterLayer 後に source kind が Single → Composite へ切り替わること。
#[test]
fn layer_count_change_switches_gpu_source_kind() {
    use crate::app::GpuCanvasSourceKind;
    pollster::block_on(async {
        let Some((device, queue)) = try_init_device().await else {
            return;
        };
        let mut app = make_test_app();
        app.install_gpu_resources(device, queue);
        assert_eq!(
            app.canvas_surface_source_kind(),
            Some(GpuCanvasSourceKind::Single)
        );

        app.apply_document_command(&DocumentCommand::AddRasterLayer);
        assert_eq!(
            app.canvas_surface_source_kind(),
            Some(GpuCanvasSourceKind::Composite),
            "multi-layer should switch GPU source to Composite"
        );
    });
}
