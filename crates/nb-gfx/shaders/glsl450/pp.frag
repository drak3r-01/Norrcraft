#version 450
#extension GL_GOOGLE_include_directive : require
// Post-traitement composite (dernier passage plein écran) : distorsions (chaleur / eau), bloom,
// brouillard, ciel (dégradé, soleil, lune, étoiles, aurores boréales), nuages, filtres de milieu
// (air / eau / lave) et tonemapping.
//
// Dérivé du port Vulkan de shaders/gl330/pp.frag, remanié :
//  - tout le calcul se fait en uv GL : uv = (x, 1 - y) ; les textures (scene, bloomBlur, depthMap)
//    sont lues via vkUv() (images Vulkan, v = 0 en haut) ;
//  - la reconstruction de la position monde utilise inverseProjGl = P_gl⁻¹ ;
//  - nuages : échantillonnage à (gridPos + 0.25) / 256 (robustesse NEAREST) ;
//  - aurores : nouveau modèle de rideaux (voir getAurora), nettement moins coûteux que l'ancien
//    ray-marching fbm (≈ 20 pas sans aucune lecture de texture) ;
//  - ajouts : disque de soleil / de lune, étoiles, tramage final contre le banding.
//
// Le dégradé de ciel, le halo solaire, le brouillard, le filtre chaud et ACES sont inchangés
// (ils sont vérifiés pixel à pixel par tests/render.rs).
#include "frame.glsl"

layout (location = 0) out vec4 FragColor;
layout (location = 0) in vec2 TexCoords;
layout (set = 0, binding = 1) uniform sampler2D scene;
layout (set = 0, binding = 2) uniform sampler2D bloomBlur;
layout (set = 0, binding = 3) uniform sampler2D depthMap;
layout (set = 0, binding = 4) uniform sampler2D cloudTexture;
layout (set = 0, binding = 5) uniform sampler2D noiseTexture;

// ----------------------------------------------------------------------------------------------
// Constantes
// ----------------------------------------------------------------------------------------------
const int   FLUID_AIR   = 0;
const int   FLUID_WATER = 1;
const int   FLUID_LAVA  = 2;

const float CLOUD_MIN_Y = 130.0;
const float CLOUD_MAX_Y = 134.0;
const float CLOUD_RANGE = 500.0;
const float CLOUD_CELL  = 12.0;     // taille d'une case de clouds.png en blocs
const int   CLOUD_STEPS = 15;

const float AURORA_H0    = 260.0;   // base des rideaux, au-dessus de la caméra (blocs)
const float AURORA_H1    = 620.0;   // sommet des rideaux
const float AURORA_SCALE = 0.0013;  // blocs -> unités de forme (1 unité ≈ 770 blocs)
const float AURORA_SHEET_GAIN = 0.55;  // gain d'un croisement de feuille
const float AURORA_HAZE_GAIN  = 0.15;  // gain du halo diffus
const float AURORA_PEAK  = 1.25;    // luminance HDR maximale d'un rideau

const float SUN_RADIUS_COS_OUT  = 0.99875;   // rayon apparent ≈ 0.05 rad
const float SUN_RADIUS_COS_IN   = 0.99910;
const float MOON_RADIUS_COS_OUT = 0.99935;   // rayon apparent ≈ 0.036 rad
const float MOON_RADIUS_COS_IN  = 0.99955;
const float STAR_GRID           = 110.0;     // cases de ~0,5° sur la sphère céleste

const vec3  WARM_FILTER = vec3(1.15, 1.05, 0.95);

// ----------------------------------------------------------------------------------------------
// Utilitaires
// ----------------------------------------------------------------------------------------------
// Bruit de valeur lu dans la texture 256² (R8, répétée).
float noise(vec2 p) { return texture(noiseTexture, p / 256.0).r; }

float fbm(vec2 p) {
    float f = 0.0, a = 0.5;
    for (int i = 0; i < 4; i++) {
        f += a * noise(p);
        p *= 2.0;
        a *= 0.5;
    }
    return f;
}

