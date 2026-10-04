#version 330
in vec2 fragTexCoord;
in vec4 fragColor;
in vec3 worldPosition;
in vec3 worldNormal;
uniform sampler2D texture0;
uniform vec4 colDiffuse;
uniform int rayengineAlphaMode;
uniform int rayenginePremultipliedTexture;
uniform float rayengineAlphaCutoff;
uniform vec3 ambient;
uniform vec3 direction;
uniform vec3 directionalColor;
uniform int pointCount;
uniform vec3 pointPosition[4];
uniform vec3 pointColor[4];
uniform float pointRange[4];
out vec4 finalColor;
void main() {
    vec4 sampled = texture(texture0, fragTexCoord);
    if (rayenginePremultipliedTexture != 0) sampled.rgb = sampled.a > 0.0 ? sampled.rgb / sampled.a : vec3(0.0);
    vec4 color = sampled * colDiffuse * fragColor;
    if (rayengineAlphaMode == 1 && color.a < rayengineAlphaCutoff) discard;
    if (rayengineAlphaMode != 2) color.a = 1.0;
    vec3 normal = normalize(worldNormal);
    vec3 irradiance = ambient + directionalColor * max(dot(normal, -direction), 0.0);
    for (int i = 0; i < pointCount; i++) {
        vec3 delta = pointPosition[i] - worldPosition;
        float distance = length(delta);
        float attenuation = max(1.0 - distance / pointRange[i], 0.0);
        // At the light position, direction is undefined: use zero diffuse.
        vec3 towardLight = delta / max(distance, 0.000001);
        irradiance += pointColor[i] * max(dot(normal, towardLight), 0.0) * attenuation * attenuation;
    }
    finalColor = vec4(color.rgb * irradiance, color.a);
}
