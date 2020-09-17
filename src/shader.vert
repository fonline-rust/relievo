#version 450

layout(location=0) in vec2 a_position;
layout(location=1) in vec2 a_size;

layout(location=0) out vec2 v_tex_coords;

layout(set = 0, binding = 0) uniform Uniforms {
    vec2 screen_size;
    vec2 screen_shift;
};

out gl_PerVertex {
    vec4 gl_Position;
};

const vec2 positions[6] = vec2[6](
    vec2(0, 1),
    vec2(1, 1),
    vec2(1, 0),
    vec2(0, 1),
    vec2(1, 0),
    vec2(0, 0)
);

const vec2 screen_space = vec2(0.5, -0.5);

void main() {
    v_tex_coords = positions[gl_VertexIndex];
    vec2 position = (v_tex_coords*a_size + a_position - screen_shift) / (screen_size * screen_space) + vec2(-1.0, 1.0);
    gl_Position = vec4(position, 0.0, 1.0);
}
