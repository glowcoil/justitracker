#version 140
uniform sampler2D tex;
in vec2 v_tex_coords;
in vec4 v_colour;
out vec4 f_colour;

void main() {
    f_colour = v_colour * texture(tex, v_tex_coords).rrrr;
}
