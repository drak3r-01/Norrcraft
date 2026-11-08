#version 450
#extension GL_GOOGLE_include_directive : require
// Particules instanciées : un draw pour toute la scène. Chaque instance est un `GpuParticle`
// (nb-particles, 32 octets) ; le quad (2 triangles, 6 sommets) est construit ici, orienté face
// caméra (billboard sphérique) ou autour de l'axe vertical (cylindrique : pluie, neige).
#include "frame.glsl"
layout (location = 0) in vec4 aPosSize;   // xyz = position monde, w = taille
layout (location = 1) in vec4 aColor;     // RGBA8 unorm (fondu et lumière déjà appliqués)
layout (location = 2) in float aRot;      // 0..1 → 0..2π
layout (location = 3) in uint aLayer;
layout (location = 4) in uvec4 aUv;       // (u0, v0, w, h) en 1/64 ; w = 0 → tuile entière
layout (location = 5) in uint aFlags;     // 1 = atlas des blocs, 2 = émissive, 4 = vertical, 8..15 = étirement ×8
layout (location = 0) out vec2 TexCoords;
layout (location = 1) out vec4 vColor;
layout (location = 2) flat out uint vLayer;
layout (location = 3) flat out uint vFlags;
const vec2 CORNERS[6] = vec2[6](
    vec2(-0.5, -0.5), vec2(0.5, -0.5), vec2(0.5, 0.5),
    vec2(-0.5, -0.5), vec2(0.5, 0.5), vec2(-0.5, 0.5));
void main() {
    vec2 c = CORNERS[gl_VertexIndex];
    // v inversé : l'image apparaît tête en bas (comportement Java d'origine, à reproduire).
    vec2 uv = vec2(c.x + 0.5, 0.5 - c.y);
    if (aUv.z != 0u) {
        uv = vec2(aUv.xy) / 64.0 + uv * (vec2(aUv.zw) / 64.0);
    }
    TexCoords = uv;
    vColor = aColor;
    vLayer = aLayer;
    vFlags = aFlags;
    vec3 right;
    vec3 up;
    float stretch = 1.0;
    uint s = (aFlags >> 8u) & 255u;
    if (s != 0u) stretch = float(s) / 8.0;
    if ((aFlags & 4u) != 0u) {
        up = vec3(0.0, 1.0, 0.0);
        vec3 toCam = u.cameraPos.xyz - aPosSize.xyz;
        right = normalize(cross(up, toCam));
    } else {
        right = vec3(u.view[0][0], u.view[1][0], u.view[2][0]);
        up = vec3(u.view[0][1], u.view[1][1], u.view[2][1]);
        float a = aRot * 6.2831853;
        float ca = cos(a);
        float sa = sin(a);
        c = vec2(c.x * ca - c.y * sa, c.x * sa + c.y * ca);
    }
    c.y *= stretch;
    vec3 world = aPosSize.xyz + (right * c.x + up * c.y) * aPosSize.w;
    gl_Position = u.projVk * u.view * vec4(world, 1.0);
}
