#version 330 core

layout(location = 0) in vec3 aPos;
layout(location = 1) in vec2 aUV;
layout(location = 2) in vec4 aTint;
layout(location = 3) in float aBrightness;

uniform mat4 model;
uniform mat4 view;
uniform mat4 projection;

out vec2 vUV;
out vec4 vTint;
out float vBrightness;

void main() {
    vUV         = aUV;
    vTint       = aTint;
    vBrightness = aBrightness;
    gl_Position = projection * view * model * vec4(aPos, 1.0);
}
