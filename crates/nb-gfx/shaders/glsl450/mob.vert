#version 450
#extension GL_GOOGLE_include_directive : require
// Port Vulkan de mob.vert (Java) : `projection` = P_vk (UBO), `model` en push constant.
#include "frame.glsl"
layout (location = 0) in vec3 aPos;
layout (location = 1) in vec2 aUV;
layout (location = 2) in vec4 aTint;
layout (location = 3) in float aBrightness;
layout (push_constant) uniform Push { mat4 model; } pc;
layout (location = 0) out vec2 vUV;
layout (location = 1) out vec4 vTint;
layout (location = 2) out float vBrightness;
void main() {
    vUV         = aUV;
    vTint       = aTint;
    vBrightness = aBrightness;
    gl_Position = u.projVk * u.view * pc.model * vec4(aPos, 1.0);
}
