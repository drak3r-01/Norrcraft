#version 450
#extension GL_GOOGLE_include_directive : require
// Port Vulkan de shaders/gl330/shadow.vert. `lightSpaceMatrix` = L_vk (§18.1 point 1).
// [JAVA-BUG] gardé : seuils 2000/1000 (feuilles et croix projettent des ombres pleines, §9.2).
// Correctif optionnel derrière u.flags.y != 0 (actif dans le jeu) : seuils 200000/100000.
// Avec le correctif : l'eau, la lave et les plantes en croix (herbes hautes, fleurs) ne projettent plus
// d'ombre. Leurs ombres découpées tachetaient les mobs et le sol de motifs d'herbe (les mobs
// semblaient transparents) ; seuls les blocs pleins et les feuilles en projettent.
#include "frame.glsl"
layout (location = 0) in vec3 aPos;
layout (location = 1) in vec2 aTexCoord;
layout (location = 2) in float aLayer;
layout (location = 0) out vec2 TexCoord;
layout (location = 1) out float Layer;
void main() {
    float uTime = u.cameraPos.w;
    float leafOffset = u.flags.y != 0 ? 200000.0 : 2000.0;
    float crossOffset = u.flags.y != 0 ? 100000.0 : 1000.0;
    float actualLayer = aLayer;
    vec3 pos = aPos;
    bool noCast = u.flags.y != 0;
    if (aLayer < 0.0) {
        if (noCast) { gl_Position = vec4(0.0, 0.0, 2.0, 1.0); Layer = 0.0; TexCoord = vec2(0.0); return; }
        actualLayer = -aLayer - 1.0;
        if (pos.y - floor(pos.y) > 0.7) {
            float w1 = sin(pos.x * 2.5 + uTime * 2.0) * cos(pos.z * 2.5 + uTime * 2.0) * 0.2f;
            float w2 = sin(pos.x * 1.25 - uTime * 1.6) * sin(pos.z * 1.75 + uTime * 2.2) * 0.1f;
            pos.y += w1 + w2;
        }
    } else if (aLayer >= leafOffset) {
        actualLayer = aLayer - leafOffset;
        float windTime = uTime * 1.2;
        float sway = sin(pos.x * 0.5 + pos.y * 0.2 + pos.z * 0.5 + windTime) * 0.05;
        float rustle = sin(pos.x * 3.0 + pos.y * 2.0 + pos.z * 3.0 + windTime * 2.5) * 0.015;
        pos.x += sway + rustle;
        pos.y += rustle * 0.5;
        pos.z += sway + rustle;
    } else if (aLayer >= crossOffset) {
        if (noCast) { gl_Position = vec4(0.0, 0.0, 2.0, 1.0); Layer = 0.0; TexCoord = vec2(0.0); return; }
        actualLayer = aLayer - crossOffset;
        if (aTexCoord.y > 0.5) {
            float windTime = uTime * 1.5;
            float sway = sin(pos.x * 1.5 + pos.y * 0.5 + pos.z * 1.5 + windTime) * 0.1;
            pos.x += sway;
            pos.z += sway;
        }
    }
    Layer = actualLayer;
    TexCoord = aTexCoord;
    gl_Position = u.lightSpaceVk * vec4(pos, 1.0);
}
