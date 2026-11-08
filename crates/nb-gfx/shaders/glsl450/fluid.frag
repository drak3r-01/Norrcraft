#version 450
#extension GL_GOOGLE_include_directive : require
// Fluides : eau et lave (passe B1, mélangée, profondeur en lecture seule).
//
// Remplace la branche « fluides » de l'ancien main.frag (spec §18, passe uRenderPass = 1) par un
// rendu dans l'esprit des shader packs de Minecraft :
//
// EAU
//  - normales de vagues analytiques (somme de six sinus directionnels, gradient exact, octaves
//    hautes atténuées avec la distance pour éviter le scintillement) ;
//  - réfraction : la couleur du terrain opaque (copie `refractTex`) est lue avec un décalage
//    proportionnel à la pente des vagues et à l'épaisseur d'eau, sans jamais lire un pixel qui
//    serait devant la surface (test de profondeur) ;
//  - absorption de Beer-Lambert par canal (le rouge disparaît en premier, puis le vert) et couleur
//    de diffusion en profondeur, teintées par la couleur de biome de l'eau ;
//  - réflexion du ciel (dégradé du composite) pondérée par le Fresnel de Schlick, avec reflet et
//    scintillement du soleil / de la lune ;
//  - liseré d'écume là où l'eau est peu profonde.
// LAVE
//  - surface émissive animée (texture déformée par deux champs d'écoulement), croûte sombre peu
//    lumineuse, veines très chaudes qui nourrissent un bloom discret ;
//  - identifiée par sa couche d'atlas (u.flags.w) et non plus par la couleur du texel : les
//    texels sombres de la croûte étaient traités comme de l'eau.
//
// Les fluides sont dessinés de l'arrière vers l'avant, profondeur en lecture seule.
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
layout (set = 1, binding = 0) uniform sampler2D opaqueDepth;
layout (set = 1, binding = 1) uniform sampler2D refractTex;

const float WATER_R0 = 0.04;                       // réflectance de Fresnel à incidence normale
const vec3  WATER_SIGMA = vec3(0.50, 0.17, 0.09);  // absorption par bloc (R, V, B)
const vec3  WATER_DEEP  = vec3(0.012, 0.075, 0.115);

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

float hash21(vec2 p) {
    p = fract(p * vec2(123.34, 456.21));
    p += dot(p, p + 45.32);
    return fract(p.x * p.y);
}

float vnoise(vec2 p) {
    vec2 i = floor(p), f = fract(p);
    f = f * f * (3.0 - 2.0 * f);
    return mix(mix(hash21(i), hash21(i + vec2(1.0, 0.0)), f.x),
               mix(hash21(i + vec2(0.0, 1.0)), hash21(i + vec2(1.0, 1.0)), f.x), f.y);
}

// Couleur du ciel dans la direction r (même dégradé que le composite).
vec3 skyColor(vec3 r) {
    float g = clamp(pow(max(0.0, r.y + 0.05), 0.6), 0.0, 1.0);
    return mix(u.horizonColor.xyz, u.zenithColor.xyz, g);
}

// Houle : gradient (d h / d x, d h / d z) d'une somme de sinus directionnels. `dist` atténue les
// octaves fines (anti-scintillement).
vec2 waveGradient(vec2 p, float t, float dist) {
    const vec2  DIR[6] = vec2[6](vec2(0.87, 0.50), vec2(-0.35, 0.94), vec2(0.62, -0.78),
                                  vec2(-0.95, -0.31), vec2(0.28, 0.96), vec2(-0.71, 0.70));
    const vec3  PRM[6] = vec3[6](vec3(0.55, 0.9, 0.120), vec3(0.93, 1.2, 0.070), vec3(1.70, 1.6, 0.045),
                                  vec3(2.90, 2.2, 0.028), vec3(5.30, 3.1, 0.014), vec3(9.10, 4.3, 0.007));
    float lod[6] = float[6](1.0, 1.0, 1.0 - smoothstep(90.0, 200.0, dist),
                            1.0 - smoothstep(55.0, 120.0, dist),
                            1.0 - smoothstep(25.0, 65.0, dist),
                            1.0 - smoothstep(12.0, 35.0, dist));
    vec2 g = vec2(0.0);
    for (int i = 0; i < 6; i++) {
        float k = PRM[i].x;
        float ph = dot(DIR[i], p) * k + t * PRM[i].y;
        g += DIR[i] * (PRM[i].z * k * lod[i] * cos(ph));
    }
    return g;
}

