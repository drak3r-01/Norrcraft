// Bloc d'uniformes par frame partagé par tous les shaders (std140, que des vec4/mat4).
// Miroir exact de `nb_gfx::frame::FrameUniforms` (même ordre, même taille : 496 octets).
// Remplace les uniformes « libres » GLSL 330 (spec nb-gfx §18.2).
layout(set = 0, binding = 0, std140) uniform Frame {
    mat4 projVk;        // C · P_gl (projection caméra corrigée Vulkan, §18.1)
    mat4 view;          // V
    mat4 lightSpaceVk;  // C · Lp_gl · Lv
    mat4 inverseView;   // V⁻¹
    mat4 inverseProjGl; // P_gl⁻¹ (pas P_vk⁻¹, §18.1 point 4)
    vec4 cameraPos;     // xyz = position caméra, w = uTime
    vec4 lightDir;      // xyz = shadowLightDir (uLightDir)
    vec4 ambientColor;
    vec4 lightColor;
    vec4 fogColor;      // (r,g,b) §8.1 ; w = densité de brouillard ajoutée par la météo (air)
    vec4 zenithColor;
    vec4 horizonColor;
    vec4 sunColor;
    vec4 projGl;        // x = P_gl[2][2], y = P_gl[3][2], z = W, w = H (uScreenSize)
    vec4 env;           // x = auroraIntensity, y = auroraPhase, z = heatIntensity, w = nuit (0 jour, 1 nuit)
    ivec4 flags;        // x = fluidState (0 air / 1 eau / 2 lave), y = correctif seuils d'ombre, z = 1 si lune, w = couche d'atlas de la lave (-1 = aucune)
} u;
