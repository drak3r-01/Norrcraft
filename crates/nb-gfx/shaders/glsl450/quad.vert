#version 450
// Remplace blur.vert / pp.vert : le quad plein écran Java (4 sommets en strip,
// `(-1,1,0, 0,1)`, `(-1,-1,0, 0,0)`, `(1,1,0, 1,1)`, `(1,-1,0, 1,0)`) est généré à partir de
// gl_VertexIndex. Positions utilisées telles quelles en NDC Vulkan (§18.1) : uv.v = 0 = haut.
layout (location = 0) out vec2 TexCoords;
void main() {
    vec2 pos = vec2((gl_VertexIndex & 2) != 0 ? 1.0 : -1.0, (gl_VertexIndex & 1) != 0 ? -1.0 : 1.0);
    TexCoords = pos * 0.5 + 0.5;
    gl_Position = vec4(pos, 0.0, 1.0);
}