// ----------------------------------------------------------------------------------------------
vec4 water(vec3 geoNormal, vec3 viewDir, float shadow, vec3 lightScale, vec3 linearTex, out vec3 bloom) {
    float t = u.cameraPos.w;
    float dist = length(u.cameraPos.xyz - FragPos);
    bool top = geoNormal.y > 0.7;

    // Normale : houle sur les faces du dessus, normale géométrique sur les côtés.
    vec3 N = geoNormal;
    if (top) {
        float strength = mix(1.6, 0.5, smoothstep(40.0, 260.0, dist));
        vec2 g = waveGradient(FragPos.xz, t, dist) * strength;
        N = normalize(vec3(-g.x, 1.0, -g.y));
    }
    float cosTheta = max(dot(viewDir, N), 0.0);

    // Teinte du biome (la couleur des sommets porte aussi la lumière) et lumière du ciel reçue.
    float tintMax = max(max(Color.r, Color.g), max(Color.b, 0.001));
    vec3 tintN = clamp(Color / tintMax, 0.0, 1.0);
    float skyVis = clamp(dot(Color, vec3(1.0 / 3.0)) * 2.0, 0.0, 1.0);

    // Caméra sous la surface : eau sombre et peu transparente (le composite ajoute le brouillard).
    if (u.cameraPos.y < FragPos.y - 0.02 && top) {
        bloom = vec3(0.0);
        return vec4(WATER_DEEP * (0.5 + tintN) * lightScale * 2.5, 0.6);
    }

    // -------------------------------------------------------------- réfraction
    vec2 screen = u.projGl.zw;
    vec2 uv = gl_FragCoord.xy / screen;
    float surfZ = getLinearDepth(gl_FragCoord.z);
    float oD = texelFetch(opaqueDepth, ivec2(gl_FragCoord.xy), 0).r;
    float thick0 = max(getLinearDepth(oD) - surfZ, 0.0);

    vec3 nView = mat3(u.view) * N;
    vec3 nView0 = mat3(u.view) * vec3(0.0, 1.0, 0.0);
    vec2 off = (nView.xy - nView0.xy) * vec2(1.0, -1.0);
    off *= 0.045 * min(thick0, 3.0) / (1.0 + surfZ * 0.05);
    vec2 uvR = uv + off;
    float thick = thick0;
    {
        ivec2 pix = ivec2(clamp(uvR, vec2(0.0), vec2(0.9999)) * screen);
        float d2 = getLinearDepth(texelFetch(opaqueDepth, pix, 0).r);
        // Pixel plus proche que la surface (herbe, bord de la berge...) : pas de décalage.
        if (d2 < surfZ - 0.02) {
            uvR = uv;
        } else {
            thick = max(d2 - surfZ, 0.0);
        }
    }
    vec3 background = texture(refractTex, uvR).rgb;

    // -------------------------------------------------------------- absorption + diffusion
    vec3 sigma = WATER_SIGMA * (1.35 - 0.7 * tintN);
    vec3 trans = exp(-min(thick, 60.0) * sigma);
    vec3 deepCol = WATER_DEEP * (0.5 + tintN) * lightScale * (0.35 + 0.65 * skyVis);
    // Léger motif de la texture (blocs d'eau de Minecraft) dans la couleur de diffusion.
    deepCol *= 0.9 + 0.35 * dot(linearTex, vec3(1.0 / 3.0));
    vec3 under = background * trans + deepCol * (1.0 - trans);

    // -------------------------------------------------------------- réflexion + Fresnel
    vec3 R = reflect(-viewDir, N);
    R.y = max(R.y, 0.01);
    R = normalize(R);
    vec3 refl = skyColor(R) * mix(0.25, 1.0, skyVis);
    float fresnel = WATER_R0 + (1.0 - WATER_R0) * pow(1.0 - cosTheta, 5.0);
    // Reflet du soleil / de la lune : lobe étroit (scintillement des vagues) + halo large.
    vec3 sunDir = normalize(-u.lightDir.xyz);
    float sd = max(dot(R, sunDir), 0.0);
    vec3 sunRefl = u.sunColor.xyz * (pow(sd, 420.0) * 9.0 + pow(sd, 28.0) * 0.30);
    sunRefl *= (1.0 - shadow) * skyVis;
    // Les faces latérales (cascades, bords) reflètent peu.
    float reflAmount = top ? 1.0 : 0.35;

    vec3 color = mix(under, refl, fresnel * reflAmount) + sunRefl * fresnel * reflAmount * 4.0;

    // -------------------------------------------------------------- écume de berge
    float alpha = smoothstep(0.0, 0.10, thick);
    if (top) {
        float band = 1.0 - smoothstep(0.0, 0.55, thick);
        float pattern = 0.5 + 0.5 * sin(FragPos.x * 5.7 + FragPos.z * 4.3 + t * 1.3
                                       + 9.0 * sin(FragPos.z * 1.9 - t * 0.7));
        float foam = band * smoothstep(0.40, 0.85, pattern) * 0.55;
        color = mix(color, vec3(0.85, 0.9, 0.95) * lightScale, foam);
        alpha = max(alpha, foam);
    }
    bloom = min(sunRefl * fresnel * reflAmount, vec3(1.2)) * 0.6;
    return vec4(color, alpha);
}

