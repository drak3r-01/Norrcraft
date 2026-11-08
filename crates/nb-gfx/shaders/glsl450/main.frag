#version 450
#extension GL_GOOGLE_include_directive : require
// Terrain : passe opaque (uRenderPass = 0, couches alpha-testées) et passe fluides (= 1, mélangée).
// Écrit la couleur HDR (location 0) et la source du bloom (location 1).
//
// Port Vulkan de shaders/gl330/main.frag. Modifications (liste fermée §18.1) :
//  - `uRenderPass` devient une constante de spécialisation (une pipeline par passe) ;
//  - ShadowCalculation : p.z = ndc.z (pas de *0.5+0.5, la profondeur Vulkan est déjà dans [0,1]) ;
//  - getLinearDepth : coefficients de P_gl (u.projGl), formule inchangée.
// Remanié pour la lisibilité (constantes nommées, sections) sans changer le résultat.
#include "frame.glsl"
layout (constant_id = 0) const int uRenderPass = 0;
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
layout (set = 1, binding = 0) uniform sampler2D opaqueDepth;

const int PASS_OPAQUE = 0;
const int PASS_FLUID  = 1;

// Feuillage : rétro-éclairage (transmission de la lumière à travers les feuilles).
const float LEAF_SSS_POWER    = 3.0;
const float LEAF_SSS_STRENGTH = 0.6;
// Fluides : reflet du soleil et absorption.
const float FLUID_SPEC_POWER = 48.0;
const float FLUID_R0         = 0.02;   // réflectance de Fresnel à incidence normale
const float WATER_DENSITY    = 0.08;
const float LAVA_DENSITY     = 1.5;

// Profondeur [0,1] (fenêtre GL) -> distance à l'œil.
float getLinearDepth(float depth) {
    float zNdc = depth * 2.0 - 1.0;
    return u.projGl.y / (zNdc + u.projGl.x);
}

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
    bool fluid = IsFluid > 0.5;
    bool leaf = IsLeaf > 0.5;

    // Chaque passe ne dessine que sa moitié de la géométrie.
    if (uRenderPass == PASS_OPAQUE && fluid) discard;
    if (uRenderPass == PASS_FLUID && !fluid) discard;

    // ------------------------------------------------------------------ albédo
    vec4 texColor = texture(textureArray, vec3(TexCoord, Layer));
    float alpha = texColor.a;
    if (uRenderPass == PASS_OPAQUE) {
        // Alpha-test net, lissé par fwidth (bords des feuilles et plantes sans crénelage).
        if (texColor.a < 0.01) discard;
        texColor.rgb = clamp(texColor.rgb / texColor.a, 0.0, 1.0);
        alpha = (texColor.a - 0.5) / max(fwidth(texColor.a), 0.0001) + 0.5;
        if (alpha < 0.5) discard;
    } else if (alpha < 0.05) {
        discard;
    }
    vec3 linearTex = pow(texColor.rgb, vec3(2.2));
    vec3 albedo = linearTex * Color;

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

    // ------------------------------------------------------------------ fluides
    float spec = 0.0;
    bool isLava = fluid && linearTex.r > 0.6 && linearTex.b < 0.2;
    if (fluid) {
        // Épaisseur d'eau traversée : écart de profondeur avec le terrain opaque dessous.
        float opaqueDepthVal = texelFetch(opaqueDepth, ivec2(gl_FragCoord.xy), 0).r;
        float thickness = max(getLinearDepth(opaqueDepthVal) - getLinearDepth(gl_FragCoord.z), 0.0);

        vec3 reflectDir = reflect(normalize(lightDir), normal);
        spec = pow(max(dot(viewDir, reflectDir), 0.0), FLUID_SPEC_POWER);
        float cosTheta = max(dot(viewDir, normal), 0.0);
        float fresnel = FLUID_R0 + (1.0 - FLUID_R0) * pow(1.0 - cosTheta, 5.0);
        float density = isLava ? LAVA_DENSITY : WATER_DENSITY;
        float depthAlpha = 1.0 - exp(-thickness * density);
        alpha = clamp(mix(depthAlpha, 1.0, fresnel), isLava ? 0.9 : 0.3, 0.98);

        vec3 deepColor    = albedo * (isLava ? vec3(2.5, 1.0, 0.1) : vec3(0.25));
        vec3 shallowColor = albedo * (isLava ? vec3(3.0, 1.5, 0.2) : vec3(1.0));
        finalColor = mix(shallowColor, deepColor, depthAlpha);
        finalColor += lightColor * 2.0 * spec * (1.0 - shadow);
    }
    FragColor = vec4(finalColor, alpha);

    // ------------------------------------------------------------------ source du bloom
    if (isLava) {
        BrightColor = vec4(FragColor.rgb * 1.8, 1.0);
    } else if (spec > 0.2 || (leaf && finalColor.g > 1.2)) {
        BrightColor = vec4(FragColor.rgb, 1.0);
    } else {
        BrightColor = vec4(0.0, 0.0, 0.0, 1.0);
    }
}