// Hash 3D -> 3D dans [0,1) (Dave Hoskins), sans sin() : stable à toutes les distances.
vec3 hash33(vec3 p) {
    p = fract(p * vec3(0.1031, 0.1030, 0.0973));
    p += dot(p, p.yxz + 33.33);
    return fract((p.xxy + p.yxx) * p.zyx);
}

// Bruit de gradient entrelacé (Jimenez) : dither par pixel sans motif visible.
float interleavedGradientNoise(vec2 pixel) {
    return fract(52.9829189 * fract(dot(pixel, vec2(0.06711056, 0.00583715))));
}

// Coordonnées uv GL (y vers le haut) -> uv d'image Vulkan.
vec2 vkUv(vec2 uvGL) { return vec2(uvGL.x, 1.0 - uvGL.y); }

// ----------------------------------------------------------------------------------------------
// Aurores boréales
//
// Deux rideaux verticaux, très hauts et très loin (AURORA_H0..H1 au-dessus de la caméra). Chacun
// est une feuille infiniment mince dont la trace horizontale serpente (pli) : le long du rayon de
// vue on cherche les changements de signe de l'écart à la feuille (≈ 24 pas, croisement affiné
// par sécante), ce qui donne un bord inférieur net et des « rayons » verticaux nets quelle que
// soit l'orientation de la vue. À chaque croisement : dégradé selon la hauteur (liseré rose à la
// base, vert, haut violet), rayons qui dansent, éclaircissement des feuilles vues de biais. Un
// léger halo volumétrique (gaussienne autour de la feuille) lie le tout. Émission seule, saturée
// en douceur par 1 - exp(-x). Aucune lecture de texture ; pas de tramage nécessaire.
// ----------------------------------------------------------------------------------------------
const vec2 AURORA_AXIS0 = vec2( 0.6216, 0.7833);   // (cos 0.9, sin 0.9)
const vec2 AURORA_AXIS1 = vec2(-0.9710, 0.2392);   // (cos 2.9, sin 2.9)

// Écart signé de q à la feuille k et coordonnée `along` le long de celle-ci.
float auroraSheetDist(vec2 q, vec2 axis, float k, float time, float phase, out float along) {
    along = dot(q, vec2(-axis.y, axis.x));
    // Pli lent (phase = dérive sur la journée) + ondulations plus rapides (time).
    float fold = 0.55 * sin(along * 1.3 + phase * 1.7 + k * 3.1)
               + 0.30 * sin(along * 3.4 - phase * 2.3 + time * 0.04 + k)
               + 0.10 * sin(along * 8.1 + time * 0.10);
    return dot(q, axis) - fold - (k - 0.5) * 2.4;
}

// Émission de la feuille k au point `along` : rayons verticaux x luminosité lente.
// rayAmount ∈ [0,1] : contraste des rayons (atténué en vue rasante où ils seraient sous-échantillonnés).
float auroraSheetEmission(float along, float k, float time, float phase, float rayAmount) {
    float r = 0.55 * sin(along * 47.0 + 2.5 * sin(along * 5.1 + time * 0.20) + time * 0.50 + k)
            + 0.45 * sin(along * 113.0 - time * 0.70 + 1.7 * sin(along * 11.0 + k));
    float rays = mix(0.6, smoothstep(-0.6, 0.9, r) * 1.5, rayAmount);
    // Certains tronçons s'éteignent, d'autres s'embrasent.
    float lum = smoothstep(-0.35, 0.65, sin(along * 0.9 + phase * 1.2 + k * 2.0 + 0.8 * sin(time * 0.07 + k)));
    return rays * lum;
}

// Dégradé vertical typique : liseré rose/magenta à la base, vert dominant, haut violet.
vec3 auroraGradient(float hN) {
    const vec3 PINK   = vec3(0.95, 0.20, 0.45);
    const vec3 GREEN  = vec3(0.12, 1.00, 0.38);
    const vec3 VIOLET = vec3(0.38, 0.22, 0.95);
    vec3 c = mix(GREEN, VIOLET, smoothstep(0.30, 0.95, hN));
    return mix(c, PINK, exp(-hN * 28.0) * 0.75);
}

