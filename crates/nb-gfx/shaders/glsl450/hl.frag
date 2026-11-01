#version 450
// Port Vulkan de shaders/gl330/hl.frag (inchangé). La pipeline des requêtes d'occlusion
// masque les deux attachements (colorWriteMask = 0) ; la surbrillance n'en a qu'un.
layout (location = 0) out vec4 FragColor;
void main() {
    FragColor = vec4(0.0, 0.0, 0.0, 1.0);
}
