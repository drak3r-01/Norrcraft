#version 450
// FragColor = texel × teinte (UIRenderer : colorTint ; ChatRenderer : texel blanc ;
// BlockIconRenderer : teinte blanche). UV en espace image (v = 0 en haut).
layout (location = 0) in vec2 TexCoord;
layout (location = 1) in vec4 Tint;
layout (set = 0, binding = 0) uniform sampler2D uiTexture;
layout (set = 0, binding = 1) uniform sampler2DArray blockTextures;
layout (push_constant) uniform Push {
    vec2 screen;
    int mode;
    float layer;
} pc;
layout (location = 0) out vec4 FragColor;
void main() {
    vec4 texel = pc.mode == 0 ? texture(uiTexture, TexCoord)
                              : texture(blockTextures, vec3(TexCoord, pc.layer));
    FragColor = texel * Tint;
}