vec3 getAurora(vec3 rd, vec3 camPos, float time, float phase, float dither) {
    float horizonFade = smoothstep(0.0, 0.12, rd.y);
    if (horizonFade <= 0.0) return vec3(0.0);

    float inv = 1.0 / rd.y;
    float t0 = AURORA_H0 * inv;
    float t1 = AURORA_H1 * inv;
    int steps = int(mix(32.0, 20.0, smoothstep(0.10, 0.60, rd.y)));
    float dt = (t1 - t0) / float(steps);
    vec2 hDir = rd.xz / max(length(rd.xz), 1e-4);            // direction horizontale du rayon
    vec2 base = camPos.xz * AURORA_SCALE;
    vec2 dq = rd.xz * AURORA_SCALE;                          // dq/dt
    float rayAmount = smoothstep(0.06, 0.35, rd.y);
    const vec3 COLD = vec3(0.55, 0.95, 1.15);                // le second rideau est plus froid

    // Pas jitterés (le premier intervalle est raccourci) : le halo est échantillonné sans
    // banding, et chaque intervalle garde son croisement exact depuis la base t0.
    vec3 sheets = vec3(0.0), haze = vec3(0.0);
    float along0, along1, d0, d1;
    float t = t0;
    d0 = auroraSheetDist(base + dq * t, AURORA_AXIS0, 0.0, time, phase, along0);
    d1 = auroraSheetDist(base + dq * t, AURORA_AXIS1, 1.0, time, phase, along1);
    for (int i = 0; i <= steps; i++) {
        float tPrev = t;
        float p0 = d0, p1 = d1;
        t = i < steps ? t0 + (float(i) + dither) * dt : t1;
        float len = t - tPrev;
        vec2 q = base + dq * t;
        d0 = auroraSheetDist(q, AURORA_AXIS0, 0.0, time, phase, along0);
        d1 = auroraSheetDist(q, AURORA_AXIS1, 1.0, time, phase, along1);

        // Halo diffus autour des feuilles (bord net d'un côté, traîne douce de l'autre).
        float hN = (rd.y * t - AURORA_H0) / (AURORA_H1 - AURORA_H0);
        float glow = exp(-d0 * d0 * (d0 > 0.0 ? 14.0 : 5.0)) + 0.8 * exp(-d1 * d1 * (d1 > 0.0 ? 14.0 : 5.0));
        haze += auroraGradient(hN) * glow * exp(-hN * 3.2) * (len / dt);

        // Croisement d'une feuille dans l'intervalle [tPrev, t] ?
        for (int k = 0; k < 2; k++) {
            float prev = k == 0 ? p0 : p1;
            float cur  = k == 0 ? d0 : d1;
            if (prev * cur >= 0.0) continue;
            vec2 axis = k == 0 ? AURORA_AXIS0 : AURORA_AXIS1;
            float kf = float(k);
            // Sécante, puis un second affinage sur le sous-intervalle qui contient la racine.
            float tc = tPrev + len * prev / (prev - cur);
            float along;
            float dc = auroraSheetDist(base + dq * tc, axis, kf, time, phase, along);
            if (dc * prev < 0.0) tc = tPrev + (tc - tPrev) * prev / (prev - dc);
            else                 tc = tc + (t - tc) * dc / (dc - cur);
            auroraSheetDist(base + dq * tc, axis, kf, time, phase, along);
            float h = (rd.y * tc - AURORA_H0) / (AURORA_H1 - AURORA_H0);
            // Vue de biais : la feuille est traversée sur plus d'épaisseur, donc plus claire.
            float grazing = 1.0 / max(abs(dot(hDir, axis)), 0.3);
            float emission = auroraSheetEmission(along, kf, time, phase, rayAmount)
                           * exp(-h * 3.2) * grazing;
            sheets += auroraGradient(h) * emission * (k == 0 ? vec3(1.0) : COLD);
        }
    }
    // Les trajets obliques traversent plus de couche : éclaircissement de bord, modéré.
    float limb = pow(inv, 0.55);
    float pulse = 0.8 + 0.2 * sin(time * 0.35);
    vec3 x = (sheets * AURORA_SHEET_GAIN + haze * (limb * AURORA_HAZE_GAIN / float(steps))) * pulse;
    return (1.0 - exp(-x)) * AURORA_PEAK * horizonFade;
}

