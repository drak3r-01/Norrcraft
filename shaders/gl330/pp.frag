#version 330 core
out vec4 FragColor;
in vec2 TexCoords;
uniform sampler2D scene;
uniform sampler2D bloomBlur;
uniform sampler2D depthMap;
uniform sampler2D cloudTexture;
uniform sampler2D noiseTexture;
uniform mat4 inverseView;
uniform mat4 inverseProj;
uniform vec3 uCameraPos;
uniform vec3 uFogColor;
uniform vec3 uLightDir;
uniform vec3 uZenithColor;
uniform vec3 uHorizonColor;
uniform vec3 uSunColor;
uniform int uFluidState;
uniform float uTime;
uniform float uAuroraIntensity;
uniform float uAuroraPhase;
uniform float uHeatIntensity;
float hash(vec2 p) { return fract(sin(dot(p, vec2(12.9898, 78.233))) * 43758.5453); }
float noise(vec2 p) { return texture(noiseTexture, p / 256.0).r; }
float fbm(vec2 p) {
    float f = 0.0, a = 0.5;
    for(int i = 0; i < 4; i++) {
        f += a * noise(p);
        p *= 2.0; a *= 0.5;
    }
    return f;
}
vec3 getAurora(vec3 ro, vec3 rd, float time, vec2 uv, float phase) {
    if (rd.y < 0.02) return vec3(0.0);
    float hMin = 80.0;
    float hMax = 200.0;
    float d1 = (hMin - ro.y) / rd.y;
    float d2 = (hMax - ro.y) / rd.y;
    if (d2 <= 0.0) return vec3(0.0);
    d1 = max(d1, 0.0);
    float stepSize = (d2 - d1) / 30.0;
    float dither = hash(uv * 1000.0);
    float t = d1 + stepSize * dither;
    vec3 col = vec3(0.0);
    float totalAlpha = 0.0;
    for (int i = 0; i < 30; i++) {
        vec3 p = ro + rd * t;
        vec2 p_xz = (p.xz + p.y * 0.5) * 0.002;
        float macroNoise = fbm(p_xz * 0.5 + phase * 0.2);
        float mask = smoothstep(0.4, 0.75, macroNoise);
        if (mask > 0.0) {
            float n1 = fbm(p_xz * 1.5 + time * 0.01);
            vec2 warped = p_xz + vec2(n1, fbm(p_xz + vec2(1.0))) * 1.5;
            float wave = sin(warped.x * 6.0 + warped.y * 4.0 + time * 0.2) * 0.5 + 0.5;
            float ribbon = smoothstep(0.35, 0.85, wave) * smoothstep(0.2, 0.8, n1);
            if (ribbon > 0.0) {
                float hNorm = (p.y - hMin) / (hMax - hMin);
                float vProfile = smoothstep(0.0, 0.2, hNorm) * smoothstep(1.0, 0.3, hNorm);
                vec3 cBase = vec3(0.1, 0.9, 0.4);
                vec3 cTop = vec3(0.4, 0.1, 0.8);
                vec3 cCol = mix(cBase, cTop, hNorm);
                float redPulse = fbm(p_xz * 2.0 - time * 0.01);
                cCol = mix(vec3(0.9, 0.1, 0.2), cCol, smoothstep(0.0, 0.2, hNorm + redPulse * 0.1));
                float angleCompensate = mix(0.3, 1.2, rd.y);
                float alpha = ribbon * vProfile * mask * angleCompensate * 0.8;
                col += cCol * alpha * (1.0 - totalAlpha);
                totalAlpha += alpha * (1.0 - totalAlpha);
                if (totalAlpha > 0.95) break;
            }
        }
        t += stepSize;
    }
    return col * 1.5;
}
vec3 ACESFilm(vec3 x) {
    float a = 2.51f;
    float b = 0.03f;
    float c = 2.43f;
    float d = 0.59f;
    float e = 0.14f;
    return clamp((x*(a*x+b))/(x*(c*x+d)+e), 0.0, 1.0);
}
void main() {
    vec2 uv = TexCoords;
    float nHeat = 0.0;
    if (uFluidState == 0 && uHeatIntensity > 0.0) {
        nHeat = fbm(uv * 15.0 + vec2(0.0, -uTime * 3.0)) * 2.0 - 1.0;
        uv += nHeat * 0.02 * uHeatIntensity;
    } else if (uFluidState == 1) {
        uv.x += sin(uv.y * 15.0 + uTime * 2.0) * 0.003;
        uv.y += cos(uv.x * 15.0 + uTime * 2.0) * 0.003;
    } else if (uFluidState == 2) {
        uv.x += sin(uv.y * 3.0 + uTime * 0.4) * 0.006;
        uv.y += cos(uv.x * 3.0 + uTime * 0.4) * 0.006;
    }
    vec3 hdrColor = texture(scene, uv).rgb;
    vec3 bloomColor = texture(bloomBlur, uv).rgb;
    hdrColor += bloomColor;
    if (uFluidState == 0 && uHeatIntensity > 0.0) {
        vec3 redColor = vec3(1.2, 0.1, 0.0);
        vec3 orangeColor = vec3(1.0, 0.5, 0.0);
        vec3 lavaGlow = mix(redColor, orangeColor, nHeat * 0.5 + 0.5);
        hdrColor += lavaGlow * uHeatIntensity * 0.35;
    }
    float depth = texture(depthMap, uv).r;
    vec4 ndc = vec4(uv * 2.0 - 1.0, depth * 2.0 - 1.0, 1.0);
    vec4 viewPos = inverseProj * ndc;
    viewPos /= viewPos.w;
    vec4 worldPos = inverseView * viewPos;
    vec3 viewDir = normalize(worldPos.xyz - uCameraPos);
    float dist = length(worldPos.xyz - uCameraPos);
    if (depth >= 1.0) dist = 10000.0;
    float fogDensity = 0.002;
    float effectiveDist = dist;
    float falloff = 1.8;
    if (uFluidState == 1) {
        fogDensity = 0.015;
        effectiveDist = dist;
        falloff = 1.2;
    } else if (uFluidState == 2) {
        fogDensity = 0.3;
        effectiveDist = dist;
        falloff = 1.0;
    }
    float fogFactor = exp(-pow((effectiveDist * fogDensity), falloff));
    fogFactor = clamp(fogFactor, 0.0, 1.0);
    vec3 zenithColor = uZenithColor;
    vec3 horizonColor = uHorizonColor;
    float skyGradient = clamp(pow(max(0.0, viewDir.y + 0.05), 0.6), 0.0, 1.0);
    vec3 skyColor = mix(horizonColor, zenithColor, skyGradient);
    if (uFluidState == 0 && uAuroraIntensity > 0.0) {
        skyColor += getAurora(uCameraPos, viewDir, uTime, uv, uAuroraPhase) * uAuroraIntensity;
    }
    float sunDot = max(dot(viewDir, normalize(-uLightDir)), 0.0);
    vec3 sunHalo = pow(sunDot, 64.0) * uSunColor;
    if (uFluidState > 0) sunHalo = vec3(0.0);
    vec3 finalFogColor = skyColor + sunHalo;
    if (uFluidState == 1) {
        vec3 waterColor = pow(uFogColor, vec3(2.2));
        float depthFactor = clamp(exp(-(64.0 - uCameraPos.y) * 0.06), 0.02, 1.0);
        vec3 deepWater = vec3(0.02, 0.06, 0.15);
        finalFogColor = mix(deepWater, waterColor, depthFactor);
        float upFactor = max(0.0, viewDir.y);
        finalFogColor += uSunColor * pow(upFactor, 8.0) * 0.15 * depthFactor;
    } else if (uFluidState == 2) {
        finalFogColor = pow(uFogColor, vec3(2.2));
    }
    hdrColor = mix(finalFogColor, hdrColor, fogFactor);
    if (uFluidState == 0 && viewDir.y > 0.0) {
        float cloudMinY = 130.0;
        float cloudMaxY = 134.0;
        float t1 = (cloudMinY - uCameraPos.y) / viewDir.y;
        float t2 = (cloudMaxY - uCameraPos.y) / viewDir.y;
        float tStart = max(t1, 0.0);
        float tEnd = max(t2, 0.0);
        if (tEnd > 0.0 && tStart < 500.0 && (depth >= 1.0 || dist > tStart)) {
            tEnd = min(tEnd, 500.0);
            if (depth < 1.0) tEnd = min(tEnd, dist);
            int steps = 15;
            float stepSize = (tEnd - tStart) / float(steps);
            float t = tStart;
            vec3 cloudColorAcc = vec3(0.0);
            float cloudAlpha = 0.0;
            float cloudScale = 12.0;
            vec3 sunColor = uSunColor;
            float phaseFunction = pow(sunDot, 4.0);
            for (int i = 0; i < steps; i++) {
                vec3 p = uCameraPos + viewDir * t;
                vec2 p_moved = p.xz;
                p_moved.x += uTime * 3.0;
                vec2 gridPos = floor(p_moved / cloudScale);
                vec2 uvCloud = gridPos / 256.0;
                vec4 tex = texture(cloudTexture, uvCloud);
                float density = tex.r * tex.a;
                if (density > 0.1) {
                    float heightGradientC = clamp((p.y - cloudMinY) / (cloudMaxY - cloudMinY), 0.0, 1.0);
                    vec3 ambientCloud = mix(horizonColor, zenithColor, heightGradientC) * 0.8;
                    vec3 cColor = mix(ambientCloud, sunColor, phaseFunction * 0.6);
                    if (p.y < cloudMinY + 0.5) cColor *= 0.6;
                    float sampleAlpha = density * 0.4;
                    cloudColorAcc += cColor * sampleAlpha * (1.0 - cloudAlpha);
                    cloudAlpha += sampleAlpha * (1.0 - cloudAlpha);
                    if (cloudAlpha > 0.95) break;
                }
                t += stepSize;
            }
            if (cloudAlpha > 0.0) {
                float effectiveCloudDist = tStart;
                float cloudFogFactor = exp(-pow((effectiveCloudDist * fogDensity), falloff));
                cloudFogFactor = clamp(cloudFogFactor, 0.0, 1.0);
                vec3 foggedCloud = mix(finalFogColor, cloudColorAcc / cloudAlpha, cloudFogFactor);
                hdrColor = mix(hdrColor, foggedCloud, cloudAlpha);
            }
        }
    }
    vec3 warmFilter = vec3(1.15, 1.05, 0.95);
    if (uFluidState == 0) {
        hdrColor *= warmFilter;
    } else if (uFluidState == 1) {
        float depthAttenuation = clamp(exp(-(64.0 - uCameraPos.y) * 0.04), 0.15, 1.0);
        vec3 waterFilter = mix(vec3(0.4, 0.7, 0.9), vec3(1.0), 0.3);
        hdrColor *= waterFilter * depthAttenuation;
        float vignette = 1.0 - smoothstep(0.4, 1.5, length(uv - 0.5));
        hdrColor *= mix(0.6, 1.0, vignette);
    } else if (uFluidState == 2) {
        hdrColor *= vec3(1.5, 0.5, 0.1);
    }
    vec3 mapped = ACESFilm(hdrColor);
    mapped = pow(mapped, vec3(1.0 / 2.2));
    FragColor = vec4(mapped, 1.0);
}