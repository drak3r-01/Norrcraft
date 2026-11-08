#version 450
#extension GL_GOOGLE_include_directive : require
// Particules instanciées. Atlas des particules (set 1) ou atlas des blocs (set 0, binding 1)
// selon le drapeau 1. Les particules émissives écrivent leur couleur dans le tampon de bloom ;
// les autres le masquent (location 1 = (0, 0, 0, alpha)), comme avant.
#include "frame.glsl"
layout (location = 0) in vec2 TexCoords;
layout (location = 1) in vec4 vColor;
layout (location = 2) flat in uint vLayer;
layout (location = 3) flat in uint vFlags;
layout (set = 0, binding = 1) uniform sampler2DArray blockAtlas;
layout (set = 1, binding = 0) uniform sampler2DArray textureArray;
layout (location = 0) out vec4 FragColor;
layout (location = 1) out vec4 BrightColor;
void main() {
    vec4 tex;
    if ((vFlags & 1u) != 0u) tex = texture(blockAtlas, vec3(TexCoords, float(vLayer)));
    else tex = texture(textureArray, vec3(TexCoords, float(vLayer)));
    float a = tex.a * vColor.a;
    if (a < 0.04)
        discard;
    vec3 rgb = tex.rgb * vColor.rgb;
    FragColor = vec4(rgb, a);
    BrightColor = (vFlags & 2u) != 0u ? vec4(rgb * a, a) : vec4(0.0, 0.0, 0.0, a);
}
