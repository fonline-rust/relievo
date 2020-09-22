#version 450

layout(location=0) in vec2 v_tex_coords;

layout(location = 0) out vec4 outColor;

layout(set = 1, binding = 0) uniform texture2D t_diffuse;
layout(set = 1, binding = 1) uniform sampler s_diffuse;

void main() {
    vec4 color = texelFetch(sampler2D(t_diffuse, s_diffuse), ivec2(v_tex_coords), 0);
    //outColor = vec4(color.rgb * color.a, color.a);
    outColor = color;
}
