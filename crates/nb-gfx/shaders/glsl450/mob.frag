#version 450
#extension GL_GOOGLE_include_directive : require
// Mobs : même chaîne de couleur que le terrain (texture sRGB -> linéaire, éclairage ambiant +
// soleil/lune avec ombres, puis tonemapping dans le composite). Le Java dessinait les mobs sans
// aucun éclairage ni conversion linéaire, ce qui les rendait plats, saturés et lumineux la nuit.
// `vLight` = niveau de lumière du lieu (0..1, ciel ouvert = 1, grotte = bas).
// Location 1 (bright buffer) = (0, 0, 0, alpha) : le bloom du terrain derrière un mob est masqué
// (lave, feuillage lumineux) au lieu de se superposer au mob.
#include "frame.glsl"
layout (location = 0) in vec2 vUV;
layout (location = 1) in vec4 vTint;
layout (location = 2) in float vBrightness;
layout (location = 3) in vec4 vLightSpace;
layout (location = 4) flat in float vLight;
layout (set = 0, binding = 2) uniform sampler2D shadowMap;
layout (set = 1, binding = 0) uniform sampler2D uTexture;
layout (location = 0) out vec4 FragColor;
layout (location = 1) out vec4 BrightColor;

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
    float light = max(vLight, 0.08);
    // Lumière du lieu appliquée à l'ambiant et au soleil, comme la lumière des sommets du terrain.
    vec3 lit = albedo * vBrightness * light * (u.ambientColor.xyz + u.lightColor.xyz * (1.0 - shadow));
    FragColor = vec4(lit, texColor.a);
    BrightColor = vec4(0.0, 0.0, 0.0, texColor.a);
}
