#version 330 core
layout (location = 0) out vec4 FragColor;
layout (location = 1) out vec4 BrightColor;
in vec3 FragPos;
uniform sampler2D cloudTexture;
uniform vec3 uCameraPos;
uniform vec3 uFogColor;
uniform float uTime;
uniform vec3 uLightDir;
uniform vec3 uZenithColor;
uniform vec3 uHorizonColor;
uniform vec3 uSunColor;
void main() {
    vec3 rayDir = normalize(FragPos - uCameraPos);
    float cloudMinY = 130.0;
    float cloudMaxY = 134.0;
    if (abs(rayDir.y) < 0.001) discard;
    float t1 = (cloudMinY - uCameraPos.y) / rayDir.y;
    float t2 = (cloudMaxY - uCameraPos.y) / rayDir.y;
    float tStart = max(min(t1, t2), 0.0);
    float tEnd = max(t1, t2);
    if (tEnd < 0.0 || tStart > 500.0) discard;
    tEnd = min(tEnd, 500.0);
    int steps = 15;
    float stepSize = (tEnd - tStart) / float(steps);
    float t = tStart;
    bool hit = false;
    vec3 hitPos = vec3(0.0);
    float cloudScale = 12.0;
    for (int i = 0; i < steps; i++) {
        vec3 p = uCameraPos + rayDir * t;
        vec2 p_moved = p.xz;
        p_moved.x += uTime * 3.0;
        vec2 gridPos = floor(p_moved / cloudScale);
        vec2 uv = gridPos / 256.0;
        vec4 tex = texture(cloudTexture, uv);
        float density = tex.r * tex.a;
        if (density > 0.1) {
            hit = true;
            hitPos = p;
            break;
        }
        t += stepSize;
    }
    if (!hit) discard;
    vec3 zenithColor = uZenithColor;
    vec3 horizonColor = uHorizonColor;
    vec3 sunColor = uSunColor;
    float sunDot = max(dot(rayDir, normalize(-uLightDir)), 0.0);
    float phaseFunction = pow(sunDot, 4.0);
    float heightGradient = clamp((hitPos.y - cloudMinY) / (cloudMaxY - cloudMinY), 0.0, 1.0);
    vec3 ambientCloud = mix(horizonColor, zenithColor, heightGradient) * 0.8;
    vec3 cloudBaseColor = mix(ambientCloud, sunColor, phaseFunction * 0.6);
    if (hitPos.y < cloudMinY + 0.5) cloudBaseColor *= 0.6;
    float dist = length(hitPos - uCameraPos);
    float fogDensity = 0.005;
    float effectiveDist = max(0.0, dist - 24.0);
    float fogFactor = exp(-pow((effectiveDist * fogDensity), 2.5));
    fogFactor = clamp(fogFactor, 0.0, 1.0);
    float skyGradient = clamp(pow(max(0.0, rayDir.y + 0.05), 0.6), 0.0, 1.0);
    vec3 skyColor = mix(horizonColor, zenithColor, skyGradient);
    vec3 sunHalo = pow(sunDot, 64.0) * sunColor;
    vec3 finalFogColor = skyColor + sunHalo;
    vec3 finalCloud = mix(finalFogColor, cloudBaseColor, fogFactor);
    FragColor = vec4(finalCloud, 0.9);
    float brightness = dot(FragColor.rgb, vec3(0.2126, 0.7152, 0.0722));
    if(brightness > 1.2) BrightColor = vec4(FragColor.rgb, 1.0);
    else BrightColor = vec4(0.0, 0.0, 0.0, 1.0);
}