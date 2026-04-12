#import bevy_pbr::mesh_bindings mesh
#import bevy_pbr::mesh_view_bindings view
#import bevy_pbr::mesh_functions mesh_position_local_to_world, mesh_normal_local_to_world, mesh_position_local_to_clip
#import bevy_pbr::shadows fetch_directional_shadow
#import rose_client::zone_lighting apply_zone_lighting

struct Vertex {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv0: vec2<f32>,
    @location(3) uv1: vec2<f32>,
    @location(4) tile_info: u32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec4<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) uv0: vec2<f32>,
    @location(3) uv1: vec2<f32>,
    @location(4) tile_info: u32,
};

@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = mesh_position_local_to_clip(mesh.model, vec4<f32>(vertex.position, 1.0));
    out.world_position = mesh_position_local_to_world(mesh.model, vec4<f32>(vertex.position, 1.0));
    out.world_normal = mesh_normal_local_to_world(vertex.normal);
    out.uv0 = vertex.uv0;
    out.uv1 = vertex.uv1;
    out.tile_info = vertex.tile_info;
    return out;
}

@group(1) @binding(0)
var tile_array_texture: binding_array<texture_2d<f32>>;
@group(1) @binding(1)
var tile_array_sampler: sampler;

struct FragmentInput {
    @builtin(position) frag_coord: vec4<f32>,
    @location(0) world_position: vec4<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) uv0: vec2<f32>,
    @location(3) uv1: vec2<f32>,
    @location(4) tile_info: u32,
};

fn sample_soft_shadow(world_position: vec4<f32>, world_normal: vec3<f32>, view_z: f32) -> f32 {
    let texel_offset = 0.025;
    // Per-pixel rotation angle to break up banding artifacts
    let angle = fract(sin(dot(world_position.xz, vec2<f32>(12.9898, 78.233))) * 43758.5453) * 6.2832;
    let ca = cos(angle);
    let sa = sin(angle);

    var shadow = fetch_directional_shadow(0u, world_position, world_normal, view_z);

    // 4 rotated offset samples on XZ plane
    let d1 = vec2<f32>(ca, sa) * texel_offset;
    let d2 = vec2<f32>(-sa, ca) * texel_offset;

    shadow += fetch_directional_shadow(0u, world_position + vec4<f32>(d1.x, 0.0, d1.y, 0.0), world_normal, view_z);
    shadow += fetch_directional_shadow(0u, world_position + vec4<f32>(-d1.x, 0.0, -d1.y, 0.0), world_normal, view_z);
    shadow += fetch_directional_shadow(0u, world_position + vec4<f32>(d2.x, 0.0, d2.y, 0.0), world_normal, view_z);
    shadow += fetch_directional_shadow(0u, world_position + vec4<f32>(-d2.x, 0.0, -d2.y, 0.0), world_normal, view_z);

    return shadow / 5.0;
}

@fragment
fn fragment(in: FragmentInput) -> @location(0) vec4<f32> {
    let view_z = dot(vec4<f32>(
        view.inverse_view[0].z,
        view.inverse_view[1].z,
        view.inverse_view[2].z,
        view.inverse_view[3].z
    ), in.world_position);

    var tile_layer1_id: u32 = (in.tile_info) & 0xffu;
    var tile_layer2_id: u32 = (in.tile_info >> 8u) & 0xffu;
    var tile_rotation: u32 = (in.tile_info >> 16u) & 0xffu;
    var layer2_uv: vec2<f32> = in.uv1;
    if (tile_rotation == 2u) {
        layer2_uv.x = 1.0 - layer2_uv.x;
    } else if (tile_rotation == 3u) {
        layer2_uv.y = 1.0 - layer2_uv.y;
    } else if (tile_rotation == 4u) {
        layer2_uv.x = 1.0 - layer2_uv.x;
        layer2_uv.y = 1.0 - layer2_uv.y;
    } else if (tile_rotation == 5u) {
        var x: f32 = layer2_uv.x;
        layer2_uv.x = layer2_uv.y;
        layer2_uv.y = 1.0 - x;
    } else if (tile_rotation == 6u) {
        var x: f32 = layer2_uv.x;
        layer2_uv.x = layer2_uv.y;
        layer2_uv.y = x;
    }

    let layer1 = textureSample(tile_array_texture[tile_layer1_id], tile_array_sampler, in.uv1);
    let layer2 = textureSample(tile_array_texture[tile_layer2_id], tile_array_sampler, layer2_uv);
    var lightmap = textureSample(tile_array_texture[0], tile_array_sampler, in.uv0);
    let shadow = sample_soft_shadow(in.world_position, in.world_normal, view_z);
    lightmap = vec4<f32>(lightmap.xyz * (shadow * 0.5 + 0.5), lightmap.w);

    let terrain_color = mix(layer1, layer2, layer2.a) * lightmap * 2.0;

    return apply_zone_lighting(in.world_position, in.world_normal, vec4<f32>(terrain_color.rgb, 1.0), view_z);
}
