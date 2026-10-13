// Lasso fill: write 1.0 to mark_out at every pixel whose center is strictly
// inside the polygon defined by polygon_points (ray casting, matching
// canvas::ops::point_in_polygon).

struct LassoMarkParams {
    polygon_count: u32,
    layer_width: u32,
    layer_height: u32,
    aabb_x0: u32,
    aabb_y0: u32,
    aabb_x1: u32,
    aabb_y1: u32,
    _pad: u32,
}

@group(0) @binding(0) var<uniform> params: LassoMarkParams;
@group(0) @binding(1) var<storage, read> polygon_points: array<vec2<f32>>;
@group(0) @binding(2) var mark_out: texture_storage_2d<rgba8unorm, write>;

fn point_in_polygon(px: f32, py: f32) -> bool {
    var inside: bool = false;
    let n = params.polygon_count;
    if (n < 3u) {
        return false;
    }
    var j: u32 = n - 1u;
    for (var i: u32 = 0u; i < n; i = i + 1u) {
        let a = polygon_points[i];
        let b = polygon_points[j];
        let cond1 = (a.y > py) != (b.y > py);
        if (cond1) {
            let dy = b.y - a.y;
            let denom = select(dy, 0.000001, dy == 0.0);
            let xint = (b.x - a.x) * (py - a.y) / denom + a.x;
            if (px < xint) {
                inside = !inside;
            }
        }
        j = i;
    }
    return inside;
}

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= params.layer_width || id.y >= params.layer_height) {
        return;
    }
    if (id.x < params.aabb_x0 || id.y < params.aabb_y0 ||
        id.x > params.aabb_x1 || id.y > params.aabb_y1) {
        textureStore(mark_out, vec2<i32>(i32(id.x), i32(id.y)), vec4<f32>(0.0, 0.0, 0.0, 0.0));
        return;
    }

    let px = f32(id.x) + 0.5;
    let py = f32(id.y) + 0.5;
    let hit = point_in_polygon(px, py);
    let m: f32 = select(0.0, 1.0, hit);
    textureStore(mark_out, vec2<i32>(i32(id.x), i32(id.y)), vec4<f32>(m, 0.0, 0.0, 1.0));
}
