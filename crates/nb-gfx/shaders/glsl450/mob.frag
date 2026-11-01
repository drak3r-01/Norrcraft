#version 450
// Port Vulkan de mob.frag (Java, inchangé) : aucun éclairage ni gamma (spec §9.6). Seule la
// sortie location 0 est écrite ; la pipeline masque l'attachement 1 (bright buffer, §14.1).
layout (location = 0) in vec2 vUV;
layout (location = 1) in vec4 vTint;
layout (location = 2) in float vBrightness;
layout (set = 1, binding = 0) uniform sampler2D uTexture;
layout (location = 0) out vec4 FragColor;
void main() {
    vec4 texColor = texture(uTexture, vUV);
    FragColor = texColor * vTint * vec4(vBrightness, vBrightness, vBrightness, 1.0);
    if (FragColor.a < 0.01) discard;
}
