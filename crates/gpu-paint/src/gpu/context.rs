//! wgpu デバイス/キュー共有コンテキスト。

use std::sync::Arc;

/// wgpu デバイスとキューを共有するコンテキスト。
///
/// Arc でラップされているため複数の構造体から安全に共有できる。
pub struct GpuCanvasContext {
    pub device: Arc<wgpu::Device>,
    pub queue: Arc<wgpu::Queue>,
}

impl GpuCanvasContext {
    /// 新しいコンテキストを生成する。
    pub fn new(device: Arc<wgpu::Device>, queue: Arc<wgpu::Queue>) -> Self {
        Self { device, queue }
    }

    /// device の `Arc` クローンを返す。パイプラインへ所有権を渡す際に使う。
    pub fn device(&self) -> Arc<wgpu::Device> {
        self.device.clone()
    }

    /// queue の `Arc` クローンを返す。
    pub fn queue(&self) -> Arc<wgpu::Queue> {
        self.queue.clone()
    }
}
