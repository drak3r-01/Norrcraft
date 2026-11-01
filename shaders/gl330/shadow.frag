#version 330 core
in vec2 TexCoord;
in float Layer;
uniform sampler2DArray textureArray;
void main() {
    vec4 texColor = texture(textureArray, vec3(TexCoord, Layer));
    if(texColor.a < 0.5) discard;
}