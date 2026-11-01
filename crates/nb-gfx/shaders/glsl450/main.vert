#version 450
#extension GL_GOOGLE_include_directive : require
// Port Vulkan de shaders/gl330/main.vert. Seule modification (§18.1 point 1) :
// `projection` = P_vk et `lightSpaceMatrix` = L_vk.
#include "frame.glsl"
layout (location = 0) in vec3 aPos;
layout (location = 1) in vec2 aTexCoord;
layout (location = 2) in float aLayer;
layout (location = 3) in vec3 aColor;
layout (location = 0) out vec2 TexCoord;
layout (location = 1) out float Layer;
layout (location = 2) out vec3 Color;
layout (location = 3) out vec3 FragPos;
layout (location = 4) out vec4 FragPosLightSpace;
layout (location = 5) out float IsFluid;
layout (location = 6) out float IsLeaf;
void main() {
    float uTime = u.cameraPos.w;
    float actualLayer = aLayer;
    float fluid = 0.0;
    float leaf = 0.0;
    vec3 pos = aPos;
    if (aLayer < 0.0) {
        actualLayer = -aLayer - 1.0;
        fluid = 1.0;
        if (pos.y - floor(pos.y) > 0.7) {
            if (aColor.r > 0.6 && aColor.b < 0.2) {
                float w1 = sin(pos.x * 0.8 + uTime * 0.3) * cos(pos.z * 0.8 + uTime * 0.3) * 0.06f;
                pos.y += w1;
            } else {
                float w1 = sin(pos.x * 2.5 + uTime * 2.0) * cos(pos.z * 2.5 + uTime * 2.0) * 0.2f;
                float w2 = sin(pos.x * 1.25 - uTime * 1.6) * sin(pos.z * 1.75 + uTime * 2.2) * 0.1f;
                pos.y += w1 + w2;
            }
        }
    } else if (aLayer >= 200000.0) {
        actualLayer = aLayer - 200000.0;
        leaf = 1.0;
        float windTime = uTime * 1.2;
        float sway = sin(pos.x * 0.5 + pos.y * 0.2 + pos.z * 0.5 + windTime) * 0.05;
        float rustle = sin(pos.x * 3.0 + pos.y * 2.0 + pos.z * 3.0 + windTime * 2.5) * 0.015;
        pos.x += sway + rustle;
        pos.y += rustle * 0.5;
        pos.z += sway + rustle;
    } else if (aLayer >= 100000.0) {
        actualLayer = aLayer - 100000.0;
        if (aTexCoord.y > 0.5) {
            float windTime = uTime * 1.5;
            float sway = sin(pos.x * 1.5 + pos.y * 0.5 + pos.z * 1.5 + windTime) * 0.1;
            pos.x += sway;
            pos.z += sway;
        }
    }
    Layer = actualLayer;
    Color = aColor;
    IsFluid = fluid;
    IsLeaf = leaf;
    TexCoord = aTexCoord;
    FragPos = pos;
    FragPosLightSpace = u.lightSpaceVk * vec4(pos, 1.0);
    gl_Position = u.projVk * u.view * vec4(pos, 1.0);
}
