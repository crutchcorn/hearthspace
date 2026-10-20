precision highp float;
varying vec2 v_coords;
uniform float alpha;
uniform vec2 area_size;
uniform vec2 window_size;
uniform vec2 window_offset;
uniform float zoom;
uniform float radius;
uniform float active;
uniform float hovered;
uniform float mode;
uniform float photo;
uniform mat3 tex_matrix;
#if defined(DEBUG_FLAGS)
uniform float tint;
#endif

float rounded_distance(vec2 p, vec2 half_size, float r) {
    r = min(r, min(half_size.x, half_size.y));
    vec2 q = abs(p) - half_size + r;
    return length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - r;
}

vec4 over(vec4 front, vec4 back) {
    return front + back * (1.0 - front.a);
}

vec4 premultiply(vec3 color, float opacity) {
    return vec4(color * opacity, opacity);
}

// Normal-CDF approximation of a blurred rounded edge. All inputs remain in
// scene coordinates so output scale and canvas zoom do not change the material.
float gaussian_edge(float distance, float sigma) {
    float x = abs(distance) / (sigma * 1.41421356);
    float t = 1.0 / (1.0 + 0.3275911 * x);
    float erf = 1.0 - (((((1.061405429 * t - 1.453152027) * t)
        + 1.421413741) * t - 0.284496736) * t + 0.254829592) * t * exp(-x*x);
    return 0.5 * (1.0 - sign(distance) * erf);
}

float drop_shadow(vec2 p, vec2 half_size, vec2 offset, float spread, float sigma) {
    return gaussian_edge(
        rounded_distance(p - offset * zoom, max(half_size + spread * zoom, vec2(0.0)),
            max(radius + spread * zoom, 0.0)),
        sigma * zoom
    );
}

vec4 header_material(vec2 position, float inner) {
    // Same layer order as masonry_titlebar's former full-width raster:
    // warm translucent surface, elliptical orange wash, control tint, details.
    vec4 color = premultiply(vec3(246.0, 244.0, 239.0) / 255.0,
        photo > 0.5 ? 0.76 : 0.54);
    float radial = length(position / max(window_size * 1.41421356237, vec2(1.0)));
    float wash = 0.05 * clamp(1.0 - radial / 0.4, 0.0, 1.0);
    color = over(premultiply(vec3(233.0, 100.0, 42.0) / 255.0, wash), color);
    color = over(premultiply(vec3(1.0), 41.0 / 255.0), color);

    // tex_matrix maps physical destination pixels into the unscaled shader
    // coordinates. Keep a one-physical-pixel AA edge at every output/zoom scale.
    float pixel_size = max(length(tex_matrix[0].xy * area_size),
        length(tex_matrix[1].xy * area_size));
    float aa = max(pixel_size * 0.5, 0.0001);
    float divider = smoothstep(43.0 * zoom - aa, 43.0 * zoom + aa, position.y);
    color = over(premultiply(vec3(65.0, 87.0, 84.0) / 255.0,
        divider * 28.0 / 255.0), color);
    float grip_distance = rounded_distance(
        position - vec2(window_size.x * 0.5, 22.0 * zoom),
        vec2(14.0, 2.0) * zoom, 2.0 * zoom);
    float grip = 1.0 - smoothstep(-aa, aa, grip_distance);
    vec3 muted = (photo > 0.5 ? vec3(88.0, 95.0, 96.0) : vec3(102.0, 115.0, 115.0)) / 255.0;
    color = over(premultiply(muted, grip * 0.26), color);
    return color * (1.0 - smoothstep(-aa, aa, inner));
}

void main() {
    vec2 half_size = window_size * 0.5;
    vec2 p = v_coords * area_size - window_offset - half_size;
    float inner = rounded_distance(p, half_size, radius);
    vec4 color;
    if (mode > 1.5) {
        color = header_material(p + half_size, inner);
    } else if (mode > 0.5) {
        // A translucent body must not see its own dark shadow.
        if (inner <= 0.0) discard;
        vec3 orange = vec3(0.91372549, 0.39215686, 0.16470588);
        vec3 dark = vec3(0.20784314, 0.30588235, 0.23137255);
        if (active > 0.5) {
            float broad = drop_shadow(p, half_size, vec2(0.0, 28.0), -18.0, 44.0);
            float near = drop_shadow(p, half_size, vec2(0.0, 8.0), -12.0, 16.0);
            color = over(premultiply(orange, broad * 0.18),
                premultiply(dark, near * 0.18823529));
        } else if (hovered > 0.5) {
            // --shadow-panel: the inactive window lifts without gaining the
            // active orange border. CSS lists the uppermost shadow first.
            float broad = drop_shadow(p, half_size, vec2(0.0, 24.0), -22.0, 36.0);
            float near = drop_shadow(p, half_size, vec2(0.0, 6.0), -8.0, 12.0);
            color = over(premultiply(dark, broad * 0.16862745),
                premultiply(orange, near * 0.16));
        } else {
            float broad = drop_shadow(p, half_size, vec2(0.0, 24.0), -20.0, 32.0);
            float near = drop_shadow(p, half_size, vec2(0.0, 6.0), -8.0, 10.0);
            color = over(premultiply(dark, broad * 0.14117647),
                premultiply(orange, near * 0.15));
        }
        color *= clamp(inner / max(zoom, 0.01), 0.0, 1.0);
    } else {
        // Match the browser's one-pixel border outside the existing content.
        float outer = rounded_distance(p, half_size + vec2(zoom), radius + zoom);
        float aa = max(0.5 * zoom, 0.25);
        float border = (1.0 - smoothstep(-aa, aa, outer)) * smoothstep(-aa, aa, inner);
        color = active > 0.5
            ? premultiply(vec3(0.954409, 0.678793, 0.5586), border * 0.681255)
            : premultiply(vec3(1.0), border * 0.43921569);
        // The inset highlight sits above the client, within the rounded edge.
        float inside = (1.0 - smoothstep(-aa, aa, inner));
        float line = smoothstep(-zoom - aa, -zoom + aa, inner) * inside;
        float top = 1.0 - smoothstep(zoom, 2.0 * zoom, p.y + half_size.y);
        bool raised = active > 0.5 || hovered > 0.5;
        float edge_alpha = max(raised ? 0.18823529 : 0.0,
            top * (raised ? 0.78039216 : 0.52156863));
        // The owned rounded frame also has the reference's very faint inset
        // orange wash. External client decorations retain their own content.
        if (active > 0.5 && radius > 0.0) {
            float glow = gaussian_edge(-inner + 14.0 * zoom, 18.0 * zoom) * 0.05 * inside;
            color = over(premultiply(vec3(0.91372549, 0.39215686, 0.16470588), glow), color);
        }
        color = over(premultiply(vec3(1.0), line * edge_alpha), color);
    }
#if defined(DEBUG_FLAGS)
    if (tint == 1.0) color = vec4(0.0, 0.2, 0.0, 0.2) + color * 0.8;
#endif
    gl_FragColor = color * alpha;
}
