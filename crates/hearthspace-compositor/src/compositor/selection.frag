precision highp float;
varying vec2 v_coords;
uniform float alpha;
uniform vec2 area_size;
uniform vec4 ink;
uniform vec3 shape; // radius, stroke width, fill alpha
uniform mat3 tex_matrix;
#if defined(DEBUG_FLAGS)
uniform float tint;
#endif

float rounded_distance(vec2 point, vec2 half_size, float radius) {
    radius = min(radius, min(half_size.x, half_size.y));
    vec2 q = abs(point) - half_size + radius;
    return length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - radius;
}

void main() {
    vec2 half_size = area_size * 0.5;
    vec2 point = v_coords * area_size - half_size;
    float pixel_size = max(length(tex_matrix[0].xy * area_size),
        length(tex_matrix[1].xy * area_size));
    float aa = max(pixel_size * 0.5, 0.0001);
    float outer = 1.0 - smoothstep(-aa, aa, rounded_distance(point, half_size, shape.x));
    float inner = min(half_size.x, half_size.y) <= shape.y ? 0.0 :
        1.0 - smoothstep(-aa, aa, rounded_distance(point, half_size - shape.y, max(0.0,shape.x-shape.y)));
    // Both the border and translucent fill retain one cached element identity.
    float opacity = (outer * shape.z + max(outer - inner, 0.0) * (1.0-shape.z)) * ink.a * alpha;
    gl_FragColor = vec4(ink.rgb * opacity, opacity);
#if defined(DEBUG_FLAGS)
    if (tint == 1.0) gl_FragColor = vec4(0.0, 0.2, 0.0, 0.2) + gl_FragColor * 0.8;
#endif
}
