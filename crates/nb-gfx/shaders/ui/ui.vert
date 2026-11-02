#version 450
// UI 2D (nb-ui) : remplace les shaders GL de UIRenderer / ChatRenderer / BlockIconRenderer.
// Positions en pixels fenêtre, origine en haut à gauche (ortho(0, w, h, 0) Java) ; le viewport
// Vulkan standard (y vers le bas) donne directement NDC = pos / écran · 2 - 1.
layout (location = 0) in vec2 aPos;
layout (location = 1) in vec2 aTexCoord;
layout (location = 2) in vec4 aColor;
layout (push_constant) uniform Push {
    vec2 screen;
    int mode;     // 0 = texture UI 2D, 1 = couche du tableau de blocs
    float layer;
} pc;
layout (location = 0) out vec2 TexCoord;
layout (location = 1) out vec4 Tint;
void main() {
    TexCoord = aTexCoord;
    Tint = aColor;
    gl_Position = vec4(aPos / pc.screen * 2.0 - 1.0, 0.0, 1.0);
}
