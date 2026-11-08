#version 450
// Port Vulkan de particle.frag (Java). Ajout : location 1 (bright buffer) = (0, 0, 0, alpha), pour que
// le bloom du terrain situé derrière une particule soit masqué (voir pipelines.rs, passe B2).
layout (location = 0) in vec2 TexCoords;
layout (set = 1, binding = 0) uniform sampler2DArray textureArray;
layout (push_constant) uniform Push { mat4 model; vec3 colorTint; int layer; } pc;
layout (location = 0) out vec4 FragColor;
layout (location = 1) out vec4 BrightColor;
void main() {
    vec4 texColor = texture(textureArray, vec3(TexCoords, float(pc.layer)));
    if (texColor.a < 0.1)
        discard;
    FragColor = vec4(texColor.rgb * pc.colorTint, texColor.a);
    BrightColor = vec4(0.0, 0.0, 0.0, texColor.a);
}
