#version 450
#extension GL_GOOGLE_include_directive : require
// Mobs animés : « peau » rigide par os et dessin instancié.
//  * binding 0 (sommet) : aPos, aUV, aTint, aBrightness (MobVertex) + aBone, l'os qui porte la boîte ;
//  * binding 1 (instance) : matrice modèle (iM0..iM3) et paramètres iParams
//      x = niveau de lumière du lieu (lu par mob.frag), y = indice de la 1re matrice de la palette
//      de cette instance ;
//  * set 2 (SSBO, une palette par frame en vol) : 3 vec4 par os = lignes d'une matrice affine 3×4
//      (`nb_anim::Mat34`) qui amène un sommet de la pose de repos à la pose animée, en espace
//      modèle. Le même dessin sert donc N mobs d'un même modèle (draw instancié) ; les modèles
//      statiques utilisent l'os 0 de la palette identité placée en tête du tampon.
// Position en espace lumière pour les ombres reçues.
#include "frame.glsl"
layout (location = 0) in vec3 aPos;
layout (location = 1) in vec2 aUV;
layout (location = 2) in vec4 aTint;
layout (location = 3) in float aBrightness;
layout (location = 4) in uint aBone;
layout (location = 5) in vec4 iM0;
layout (location = 6) in vec4 iM1;
layout (location = 7) in vec4 iM2;
layout (location = 8) in vec4 iM3;
layout (location = 9) in vec4 iParams;
layout (set = 2, binding = 0, std430) readonly buffer Palette { vec4 rows[]; } pal;
layout (location = 0) out vec2 vUV;
layout (location = 1) out vec4 vTint;
layout (location = 2) out float vBrightness;
layout (location = 3) out vec4 vLightSpace;
layout (location = 4) flat out float vLight;
layout (location = 5) out vec3 vWorld;
layout (location = 6) flat out float vWaterY;
void main() {
    vUV         = aUV;
    vTint       = aTint;
    vBrightness = aBrightness;
    vLight      = iParams.x;
    uint m      = (uint(iParams.y) + aBone) * 3u;
    vec4 p4     = vec4(aPos, 1.0);
    vec3 skinned = vec3(dot(pal.rows[m], p4), dot(pal.rows[m + 1u], p4), dot(pal.rows[m + 2u], p4));
    vec4 world  = mat4(iM0, iM1, iM2, iM3) * vec4(skinned, 1.0);
    vWorld      = world.xyz;
    vWaterY     = iParams.z;
    vLightSpace = u.lightSpaceVk * world;
    gl_Position = u.projVk * u.view * world;
}
