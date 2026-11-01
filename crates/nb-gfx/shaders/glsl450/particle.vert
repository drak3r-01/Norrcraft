#version 450
#extension GL_GOOGLE_include_directive : require
// Port Vulkan de particle.vert (Java) : `projection` = P_vk (UBO) ; `model`, `colorTint` et
// `layer` en push constants (80 octets, même bloc que particle.frag).
#include "frame.glsl"
layout (location = 0) in vec3 aPos;
layout (location = 1) in vec2 aTexCoords;
layout (push_constant) uniform Push { mat4 model; vec3 colorTint; int layer; } pc;
layout (location = 0) out vec2 TexCoords;
void main() {
    TexCoords = aTexCoords;
    gl_Position = u.projVk * u.view * pc.model * vec4(aPos, 1.0);
}