// ----------------------------------------------------------------------------------------------
// Ciel
// ----------------------------------------------------------------------------------------------
// Étoiles : une étoile au plus par case 3D de la sphère céleste, avec scintillement.
vec3 getStars(vec3 rd, float time) {
    vec3 p = rd * STAR_GRID;
    vec3 cell = floor(p);
    vec3 h = hash33(cell);
    if (h.x < 0.965) return vec3(0.0);                       // ~3,5 % des cases
    vec3 pos = (hash33(cell + vec3(19.1, 7.7, 3.3)) - 0.5) * 0.6;
    float d = length(fract(p) - 0.5 - pos);
    float size = 0.10 + 0.12 * h.y;
    float star = smoothstep(size, 0.0, d);
    float twinkle = 0.75 + 0.25 * sin(time * (1.5 + 3.0 * h.z) + h.y * 40.0);
    vec3 tint = mix(vec3(0.70, 0.80, 1.00), vec3(1.00, 0.85, 0.70), h.z);
    return tint * star * twinkle * (0.5 + 1.6 * h.y * h.y);
}

// Disque du soleil (jour) ou de la lune (nuit) dans la direction de la lumière principale.
vec3 getCelestialDisc(vec3 rd, vec3 bodyDir, bool moon, vec3 sunColor) {
    float c = dot(rd, bodyDir);
    float fade = smoothstep(-0.04, 0.05, rd.y);              // se couche derrière l'horizon
    if (moon) {
        float disc = smoothstep(MOON_RADIUS_COS_OUT, MOON_RADIUS_COS_IN, c);
        return vec3(0.78, 0.84, 1.00) * 1.7 * disc * fade;
    }
    float disc = smoothstep(SUN_RADIUS_COS_OUT, SUN_RADIUS_COS_IN, c);
    return sunColor * 5.0 * disc * fade;
}

// ----------------------------------------------------------------------------------------------
// Nuages : 15 pas dans la couche [CLOUD_MIN_Y, CLOUD_MAX_Y], cases lues dans clouds.png qui défile
// avec le vent. Renvoie (couleur moyenne, opacité) ; opacité 0 = pas de nuage.
// ----------------------------------------------------------------------------------------------
vec4 getClouds(vec3 camPos, vec3 rd, float time, float depth, float dist,
               vec3 horizon, vec3 zenith, vec3 sunColor, float sunDot) {
    float t1 = (CLOUD_MIN_Y - camPos.y) / rd.y;
    float t2 = (CLOUD_MAX_Y - camPos.y) / rd.y;
    float tStart = max(t1, 0.0);
    float tEnd = max(t2, 0.0);
    // Derrière le terrain : les nuages ne comptent que s'ils sont plus proches que lui.
    if (tEnd <= 0.0 || tStart >= CLOUD_RANGE || !(depth >= 1.0 || dist > tStart)) return vec4(0.0);
    tEnd = min(tEnd, CLOUD_RANGE);
    if (depth < 1.0) tEnd = min(tEnd, dist);

    float stepSize = (tEnd - tStart) / float(CLOUD_STEPS);
    float phase = pow(sunDot, 4.0);                           // diffusion vers l'avant
    vec3 acc = vec3(0.0);
    float alpha = 0.0;
    float t = tStart;
    for (int i = 0; i < CLOUD_STEPS; i++) {
        vec3 p = camPos + rd * t;
        vec2 cell = floor((p.xz + vec2(time * 3.0, 0.0)) / CLOUD_CELL);
        vec4 tex = texture(cloudTexture, (cell + 0.25) / 256.0);
        float density = tex.r * tex.a;
        if (density > 0.1) {
            float h = clamp((p.y - CLOUD_MIN_Y) / (CLOUD_MAX_Y - CLOUD_MIN_Y), 0.0, 1.0);
            vec3 ambient = mix(horizon, zenith, h) * 0.8;
            vec3 col = mix(ambient, sunColor, phase * 0.6);
            if (p.y < CLOUD_MIN_Y + 0.5) col *= 0.6;          // dessous plus sombre
            float a = density * 0.4;
            acc += col * a * (1.0 - alpha);
            alpha += a * (1.0 - alpha);
            if (alpha > 0.95) break;
        }
        t += stepSize;
    }
    if (alpha <= 0.0) return vec4(0.0);
    return vec4(acc / alpha, alpha);
}

