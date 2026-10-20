uniform vec3 clip_x;
uniform vec3 clip_y;
uniform vec2 clip_size;
uniform float clip_radius;

float window_mask() {
    vec3 fragment = vec3(gl_FragCoord.xy, 1.0);
    vec2 position = vec2(dot(clip_x, fragment), dot(clip_y, fragment));
    vec2 half_size = clip_size * 0.5;
    float radius = min(clip_radius, min(half_size.x, half_size.y));
    vec2 q = abs(position - half_size) - half_size + radius;
    float distance = length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - radius;
    return 1.0 - smoothstep(-0.5, 0.5, distance);
}
