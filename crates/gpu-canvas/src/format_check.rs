//! 起動時のフォーマットサポート確認。

/// アダプターが Rgba8Unorm の STORAGE_READ_WRITE をサポートするか確認する。
///
/// `true` なら Rgba8Unorm を compute 用フォーマットとして採用できる。
/// `false` なら Rgba32Float + blit パスが必要（Phase 8A では採用フォーマットを選択するのみ）。
pub fn supports_rgba8unorm_storage(adapter: &wgpu::Adapter) -> bool {
    let features = adapter.features();
    features.contains(wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES)
        || adapter
            .get_texture_format_features(wgpu::TextureFormat::Rgba8Unorm)
            .allowed_usages
            .contains(wgpu::TextureUsages::STORAGE_BINDING)
}
