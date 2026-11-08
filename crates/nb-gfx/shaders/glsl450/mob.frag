#version 450
#extension GL_GOOGLE_include_directive : require
// Mobs : même chaîne de couleur que le terrain (texture sRGB -> linéaire, éclairage ambiant +
// soleil/lune avec ombres, puis tonemapping dans le composite). Le Java dessinait les mobs sans
// aucun éclairage ni conversion linéaire, ce qui les rendait plats, saturés et lumineux la nuit.
// `pc.params.x` = niveau de lumière du lieu (0..1, ciel ouvert = 1, grotte = bas).
// Seule la sortie location 0 est écrite ; la pipeline n'attache pas le bright buffer (§14.1).
#include "frame.glsl"
layout (location = 0) in vec2 vUV;
layout (location = 1) in vec4 vTint;
layout (location = 2) in float vBrightness;
layout (location = 3) in vec4 vLightSpace;
layout (set = 0, binding = 2) uniform sampler2D shadowMap;
layout (set = 1, binding = 0) uniform sampler2D uTexture;
layout (push_constant) uniform Push { mat4 model; vec4 params; } pc;
layout (location = 0) out vec4 FragColor;

// Part d'ombre [0,1] : PCF 3x3, comme le terrain (faces des mobs sans normale : biais fixe).
float mobShadow(vec4 ls) {
    vec3 p = ls.xyz / ls.w;
    p.xy = p.xy * 0.5 + 0.5;
    if (p.z > 1.0 || p.x < 0.0 || p.x > 1.0 || p.y < 0.0 || p.y > 1.0) return 0.0;
    vec2 texel = 1.0 / vec2(textureSize(shadowMap, 0));
    float s = 0.0;
    for (int x = -1; x <= 1; ++x)
        for (int y = -1; y <= 1; ++y)
            s += p.z - 0.003 > texture(shadowMap, p.xy + vec2(x, y) * texel).r ? 1.0 : 0.0;
    return s / 9.0;
}

void main() {
    vec4 texColor = texture(uTexture, vUV) * vTint;
    if (texColor.a < 0.01) discard;
    vec3 albedo = pow(texColor.rgb, vec3(2.2));
    float shadow = mobShadow(vLightSpace);
    float light = max(pc.params.x, 0.08);
    // Lumière du lieu appliquée à l'ambiant et au soleil, comme la lumière des sommets du terrain.
    vec3 lit = albedo * vBrightness * light * (u.ambientColor.xyz + u.lightColor.xyz * (1.0 - shadow));
    FragColor = vec4(lit, texColor.a);
}
