#version 330
in vec2 fragTexCoord;
in vec4 fragColor;
uniform sampler2D texture0;
uniform vec2 lantern;
uniform vec2 torch0;
uniform vec2 torch1;
uniform vec2 torch2;
uniform vec2 torch3;
uniform float time;
uniform float hurt;
out vec4 finalColor;
float glow(vec2 p, vec2 center, float radius) {
    return pow(max(0.0, 1.0 - distance(p, center) / radius), 1.5);
}
void main() {
    vec2 p = vec2(fragTexCoord.x, 1.0 - fragTexCoord.y) * vec2(960.0, 640.0);
    vec4 scene = texture(texture0, fragTexCoord) * fragColor;
    float fire = glow(p, torch0, 180.0) + glow(p, torch1, 180.0)
        + glow(p, torch2, 180.0) + glow(p, torch3, 180.0);
    fire *= 0.88 + 0.06 * sin(time * 8.0) + 0.04 * sin(time * 13.0);
    float player = glow(p, lantern, 240.0);
    vec3 light = vec3(0.48, 0.55, 0.66) + fire * vec3(0.95, 0.54, 0.20)
        + player * vec3(0.60, 0.57, 0.39);
    vec2 edge = fragTexCoord * 2.0 - 1.0;
    float danger = hurt * smoothstep(0.35, 1.3, dot(edge, edge));
    finalColor = vec4(scene.rgb * min(light, vec3(1.35)) * (1.0 - danger * 0.65)
        + vec3(0.20, 0.012, 0.025) * danger * scene.a, scene.a);
}
