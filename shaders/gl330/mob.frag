#version 330 core

in vec2 vUV;
in vec4 vTint;
in float vBrightness;

uniform sampler2D uTexture;

out vec4 FragColor;

void main() {
    vec4 texColor = texture(uTexture, vUV);
    FragColor = texColor * vTint * vec4(vBrightness, vBrightness, vBrightness, 1.0);
    if (FragColor.a < 0.01) discard;
}
