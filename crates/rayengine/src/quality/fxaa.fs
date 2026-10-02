#version 330
// Luminance-directed FXAA resolve. Sample offsets are internal world texels.
in vec2 fragTexCoord;
out vec4 finalColor;
uniform sampler2D texture0;
uniform vec2 inverseSize;
float luma(vec3 c) { return dot(c, vec3(0.299, 0.587, 0.114)); }
vec4 sampleAt(vec2 p) { return texture(texture0, p); }
void main() {
    vec2 p = fragTexCoord;
    vec4 center = sampleAt(p);
    float m = luma(center.rgb);
    float nw = luma(sampleAt(p + vec2(-1, -1)*inverseSize).rgb);
    float ne = luma(sampleAt(p + vec2( 1, -1)*inverseSize).rgb);
    float sw = luma(sampleAt(p + vec2(-1,  1)*inverseSize).rgb);
    float se = luma(sampleAt(p + vec2( 1,  1)*inverseSize).rgb);
    float lo = min(m, min(min(nw,ne),min(sw,se)));
    float hi = max(m, max(max(nw,ne),max(sw,se)));
    if (hi-lo < max(0.0312,hi*0.125)) { finalColor=center; return; }
    vec2 direction = vec2(sw+se-nw-ne,nw+sw-ne-se);
    float reduction = max((nw+ne+sw+se)*0.03125,0.0078125);
    direction = clamp(direction/(min(abs(direction.x),abs(direction.y))+reduction),vec2(-8),vec2(8))*inverseSize;
    vec4 inner = (sampleAt(p-direction/6.0)+sampleAt(p+direction/6.0))*0.5;
    vec4 outer = inner*0.5+(sampleAt(p-direction*0.5)+sampleAt(p+direction*0.5))*0.25;
    float edge = luma(outer.rgb);
    finalColor = (edge < lo || edge > hi) ? inner : outer;
}
