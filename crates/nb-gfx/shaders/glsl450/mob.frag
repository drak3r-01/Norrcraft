#version 450
#extension GL_GOOGLE_include_directive : require
// Mobs : même chaîne de couleur que le terrain (texture sRGB -> linéaire, éclairage ambiant +
// soleil/lune avec ombres douces, puis tonemapping dans le composite). Le Java dessinait les mobs
// sans aucun éclairage ni conversion linéaire, ce qui les rendait plats, saturés et lumineux la nuit.
// `vLight` = niveau de lumière du lieu (0..1, ciel ouvert = 1, grotte = bas).
// `vWaterY` = hauteur de la surface de l'eau au-dessus du mob (-1000 : hors de l'eau). Vus depuis
// l'air, les fragments immergés sont absorbés / teintés comme le fond réfracté de fluid.frag
// (Beer-Lambert par canal, diffusion bleu-vert) et reflètent le ciel (Fresnel) : un orque ou une
// méduse ne se détache plus « par-dessus » l'eau. Sous la surface, le composite applique déjà le
// brouillard sous-marin à toute l'image.
// Location 1 (bright buffer) = (0, 0, 0, alpha) : le bloom du terrain derrière un mob est masqué
// (lave, feuillage lumineux) au lieu de se superposer au mob.
#include "frame.glsl"
layout (location = 0) in vec2 vUV;
layout (location = 1) in vec4 vTint;
layout (location = 2) in float vBrightness;
layout (location = 3) in vec4 vLightSpace;
layout (location = 4) flat in float vLight;
layout (location = 5) in vec3 vWorld;
layout (location = 6) flat in float vWaterY;
layout (set = 0, binding = 2) uniform sampler2D shadowMap;
layout (set = 1, binding = 0) uniform sampler2D uTexture;
layout (location = 0) out vec4 FragColor;
layout (location = 1) out vec4 BrightColor;
#include "shadow.glsl"

const vec3 WATER_SIGMA = vec3(0.50, 0.17, 0.09);   // identique à fluid.frag
const vec3 WATER_DEEP  = vec3(0.012, 0.075, 0.115);
const vec3 WATER_TINT  = vec3(0.55, 0.80, 1.00);   // teinte moyenne de l'eau (biome inconnu ici)

void main() {
    vec4 texColor = texture(uTexture, vUV) * vTint;
    if (texColor.a < 0.01) discard;
    vec3 albedo = pow(texColor.rgb, vec3(2.2));
    vec3 viewDir = normalize(u.cameraPos.xyz - vWorld);
    vec3 normal = normalize(cross(dFdx(vWorld), dFdy(vWorld)));
    if (dot(normal, viewDir) < 0.0) normal = -normal;
    float shadow = softShadow(vWorld, normal, 0.0006);
    float light = max(vLight, 0.08);
    // Lumière du lieu appliquée à l'ambiant et au soleil, comme la lumière des sommets du terrain.
    vec3 lightScale = u.ambientColor.xyz + u.lightColor.xyz * (1.0 - shadow);
    vec3 lit = albedo * vBrightness * light * lightScale;

    bool underwater = vWaterY > -500.0 && vWorld.y < vWaterY && u.cameraPos.y > vWaterY
                      && u.flags.x == 0;
    if (underwater) {
        float vertical = vWaterY - vWorld.y;
        float path = vertical / max(abs(viewDir.y), 0.3);
        vec3 sigma = WATER_SIGMA * (1.35 - 0.7 * WATER_TINT);
        vec3 trans = exp(-min(path, 60.0) * sigma);
        vec3 deepCol = WATER_DEEP * (0.5 + WATER_TINT) * (u.ambientColor.xyz + u.lightColor.xyz) * 0.6;
        vec3 under = lit * trans + deepCol * (1.0 - trans);
        // Reflet du ciel sur la surface, pondéré par Fresnel (Schlick).
        float cosTheta = clamp(abs(viewDir.y), 0.0, 1.0);
        float fresnel = 0.04 + 0.96 * pow(1.0 - cosTheta, 5.0);
        vec3 refl = mix(u.horizonColor.xyz, u.zenithColor.xyz, 0.6) * 0.6;
        lit = mix(under, refl, fresnel);
    }
    FragColor = vec4(lit, texColor.a);
    BrightColor = vec4(0.0, 0.0, 0.0, texColor.a);
}
