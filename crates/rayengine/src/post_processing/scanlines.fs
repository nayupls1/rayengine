#version 330
in vec2 fragTexCoord;
in vec4 fragColor;
uniform sampler2D texture0;
uniform float strength;
uniform float lines;
out vec4 finalColor;
void main() {
    vec4 c = texture(texture0, fragTexCoord) * fragColor;
    float scan = 0.5 + 0.5 * cos(fragTexCoord.y * max(lines, 1.0) * 6.2831853);
    finalColor = vec4(c.rgb * (1.0 - clamp(strength, 0.0, 1.0) * scan), c.a);
}
