#version 330
in vec2 fragTexCoord;
in vec4 fragColor;
uniform sampler2D texture0;
uniform vec4 colDiffuse;
uniform int rayengineAlphaMode;
uniform int rayenginePremultipliedTexture;
uniform float rayengineAlphaCutoff;
out vec4 finalColor;
void main() {
    vec4 sampled = texture(texture0, fragTexCoord);
    if (rayenginePremultipliedTexture != 0) sampled.rgb = sampled.a > 0.0 ? sampled.rgb / sampled.a : vec3(0.0);
    vec4 color = sampled * colDiffuse * fragColor;
    if (rayengineAlphaMode == 1 && color.a < rayengineAlphaCutoff) discard;
    if (rayengineAlphaMode != 2) color.a = 1.0;
    finalColor = color;
}
