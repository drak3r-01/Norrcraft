// Ombres douces partagées (terrain, fluides, mobs). À inclure après la déclaration de
// `sampler2D shadowMap` et de frame.glsl.
//
//  - décalage le long de la normale (en texels de la carte) + biais selon la pente : pas d'acné sans
//    « décollage » des ombres ;
//  - PCSS simplifié : recherche de bloqueurs (8 prises) pour estimer la pénombre, puis filtre PCF à
//    16 prises en disque de Vogel, tourné par pixel (bruit de gradient entrelacé) : bords d'ombre
//    doux et sans pixels carrés, même à 1024² ;
//  - fondu sur les bords de la carte (plus de coupure franche d'ombre au loin).
// La carte est calée sur ses texels côté CPU (camera::light_space_gl) : pas de scintillement quand
// la caméra bouge.
const float SH_HALF_EXTENT = 120.0;   // = camera::LIGHT_HALF_EXTENT
const float SH_DEPTH_RANGE = 399.0;   // far - near de la projection de la lumière

float shIgn(vec2 p) { return fract(52.9829189 * fract(dot(p, vec2(0.06711056, 0.00583715)))); }

vec2 shVogel(int i, int n, float rot) {
    float r = sqrt((float(i) + 0.5) / float(n));
    float a = float(i) * 2.39996323 + rot;
    return r * vec2(cos(a), sin(a));
}

// Part d'ombre [0,1]. `normal` = normale géométrique tournée vers l'observateur (ou la verticale
// pour les surfaces sans normale).
float softShadow(vec3 worldPos, vec3 normal, float extraBias) {
    float texelW = 2.0 * SH_HALF_EXTENT / float(textureSize(shadowMap, 0).x);
    vec3 L = -u.lightDir.xyz;
    float cosT = clamp(dot(normal, L), 0.0, 1.0);
    float sinT = sqrt(1.0 - cosT * cosT);
    // Décalage normal : plus fort en incidence rasante.
    vec3 p = worldPos + normal * texelW * (1.2 + 1.8 * sinT);
    vec4 ls = u.lightSpaceVk * vec4(p, 1.0);
    vec3 pc = ls.xyz / ls.w;
    pc.xy = pc.xy * 0.5 + 0.5;
    if (pc.z > 1.0 || pc.z < 0.0) return 0.0;
    float edge = min(min(pc.x, 1.0 - pc.x), min(pc.y, 1.0 - pc.y));
    float edgeFade = smoothstep(0.0, 0.04, edge);
    if (edgeFade <= 0.0) return 0.0;

    float bias = (texelW * (0.8 + 1.6 * sinT / max(cosT, 0.2))) / SH_DEPTH_RANGE + extraBias;
    float z = pc.z - bias;
    vec2 texel = 1.0 / vec2(textureSize(shadowMap, 0));
    float rot = shIgn(gl_FragCoord.xy) * 6.2831853;

    // Recherche de bloqueurs.
    float blockerSum = 0.0;
    float blockers = 0.0;
    for (int i = 0; i < 8; ++i) {
        float d = texture(shadowMap, pc.xy + shVogel(i, 8, rot) * 3.5 * texel).r;
        if (d < z) { blockerSum += d; blockers += 1.0; }
    }
    if (blockers < 0.5) return 0.0;
    float avgBlocker = blockerSum / blockers;
    // Pénombre : proportionnelle à la distance bloqueur -> récepteur (en blocs), bornée.
    float pen = clamp((z - avgBlocker) * SH_DEPTH_RANGE * 0.35, 1.0, 4.5);

    float shadow = 0.0;
    for (int i = 0; i < 16; ++i) {
        float d = texture(shadowMap, pc.xy + shVogel(i, 16, rot) * pen * texel).r;
        shadow += z > d ? 1.0 : 0.0;
    }
    return (shadow / 16.0) * edgeFade;
}
