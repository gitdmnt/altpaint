//! 起動時のフォーマットサポート確認。

/// アダプターが Rgba8Unorm の STORAGE_READ_WRITE をサポートするか確認する。
///
/// `true` なら Rgba8Unorm を compute shader の read_write storage texture として使用できる。
/// `false` なら GPU ブラシ dispatch は使用不可（CPU パスへフォールバック）。
pub fn supports_rgba8unorm_storage(adapter: &wgpu::Adapter) -> bool {
    adapter
        .get_texture_format_features(wgpu::TextureFormat::Rgba8Unorm)
        .flags
        .contains(wgpu::TextureFormatFeatureFlags::STORAGE_READ_WRITE)
}
