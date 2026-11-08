#version 450
#extension GL_GOOGLE_include_directive : require
// Terrain : passe opaque (couches alpha-testées). Les fluides (eau, lave) ont leur propre shader,
// fluid.frag. Écrit la couleur HDR (location 0) et la source du bloom (location 1).
//
// Port Vulkan de shaders/gl330/main.frag. Modifications (liste fermée §18.1) :
//  - la passe fluides est sortie dans fluid.frag (elle échantillonne aussi une copie de l'image) ;
//  - ShadowCalculation : p.z = ndc.z (pas de *0.5+0.5, la profondeur Vulkan est déjà dans [0,1]) ;
// Remanié pour la lisibilité (constantes nommées, sections) sans changer le résultat, sauf :
//  - la couleur des texels de bord n'est plus « dépremultipliée » : la chaîne de mips est désormais
//    calculée avec une couleur pondérée par l'alpha (atlas.rs), ce qui supprime le liseré clair
//    autour des feuilles et des herbes.
#include "frame.glsl"
layout (location = 0) out vec4 FragColor;
layout (location = 1) out vec4 BrightColor;
layout (location = 0) in vec2 TexCoord;
layout (location = 1) in float Layer;
layout (location = 2) in vec3 Color;
layout (location = 3) in vec3 FragPos;
layout (location = 4) in vec4 FragPosLightSpace;
layout (location = 5) in float IsFluid;
layout (location = 6) in float IsLeaf;
layout (set = 0, binding = 1) uniform sampler2DArray textureArray;
layout (set = 0, binding = 2) uniform sampler2D shadowMap;

// Variation de teinte à grande échelle de la végétation teintée par le biome (herbe, feuilles) :
// sans elle, de vastes aplats d'un vert uniforme donnent un aspect plastique.
const float VEG_SATURATION_MIN = 0.40;

float hash21(vec2 p) {
    p = fract(p * vec2(123.34, 456.21));
    p += dot(p, p + 45.32);
    return fract(p.x * p.y);
}

float valueNoise(vec2 p) {
    vec2 i = floor(p), f = fract(p);
    f = f * f * (3.0 - 2.0 * f);
    return mix(mix(hash21(i), hash21(i + vec2(1.0, 0.0)), f.x),
               mix(hash21(i + vec2(0.0, 1.0)), hash21(i + vec2(1.0, 1.0)), f.x), f.y);
}

// Feuillage : rétro-éclairage (transmission de la lumière à travers les feuilles).
const float LEAF_SSS_POWER    = 3.0;
const float LEAF_SSS_STRENGTH = 0.6;

// Part d'ombre [0,1] : PCF 3x3 sur la carte d'ombre.
float ShadowCalculation(vec4 fragPosLightSpace, vec3 normal) {
    vec3 projCoords = fragPosLightSpace.xyz / fragPosLightSpace.w;
    projCoords.xy = projCoords.xy * 0.5 + 0.5;
    if (projCoords.z > 1.0) return 0.0;
    float currentDepth = projCoords.z;
    float bias = max(0.005 * (1.0 - dot(normal, -u.lightDir.xyz)), 0.001);
    float shadow = 0.0;
    vec2 texelSize = 1.0 / vec2(textureSize(shadowMap, 0));
    for (int x = -1; x <= 1; ++x) {
        for (int y = -1; y <= 1; ++y) {
            float pcfDepth = texture(shadowMap, projCoords.xy + vec2(x, y) * texelSize).r;
            shadow += currentDepth - bias > pcfDepth ? 1.0 : 0.0;
        }
    }
    return shadow / 9.0;
}

void main() {
    vec3 camPos = u.cameraPos.xyz;
    vec3 lightDir = u.lightDir.xyz;
    vec3 ambient = u.ambientColor.xyz;
    vec3 lightColor = u.lightColor.xyz;
    bool leaf = IsLeaf > 0.5;

    // Les fluides sont dessinés par fluid.frag.
    if (IsFluid > 0.5) discard;

    // ------------------------------------------------------------------ albédo
    vec4 texColor = texture(textureArray, vec3(TexCoord, Layer));
    // Alpha-test net, lissé par fwidth (bords des feuilles et plantes sans crénelage).
    if (texColor.a < 0.01) discard;
    float alpha = (texColor.a - 0.5) / max(fwidth(texColor.a), 0.0001) + 0.5;
    if (alpha < 0.5) discard;
    vec3 linearTex = pow(texColor.rgb, vec3(2.2));
    vec3 albedo = linearTex * Color;
    // Végétation (couleur de sommet saturée : teinte de biome) : patchs plus secs / plus sombres. Le facteur moyen
    // (~0,8) compense le x2,2 de la teinte du mailleur, qui saturait les verts après tonemapping.
    float vMax = max(max(Color.r, Color.g), Color.b);
    float vMin = min(min(Color.r, Color.g), Color.b);
    if ((vMax - vMin) > VEG_SATURATION_MIN * vMax) {
        vec2 q = FragPos.xz + vec2(FragPos.y * 0.37, -FragPos.y * 0.29);
        float n = 0.6 * valueNoise(q * 0.05) + 0.4 * valueNoise(q * 0.23 + 7.0);
        n = smoothstep(0.30, 0.70, n);
        albedo *= mix(vec3(0.60, 0.76, 0.64), vec3(1.00, 0.93, 0.74), n);
        // Petite variation par bloc (comme les herbes du jeu d'origine).
        albedo *= 0.94 + 0.12 * hash21(floor(q * 1.0001));
    }

    // ------------------------------------------------------------------ normale + ombre
    vec3 normal = normalize(cross(dFdx(FragPos), dFdy(FragPos)));
    vec3 viewDir = normalize(camPos - FragPos);
    if (dot(normal, viewDir) < 0.0) normal = -normal;
    float shadow = ShadowCalculation(FragPosLightSpace, normal);

    // ------------------------------------------------------------------ éclairage
    vec3 finalColor = albedo * (ambient + lightColor * (1.0 - shadow));
    if (leaf) {
        float backlight = max(0.0, dot(viewDir, normalize(-lightDir)));
        float sss = pow(backlight, LEAF_SSS_POWER) * LEAF_SSS_STRENGTH;
        float wrapLight = max(0.0, dot(normal, lightDir) * 0.5 + 0.5);
        finalColor += albedo * lightColor * sss * wrapLight * (1.0 - shadow * 0.7);
    }
    FragColor = vec4(finalColor, 1.0);

    // ------------------------------------------------------------------ source du bloom
    if (leaf && finalColor.g > 1.2) {
        BrightColor = vec4(FragColor.rgb, 1.0);
    } else {
        BrightColor = vec4(0.0, 0.0, 0.0, 1.0);
    }
}
