#version 450
#extension GL_GOOGLE_include_directive : require
// Port Vulkan de shaders/gl330/hl.vert : `projection` = P_vk, `model` en push constant.
#include "frame.glsl"
layout (location = 0) in vec3 aPos;
layout (push_constant) uniform Push { mat4 model; } pc;
void main() {
    gl_Position = u.projVk * u.view * pc.model * vec4(aPos, 1.0);
}
