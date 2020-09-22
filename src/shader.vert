#version 450

layout(location=0) in vec2 a_position;
layout(location=1) in vec2 a_size;
layout(location=2) in uvec4 a_tex;

layout(location=0) out vec2 v_tex_coords;

layout(set = 0, binding = 0) uniform Uniforms {
    mat4 projection_matrix;
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
    vec2 quad = positions[gl_VertexIndex];
    switch(gl_VertexIndex){
        case 0: v_tex_coords = vec2(a_tex.ra); break;
        case 1: v_tex_coords = vec2(a_tex.ba); break;
        case 2: v_tex_coords = vec2(a_tex.bg); break;
        case 3: v_tex_coords = vec2(a_tex.ra); break;
        case 4: v_tex_coords = vec2(a_tex.bg); break;
        case 5: v_tex_coords = vec2(a_tex.rg); break;
    }
    vec4 position = vec4(quad*a_size + a_position, 0.0, 1.0);
    gl_Position = projection_matrix*position;
}
