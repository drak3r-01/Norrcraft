#version 330 core
layout (location = 0) out vec4 FragColor;
layout (location = 1) out vec4 BrightColor;
in vec2 TexCoord;
in float Layer;
in vec3 Color;
in vec3 FragPos;
in vec4 FragPosLightSpace;
in float IsFluid;
in float IsLeaf;
uniform sampler2DArray textureArray;
uniform sampler2D shadowMap;
uniform sampler2D opaqueDepth;
uniform vec2 uScreenSize;
uniform vec3 uCameraPos;
uniform vec3 uLightDir;
uniform vec3 uAmbientColor;
uniform vec3 uLightColor;
uniform int uRenderPass;
uniform mat4 projection;
float getLinearDepth(float depth) {
    float zNdc = depth * 2.0 - 1.0;
    return projection[3][2] / (zNdc + projection[2][2]);
}
float ShadowCalculation(vec4 fragPosLightSpace, vec3 normal) {
    vec3 projCoords = fragPosLightSpace.xyz / fragPosLightSpace.w;
    projCoords = projCoords * 0.5 + 0.5;
    if(projCoords.z > 1.0) return 0.0;
    float currentDepth = projCoords.z;
    float bias = max(0.005 * (1.0 - dot(normal, -uLightDir)), 0.001);
    float shadow = 0.0;
    vec2 texelSize = 1.0 / textureSize(shadowMap, 0);
    for(int x = -1; x <= 1; ++x) {
        for(int y = -1; y <= 1; ++y) {
            float pcfDepth = texture(shadowMap, projCoords.xy + vec2(x, y) * texelSize).r;
            shadow += currentDepth - bias > pcfDepth ? 1.0 : 0.0;
        }
    }
    return shadow / 9.0;
}
void main() {
    if (uRenderPass == 0 && IsFluid > 0.5) discard;
    if (uRenderPass == 1 && IsFluid < 0.5) discard;
    vec4 texColor = texture(textureArray, vec3(TexCoord, Layer));
    float alpha = texColor.a;
    if (uRenderPass == 0) {
        if (texColor.a < 0.01) discard;
        texColor.rgb = clamp(texColor.rgb / texColor.a, 0.0, 1.0);
        alpha = (texColor.a - 0.5) / max(fwidth(texColor.a), 0.0001) + 0.5;
        if (alpha < 0.5) discard;
    } else {
        if (alpha < 0.05) discard;
    }
    vec3 linearTex = pow(texColor.rgb, vec3(2.2));
    vec3 normal = normalize(cross(dFdx(FragPos), dFdy(FragPos)));
    vec3 viewDir = normalize(uCameraPos - FragPos);
    if (dot(normal, viewDir) < 0.0) normal = -normal;
    float shadow = ShadowCalculation(FragPosLightSpace, normal);
    vec3 finalColor = linearTex * Color * (uAmbientColor + uLightColor * (1.0 - shadow));
    if (IsLeaf > 0.5) {
        float backlight = max(0.0, dot(viewDir, normalize(-uLightDir)));
        float sss = pow(backlight, 3.0) * 0.6;
        float wrapLight = max(0.0, dot(normal, uLightDir) * 0.5 + 0.5);
        finalColor += linearTex * Color * uLightColor * sss * wrapLight * (1.0 - shadow * 0.7);
    }
    float spec = 0.0;
    bool isLava = (IsFluid > 0.5 && linearTex.r > 0.6 && linearTex.b < 0.2);
    if (IsFluid > 0.5) {
        float opaqueDepthVal = texelFetch(opaqueDepth, ivec2(gl_FragCoord.xy), 0).r;
        float opLinDepth = getLinearDepth(opaqueDepthVal);
        float myLinDepth = getLinearDepth(gl_FragCoord.z);
        float thickness = max(opLinDepth - myLinDepth, 0.0);
        vec3 reflectDir = reflect(normalize(uLightDir), normal);
        spec = pow(max(dot(viewDir, reflectDir), 0.0), 48.0);
        float cosTheta = max(dot(viewDir, normal), 0.0);
        float r0 = 0.02;
        float fresnel = r0 + (1.0 - r0) * pow(1.0 - cosTheta, 5.0);
        float density = isLava ? 1.5 : 0.08;
        float depthAlpha = 1.0 - exp(-thickness * density);
        alpha = mix(depthAlpha, 1.0, fresnel);
        alpha = clamp(alpha, isLava ? 0.9 : 0.3, 0.98);
        vec3 deepColor = linearTex * Color * (isLava ? vec3(2.5, 1.0, 0.1) : vec3(0.25));
        vec3 shallowColor = linearTex * Color * (isLava ? vec3(3.0, 1.5, 0.2) : vec3(1.0));
        finalColor = mix(shallowColor, deepColor, depthAlpha);
        finalColor += uLightColor * 2.0 * spec * (1.0 - shadow);
    }
    FragColor = vec4(finalColor, alpha);
    if(isLava) {
        BrightColor = vec4(FragColor.rgb * 1.8, 1.0);
    } else if(spec > 0.2 || (IsLeaf > 0.5 && finalColor.g > 1.2)) {
        BrightColor = vec4(FragColor.rgb, 1.0);
    } else {
        BrightColor = vec4(0.0, 0.0, 0.0, 1.0);
    }
}