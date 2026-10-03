#version 330
in vec2 fragTexCoord;
in vec4 fragColor;
uniform sampler2D texture0;
uniform float strength;
out vec4 finalColor;
void main() {
    vec4 c = texture(texture0, fragTexCoord) * fragColor;
    vec2 p = fragTexCoord * 2.0 - 1.0;
    float shade = 1.0 - clamp(strength, 0.0, 1.0) * smoothstep(0.15, 1.4, dot(p, p));
    finalColor = vec4(c.rgb * shade, c.a);
}
