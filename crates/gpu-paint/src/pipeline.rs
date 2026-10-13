//! compute パイプライン生成の共通ヘルパ。
//!
//! brush / fill / composite の各ディスパッチャが個別に持っていた
//! `build_pipeline` 3 重複を 1 箇所へ集約する。

/// 指定 WGSL ソースから entry_point `main` の compute パイプラインを生成する。
///
/// 単一 bind group layout を持つパイプライン用。`fill` / `composite` の
/// 各シェーダ生成で使う。
pub(crate) fn build_compute_pipeline(
    device: &wgpu::Device,
    bgl: &wgpu::BindGroupLayout,
    wgsl: &str,
    label: &str,
) -> wgpu::ComputePipeline {
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(label),
        bind_group_layouts: &[bgl],
        immediate_size: 0,
    });
    build_compute_pipeline_with_layout(device, &pipeline_layout, wgsl, label)
}

/// 既存の `pipeline_layout` を共有して compute パイプラインを生成する。
///
/// 複数シェーダが同一レイアウトを共有する場合 (brush の stroke / erase) に使う。
pub(crate) fn build_compute_pipeline_with_layout(
    device: &wgpu::Device,
    pipeline_layout: &wgpu::PipelineLayout,
    wgsl: &str,
    label: &str,
) -> wgpu::ComputePipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(wgsl.into()),
    });
    device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some(label),
        layout: Some(pipeline_layout),
        module: &shader,
        entry_point: Some("main"),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    })
}
