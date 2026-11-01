#version 450
// Port Vulkan de shaders/gl330/shadow.frag (inchangé).
layout (location = 0) in vec2 TexCoord;
layout (location = 1) in float Layer;
layout (set = 0, binding = 1) uniform sampler2DArray textureArray;
void main() {
    vec4 texColor = texture(textureArray, vec3(TexCoord, Layer));
    if(texColor.a < 0.5) discard;
}
