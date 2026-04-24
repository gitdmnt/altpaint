// Clear composite_rw to transparent within the dirty rect.

struct ClearParams {
    dirty_x0: u32,
    dirty_y0: u32,
    dirty_x1: u32,
    dirty_y1: u32,
    layer_width: u32,
    layer_height: u32,
    _pad0: u32,
    _pad1: u32,
}

@group(0) @binding(0) var<uniform> params: ClearParams;
@group(0) @binding(1) var composite_rw: texture_storage_2d<rgba8unorm, write>;

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let x = id.x + params.dirty_x0;
    let y = id.y + params.dirty_y0;
    if (x >= params.dirty_x1 || y >= params.dirty_y1) {
        return;
    }
    if (x >= params.layer_width || y >= params.layer_height) {
        return;
    }
    textureStore(composite_rw, vec2<i32>(i32(x), i32(y)), vec4<f32>(0.0, 0.0, 0.0, 0.0));
}
