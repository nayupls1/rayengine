#version 330
in vec2 fragTexCoord;
in vec4 fragColor;
uniform sampler2D texture0;
uniform vec4 colDiffuse;
uniform vec4 voxelTileRect;
uniform int rayengineAlphaMode;
uniform float rayengineAlphaCutoff;
out vec4 finalColor;
void main() {
    vec2 uv = voxelTileRect.xy + fract(fragTexCoord) * voxelTileRect.zw;
    // Preserve pre-wrap derivatives so tile boundaries do not select coarse mips.
    vec4 color = textureGrad(texture0, uv,
        dFdx(fragTexCoord) * voxelTileRect.zw,
        dFdy(fragTexCoord) * voxelTileRect.zw) * colDiffuse * fragColor;
    if (rayengineAlphaMode == 1 && color.a < rayengineAlphaCutoff) discard;
    if (rayengineAlphaMode != 2) color.a = 1.0;
    finalColor = color;
}
