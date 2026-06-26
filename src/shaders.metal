// Metal shaders for full-screen texture rendering
#include <metal_stdlib>
using namespace metal;

struct VertexOut {
    float4 position [[position]];
    float2 textureCoord;
};

// Vertex shader: full-screen quad (NDC -1..1)
vertex VertexOut fullscreen_quad(
    uint vertexID [[vertex_id]]
) {
    // Triangle strip: 4 vertices covering entire screen
    float4 positions[4] = {
        float4(-1.0, -1.0, 0.0, 1.0),  // bottom-left
        float4( 1.0, -1.0, 0.0, 1.0),  // bottom-right
        float4(-1.0,  1.0, 0.0, 1.0),  // top-left
        float4( 1.0,  1.0, 0.0, 1.0)   // top-right
    };

    float2 texCoords[4] = {
        float2(0.0, 1.0),  // bottom-left (flip Y for Metal)
        float2(1.0, 1.0),  // bottom-right
        float2(0.0, 0.0),  // top-left
        float2(1.0, 0.0)   // top-right
    };

    VertexOut out;
    out.position = positions[vertexID];
    out.textureCoord = texCoords[vertexID];
    return out;
}

// Fragment shader: sample texture
fragment float4 texture_fragment(
    VertexOut in [[stage_in]],
    texture2d<float> texture [[texture(0)]]
) {
    constexpr sampler s(coord::normalized, address::clamp_to_edge, filter::linear);
    return texture.sample(s, in.textureCoord);
}