// ----------------------------------------------------------------------------------------------
vec3 ACESFilm(vec3 x) {
    const float a = 2.51, b = 0.03, c = 2.43, d = 0.59, e = 0.14;
    return clamp((x * (a * x + b)) / (x * (c * x + d) + e), 0.0, 1.0);
}

void main() {
    vec3 camPos = u.cameraPos.xyz;
    float time = u.cameraPos.w;
    vec3 uFogColor = u.fogColor.xyz;
    vec3 uLightDir = u.lightDir.xyz;
    vec3 uSunColor = u.sunColor.xyz;
    int fluid = u.flags.x;
    bool moonUp = u.flags.z != 0;
    float auroraIntensity = u.env.x;
    float auroraPhase = u.env.y;
    float heatIntensity = u.env.z;
    float night = u.env.w;

    // ------------------------------------------------------------------ distorsion de l'image
    vec2 uv = vec2(TexCoords.x, 1.0 - TexCoords.y);
    float nHeat = 0.0;
    bool heated = fluid == FLUID_AIR && heatIntensity > 0.0;
    if (heated) {
        nHeat = fbm(uv * 15.0 + vec2(0.0, -time * 3.0)) * 2.0 - 1.0;
        uv += nHeat * 0.02 * heatIntensity;
    } else if (fluid == FLUID_WATER) {
        uv.x += sin(uv.y * 15.0 + time * 2.0) * 0.003;
        uv.y += cos(uv.x * 15.0 + time * 2.0) * 0.003;
    } else if (fluid == FLUID_LAVA) {
        uv.x += sin(uv.y * 3.0 + time * 0.4) * 0.006;
        uv.y += cos(uv.x * 3.0 + time * 0.4) * 0.006;
    }

    // ------------------------------------------------------------------ scène HDR + bloom
    vec3 hdrColor = texture(scene, vkUv(uv)).rgb + texture(bloomBlur, vkUv(uv)).rgb;
    if (heated) {
        vec3 lavaGlow = mix(vec3(1.2, 0.1, 0.0), vec3(1.0, 0.5, 0.0), nHeat * 0.5 + 0.5);
        hdrColor += lavaGlow * heatIntensity * 0.35;
    }

    // ------------------------------------------------------------------ rayon de vue + distance
    float depth = texture(depthMap, vkUv(uv)).r;
    vec4 ndc = vec4(uv * 2.0 - 1.0, depth * 2.0 - 1.0, 1.0);
    vec4 viewPos = u.inverseProjGl * ndc;
    viewPos /= viewPos.w;
    vec3 worldPos = (u.inverseView * viewPos).xyz;
    vec3 viewDir = normalize(worldPos - camPos);
    float dist = depth >= 1.0 ? 10000.0 : length(worldPos - camPos);

    // ------------------------------------------------------------------ brouillard
    float fogDensity = 0.002;
    float fogFalloff = 1.8;
    if (fluid == FLUID_WATER) {
        fogDensity = 0.015;
        fogFalloff = 1.2;
    } else if (fluid == FLUID_LAVA) {
        fogDensity = 0.3;
        fogFalloff = 1.0;
    }
    float fogFactor = clamp(exp(-pow(dist * fogDensity, fogFalloff)), 0.0, 1.0);

    // ------------------------------------------------------------------ ciel (couleur du fond)
    vec3 zenithColor = u.zenithColor.xyz;
    vec3 horizonColor = u.horizonColor.xyz;
    vec3 sunDir = normalize(-uLightDir);          // soleil le jour, lune la nuit
    float sunDot = max(dot(viewDir, sunDir), 0.0);

    float skyGradient = clamp(pow(max(0.0, viewDir.y + 0.05), 0.6), 0.0, 1.0);
    vec3 skyColor = mix(horizonColor, zenithColor, skyGradient);
    // Halo du soleil (plein) ou de la lune (atténué : la nuit il écrasait les étoiles).
    vec3 sunHalo = fluid == FLUID_AIR ? pow(sunDot, 64.0) * uSunColor * mix(1.0, 0.3, night) : vec3(0.0);

    // Éléments du ciel : seulement là où le fond est visible (ni sous l'eau, ni derrière du
    // terrain proche entièrement noyé dans le brouillard).
    bool skyVisible = fluid == FLUID_AIR && viewDir.y > 0.0 && (1.0 - fogFactor) > 0.02;
    if (skyVisible) {
        skyColor += getCelestialDisc(viewDir, sunDir, moonUp, uSunColor);
        if (night > 0.0) {
            float starFade = smoothstep(0.0, 0.25, viewDir.y);
            skyColor += getStars(viewDir, time) * night * starFade;
        }
        if (auroraIntensity > 0.0) {
            // Divisé par le filtre chaud : les aurores gardent leurs couleurs.
            skyColor += getAurora(viewDir, camPos, time, auroraPhase, interleavedGradientNoise(gl_FragCoord.xy))
                      * auroraIntensity / WARM_FILTER;
        }
    }
    vec3 skyFogColor = skyColor + sunHalo;

    // Couleur vers laquelle le brouillard tend, selon le milieu.
    vec3 finalFogColor = skyFogColor;
    if (fluid == FLUID_WATER) {
        vec3 waterColor = pow(uFogColor, vec3(2.2));
        float depthFactor = clamp(exp(-(64.0 - camPos.y) * 0.06), 0.02, 1.0);
        finalFogColor = mix(vec3(0.02, 0.06, 0.15), waterColor, depthFactor);
        finalFogColor += uSunColor * pow(max(0.0, viewDir.y), 8.0) * 0.15 * depthFactor;
    } else if (fluid == FLUID_LAVA) {
        finalFogColor = pow(uFogColor, vec3(2.2));
    }
    hdrColor = mix(finalFogColor, hdrColor, fogFactor);

    // ------------------------------------------------------------------ nuages
    if (fluid == FLUID_AIR && viewDir.y > 0.0) {
        vec4 cloud = getClouds(camPos, viewDir, time, depth, dist,
                               horizonColor, zenithColor, uSunColor, sunDot);
        if (cloud.a > 0.0) {
            // Brouillard évalué à l'entrée du nuage.
            float tStart = max((CLOUD_MIN_Y - camPos.y) / viewDir.y, 0.0);
            float cloudFog = clamp(exp(-pow(tStart * fogDensity, fogFalloff)), 0.0, 1.0);
            hdrColor = mix(hdrColor, mix(finalFogColor, cloud.rgb, cloudFog), cloud.a);
        }
    }

    // ------------------------------------------------------------------ filtre du milieu
    if (fluid == FLUID_AIR) {
        hdrColor *= WARM_FILTER;
    } else if (fluid == FLUID_WATER) {
        float attenuation = clamp(exp(-(64.0 - camPos.y) * 0.04), 0.15, 1.0);
        hdrColor *= mix(vec3(0.4, 0.7, 0.9), vec3(1.0), 0.3) * attenuation;
        float vignette = 1.0 - smoothstep(0.4, 1.5, length(uv - 0.5));
        hdrColor *= mix(0.6, 1.0, vignette);
    } else {
        hdrColor *= vec3(1.5, 0.5, 0.1);
    }

    // ------------------------------------------------------------------ tonemapping + sortie
    vec3 mapped = pow(ACESFilm(hdrColor), vec3(1.0 / 2.2));
    // Tramage triangulaire d'amplitude ±1/255 : supprime le banding des dégradés sombres (ciel
    // nocturne, aurores) sans changer la couleur moyenne.
    float n1 = interleavedGradientNoise(gl_FragCoord.xy);
    float n2 = interleavedGradientNoise(gl_FragCoord.yx + 17.0);
    mapped += (n1 + n2 - 1.0) * (0.5 / 255.0);
    FragColor = vec4(mapped, 1.0);
}
