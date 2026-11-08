#version 450
#extension GL_GOOGLE_include_directive : require
// Port Vulkan de mob.vert (Java) : `projection` = P_vk (UBO), `model` et `params` (x = niveau de
// lumière du lieu, lu par mob.frag) en push constant. Ajout : position en espace lumière pour les
// ombres reçues.
#include "frame.glsl"
layout (location = 0) in vec3 aPos;
layout (location = 1) in vec2 aUV;
layout (location = 2) in vec4 aTint;
layout (location = 3) in float aBrightness;
layout (push_constant) uniform Push { mat4 model; vec4 params; } pc;
layout (location = 0) out vec2 vUV;
layout (location = 1) out vec4 vTint;
layout (location = 2) out float vBrightness;
layout (location = 3) out vec4 vLightSpace;
void main() {
    vUV         = aUV;
    vTint       = aTint;
    vBrightness = aBrightness;
    vec4 world  = pc.model * vec4(aPos, 1.0);
    vLightSpace = u.lightSpaceVk * world;
    gl_Position = u.projVk * u.view * world;
}
