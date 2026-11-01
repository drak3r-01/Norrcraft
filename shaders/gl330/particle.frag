#version 330 core
in vec2 TexCoords;
out vec4 FragColor;

uniform sampler2DArray textureArray;
uniform int layer;
uniform vec3 colorTint;

void main() {
    vec4 texColor = texture(textureArray, vec3(TexCoords, float(layer)));
    if(texColor.a < 0.1)
        discard;
    FragColor = vec4(texColor.rgb * colorTint, texColor.a);
}