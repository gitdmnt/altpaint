// Flood fill expansion step (4-connect region growing).
//
// Writes mark_out = 1.0 for pixels matching seed color that are either the seed
// itself or have at least one marked neighbor in mark_in.
// changed_counter is incremented for each newly marked pixel so the host can
// detect convergence.

struct FloodFillStepParams {
    seed_x: u32,
    seed_y: u32,
    layer_width: u32,
    layer_height: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
    _pad3: u32,
}

@group(0) @binding(0) var<uniform> params: FloodFillStepParams;
@group(0) @binding(1) var source_tex: texture_2d<f32>;
@group(0) @binding(2) var mark_in: texture_2d<f32>;
@group(0) @binding(3) var mark_out: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(4) var<storage, read_write> changed_counter: array<atomic<u32>, 1>;

fn color_equals(a: vec4<f32>, b: vec4<f32>) -> bool {
    let ai = vec4<i32>(floor(a * 255.0 + vec4<f32>(0.5)));
    let bi = vec4<i32>(floor(b * 255.0 + vec4<f32>(0.5)));
    return ai.x == bi.x && ai.y == bi.y && ai.z == bi.z && ai.w == bi.w;
}

fn sample_mark(p: vec2<i32>) -> f32 {
    if (p.x < 0 || p.y < 0 ||
        p.x >= i32(params.layer_width) || p.y >= i32(params.layer_height)) {
        return 0.0;
    }
    return textureLoad(mark_in, p, 0).r;
}

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= params.layer_width || id.y >= params.layer_height) {
        return;
    }

    let p = vec2<i32>(i32(id.x), i32(id.y));
    let prev = textureLoad(mark_in, p, 0).r;
    if (prev > 0.5) {
        textureStore(mark_out, p, vec4<f32>(1.0, 0.0, 0.0, 1.0));
        return;
    }

    let seed_pos = vec2<i32>(i32(params.seed_x), i32(params.seed_y));
    let seed_color = textureLoad(source_tex, seed_pos, 0);
    let my_color = textureLoad(source_tex, p, 0);
    if (!color_equals(my_color, seed_color)) {
        textureStore(mark_out, p, vec4<f32>(0.0, 0.0, 0.0, 1.0));
        return;
    }

    let is_seed = (p.x == seed_pos.x && p.y == seed_pos.y);
    let n_up    = sample_mark(vec2<i32>(p.x,     p.y - 1));
    let n_down  = sample_mark(vec2<i32>(p.x,     p.y + 1));
    let n_left  = sample_mark(vec2<i32>(p.x - 1, p.y));
    let n_right = sample_mark(vec2<i32>(p.x + 1, p.y));
    let any_neighbor = max(max(n_up, n_down), max(n_left, n_right));

    if (is_seed || any_neighbor > 0.5) {
        textureStore(mark_out, p, vec4<f32>(1.0, 0.0, 0.0, 1.0));
        atomicAdd(&changed_counter[0], 1u);
    } else {
        textureStore(mark_out, p, vec4<f32>(0.0, 0.0, 0.0, 1.0));
    }
}
