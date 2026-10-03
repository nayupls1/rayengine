#version 330
in vec2 fragTexCoord;
in vec4 fragColor;
uniform sampler2D texture0;
uniform vec3 gain;
uniform vec3 lift;
out vec4 finalColor;
void main() {
    vec4 c = texture(texture0, fragTexCoord) * fragColor;
    finalColor = vec4(clamp(c.rgb * gain + lift * c.a, vec3(0.0), vec3(c.a)), c.a);
}