// ----------------------------------------------------------------------------------------------
vec4 lava(vec3 geoNormal, vec3 viewDir, vec3 lightScale, out vec3 bloom) {
    float t = u.cameraPos.w;
    vec2 p = FragPos.xz;
    // Deux champs d'écoulement lents, déphasés : la croûte dérive et se déchire.
    vec2 flowA = 0.045 * vec2(sin(p.x * 1.3 + t * 0.30 + 1.7 * cos(p.y * 0.9 - t * 0.21)),
                              cos(p.y * 1.1 - t * 0.27 + 1.5 * sin(p.x * 0.7 + t * 0.17)));
    vec2 flowB = 0.06 * vec2(cos(p.y * 0.8 + t * 0.22), sin(p.x * 0.9 - t * 0.19));
    vec3 a = pow(texture(textureArray, vec3(TexCoord + flowA, Layer)).rgb, vec3(2.2));
    vec3 b = pow(texture(textureArray, vec3(TexCoord * 0.75 + flowB + vec2(t * 0.012, -t * 0.009), Layer)).rgb, vec3(2.2));
    vec3 lavaCol = mix(a, b, 0.35);
    float lum = dot(lavaCol, vec3(0.30, 0.59, 0.11));
    // Croûte sombre ↔ veines incandescentes.
    float hot = smoothstep(0.10, 0.55, lum);
    float pulse = 0.92 + 0.08 * sin(t * 1.6 + p.x * 0.5 + p.y * 0.4);
    // Plaques de croûte plus froides qui dérivent avec l'écoulement.
    vec2 cp = p * 0.85 + flowA * 6.0 + vec2(t * 0.02, t * 0.015);
    float plate = 0.6 * vnoise(cp) + 0.4 * vnoise(cp * 2.3 + 11.0);
    float cool = smoothstep(0.52, 0.72, plate) * 0.75;
    hot *= 1.0 - cool;
    vec3 emissive = lavaCol * mix(0.45, 3.0, hot) * (1.0 - 0.45 * cool) * pulse * vec3(1.10, 0.85, 0.60);
    // Léger lustre de la croûte (reflet du ciel très atténué) : la surface n'est pas un aplat.
    float cosTheta = max(dot(viewDir, geoNormal), 0.0);
    float sheen = pow(1.0 - cosTheta, 4.0) * (1.0 - hot) * 0.10;
    vec3 color = emissive + skyColor(reflect(-viewDir, geoNormal)) * sheen
               + lavaCol * lightScale * 0.12;
    bloom = min(emissive * hot, vec3(1.6)) * 0.5;
    return vec4(color, 1.0);
}

void main() {
    if (IsFluid < 0.5) discard;

    vec3 camPos = u.cameraPos.xyz;
    vec3 viewDir = normalize(camPos - FragPos);
    vec3 geoNormal = normalize(cross(dFdx(FragPos), dFdy(FragPos)));
    if (dot(geoNormal, viewDir) < 0.0) geoNormal = -geoNormal;
    float shadow = ShadowCalculation(FragPosLightSpace, geoNormal);
    vec3 lightScale = u.ambientColor.xyz + u.lightColor.xyz * (1.0 - shadow);

    vec4 texColor = texture(textureArray, vec3(TexCoord, Layer));
    vec3 linearTex = pow(texColor.rgb, vec3(2.2));

    bool isLava;
    if (u.flags.w >= 0) {
        isLava = abs(Layer - float(u.flags.w)) < 0.5;
    } else {
        isLava = linearTex.r > 0.6 && linearTex.b < 0.2;
    }

    vec3 bloom;
    vec4 outColor;
    if (isLava) {
        outColor = lava(geoNormal, viewDir, lightScale, bloom);
    } else {
        outColor = water(geoNormal, viewDir, shadow, lightScale, linearTex, bloom);
    }
    FragColor = outColor;
    BrightColor = vec4(bloom, 1.0);
}
