struct Uniforms {
    view_transform: mat4x4<f32>,
    model_transform: mat4x4<f32>,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) world_pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
};

@group(0)
@binding(0)
var<uniform> uniforms: Uniforms;

@vertex
fn vs_main(
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
) -> VertexOutput {
    var result: VertexOutput;
    result.position = uniforms.view_transform * uniforms.model_transform * vec4<f32>(position, 1.0);
    result.world_pos = position;
    result.normal = (uniforms.model_transform * vec4<f32>(normal, 0.0)).xyz;

    return result;
}

@fragment
fn fs_main(vertex: VertexOutput) -> @location(0) vec4<f32> {
    var cam = normalize(vertex.world_pos - vec3<f32>(0.0, 150.0, 0.0));

    var a = vec3<f32>(0.1, 0.1, 0.1);
    var d = vec3<f32>(0.5, 0.5, 1.0);
    var s = vec3<f32>(0.3, 0.3, 0.3);
    var p = 8.0;

    var l1 = vec3<f32>(0.0, -1.0, 0.0);
    var l2 = normalize(vec3<f32>(0.5, 1.0, 1.0));

    var r = reflect(cam, vertex.normal);
    var ndotl1 = max(0.0, dot(vertex.normal, l1));
    var ndotl2 = max(0.0, dot(vertex.normal, l2));
    var rdotl1 = max(0.0, dot(r, l1));
    var rdotl2 = max(0.0, dot(r, l2));

    var phong1 = d * ndotl1 + s * pow(rdotl1, p);
    var phong2 = d * ndotl2 + s * pow(rdotl2, p);

    return vec4<f32>(d * (a + phong1 + phong2), 1.0);
}
