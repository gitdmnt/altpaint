use anyhow::Result;
use app_core::CanvasViewTransform;
use render::RenderFrame;
use slint::{GraphicsAPI, RenderingState};
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Debug)]
struct GpuCanvasResources {
    texture: wgpu::Texture,
    texture_view: wgpu::TextureView,
    sampler: wgpu::Sampler,
    frame_width: u32,
    frame_height: u32,
}

#[derive(Debug, Default)]
pub struct WgpuCanvasState {
    pub clear_color: wgpu::Color,
    pub device_ready: bool,
    pub active_tool_label: String,
    pub frame: Option<RenderFrame>,
    pub upload_pending: bool,
    pub transform: CanvasViewTransform,
    gpu: Option<GpuCanvasResources>,
}

impl WgpuCanvasState {
    pub fn new() -> Self {
        Self {
            clear_color: wgpu::Color {
                r: 0.35,
                g: 0.35,
                b: 0.35,
                a: 1.0,
            },
            device_ready: false,
            active_tool_label: "Brush".to_string(),
            frame: None,
            upload_pending: false,
            transform: CanvasViewTransform::default(),
            gpu: None,
        }
    }
}

pub fn install_wgpu_underlay(window: &slint::Window) -> Result<Rc<RefCell<WgpuCanvasState>>> {
    let state = Rc::new(RefCell::new(WgpuCanvasState::new()));
    let state_for_notifier = state.clone();

    window.set_rendering_notifier(move |state_info, graphics_api| {
        match state_info {
            RenderingState::RenderingTeardown => {}
            RenderingState::BeforeRendering => {
                if let GraphicsAPI::WGPU28 { queue, .. } = graphics_api {
                    if let Some(command_buffer) = graphics_api_command_encoder(graphics_api, &state_for_notifier) {
                        queue.submit([command_buffer]);
                    }
                }
            }
            RenderingState::AfterRendering => {}
            RenderingState::RenderingSetup => {
                if let GraphicsAPI::WGPU28 { device, .. } = graphics_api {
                    let mut state = state_for_notifier.borrow_mut();
                    state.device_ready = true;
                    ensure_gpu_resources(device, &mut state);
                }
            }
            _ => {}
        }
    })?;

    Ok(state)
}

pub fn update_canvas_state_from_document(
    state: &Rc<RefCell<WgpuCanvasState>>,
    tool_label: String,
    clear_color: wgpu::Color,
    frame: RenderFrame,
    transform: CanvasViewTransform,
) {
    let mut state = state.borrow_mut();
    state.active_tool_label = tool_label;
    state.clear_color = clear_color;
    state.frame = Some(frame);
    state.transform = transform;
    state.upload_pending = true;
}

fn graphics_api_command_encoder(
    graphics_api: &GraphicsAPI<'_>,
    state: &Rc<RefCell<WgpuCanvasState>>,
) -> Option<wgpu::CommandBuffer> {
    let GraphicsAPI::WGPU28 { device, queue, .. } = graphics_api else {
        return None;
    };

    let mut state = state.borrow_mut();
    ensure_gpu_resources(device, &mut state);
    upload_frame_if_needed(device, queue, &mut state);

    let encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("altpaint-canvas-underlay"),
    });

    let gpu = state.gpu.as_ref()?;
    let _ = state.transform;
    let _ = (&gpu.texture_view, &gpu.sampler);

    Some(encoder.finish())
}

fn ensure_gpu_resources(device: &wgpu::Device, state: &mut WgpuCanvasState) {
    let Some(frame) = state.frame.as_ref() else {
        return;
    };

    let width = frame.width as u32;
    let height = frame.height as u32;
    let needs_rebuild = state.gpu.as_ref().is_none_or(|gpu| {
        gpu.frame_width != width || gpu.frame_height != height
    });

    if !needs_rebuild {
        return;
    }

    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("altpaint-canvas-texture"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });

    let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("altpaint-canvas-sampler"),
        mag_filter: wgpu::FilterMode::Nearest,
        min_filter: wgpu::FilterMode::Nearest,
        mipmap_filter: wgpu::MipmapFilterMode::Nearest,
        ..Default::default()
    });

    state.gpu = Some(GpuCanvasResources {
        texture,
        texture_view,
        sampler,
        frame_width: width,
        frame_height: height,
    });
    state.upload_pending = true;
}

fn upload_frame_if_needed(device: &wgpu::Device, queue: &wgpu::Queue, state: &mut WgpuCanvasState) {
    if !state.upload_pending {
        return;
    }
    let (Some(frame), Some(gpu)) = (state.frame.as_ref(), state.gpu.as_ref()) else {
        return;
    };

    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &gpu.texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        frame_to_bgra_pixels(frame).as_slice(),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some((frame.width * 4) as u32),
            rows_per_image: Some(frame.height as u32),
        },
        wgpu::Extent3d {
            width: frame.width as u32,
            height: frame.height as u32,
            depth_or_array_layers: 1,
        },
    );

    let _ = device;
    state.upload_pending = false;
}

pub fn frame_to_bgra_pixels(frame: &RenderFrame) -> Vec<u8> {
    let mut pixels = frame.pixels.clone();
    for chunk in pixels.chunks_exact_mut(4) {
        chunk.swap(0, 2);
    }
    pixels
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_to_bgra_swaps_red_and_blue_channels() {
        let frame = RenderFrame {
            width: 2,
            height: 1,
            pixels: vec![10, 20, 30, 255, 1, 2, 3, 255],
        };

        assert_eq!(frame_to_bgra_pixels(&frame), vec![30, 20, 10, 255, 3, 2, 1, 255]);
    }
}
