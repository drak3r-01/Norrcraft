#version 450
// Port Vulkan de shaders/gl330/blur.frag : `weight` en const, `horizontal` en push constant.
layout (location = 0) out vec4 FragColor;
layout (location = 0) in vec2 TexCoords;
layout (set = 0, binding = 0) uniform sampler2D image;
layout (push_constant) uniform Push { int horizontal; } pc;
const float weight[5] = float[] (0.227027, 0.1945946, 0.1216216, 0.054054, 0.016216);
void main() {
    vec2 tex_offset = 1.0 / vec2(textureSize(image, 0));
    vec3 result = texture(image, TexCoords).rgb * weight[0];
    if(pc.horizontal != 0) {
        for(int i = 1; i < 5; ++i) {
            result += texture(image, TexCoords + vec2(tex_offset.x * i, 0.0)).rgb * weight[i];
            result += texture(image, TexCoords - vec2(tex_offset.x * i, 0.0)).rgb * weight[i];
        }
    } else {
        for(int i = 1; i < 5; ++i) {
            result += texture(image, TexCoords + vec2(0.0, tex_offset.y * i)).rgb * weight[i];
            result += texture(image, TexCoords - vec2(0.0, tex_offset.y * i)).rgb * weight[i];
        }
    }
    FragColor = vec4(result, 1.0);
}
