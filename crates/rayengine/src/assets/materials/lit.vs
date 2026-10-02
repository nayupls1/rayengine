#version 330
in vec3 vertexPosition;
in vec2 vertexTexCoord;
in vec3 vertexNormal;
in vec4 vertexColor;
uniform mat4 mvp;
uniform mat4 matModel;
uniform mat4 matNormal;
out vec2 fragTexCoord;
out vec4 fragColor;
out vec3 worldPosition;
out vec3 worldNormal;
void main() {
    fragTexCoord = vertexTexCoord;
    fragColor = vertexColor;
    worldPosition = vec3(matModel * vec4(vertexPosition, 1.0));
    worldNormal = vec3(matNormal * vec4(vertexNormal, 0.0));
    gl_Position = mvp * vec4(vertexPosition, 1.0);
}
