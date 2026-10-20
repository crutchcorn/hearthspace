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
uniform vec4 chrome_color;
uniform vec4 wash_color;
uniform vec4 control_color;
uniform vec4 line_color;
uniform vec4 muted_color;
uniform vec4 border_color;
uniform vec4 edge_color;
uniform vec4 edge_all_color;
uniform vec4 broad_shadow_color;
uniform vec4 near_shadow_color;
uniform vec4 inset_color;
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
    vec4 color = premultiply(chrome_color.rgb, chrome_color.a);
    float radial = length(position / max(window_size * 1.41421356237, vec2(1.0)));
    float wash = wash_color.a * clamp(1.0 - radial / 0.4, 0.0, 1.0);
    color = over(premultiply(wash_color.rgb, wash), color);
    color = over(premultiply(control_color.rgb, control_color.a), color);

    // tex_matrix maps physical destination pixels into the unscaled shader
    // coordinates. Keep a one-physical-pixel AA edge at every output/zoom scale.
    float pixel_size = max(length(tex_matrix[0].xy * area_size),
        length(tex_matrix[1].xy * area_size));
    float aa = max(pixel_size * 0.5, 0.0001);
    float divider = smoothstep(43.0 * zoom - aa, 43.0 * zoom + aa, position.y);
    color = over(premultiply(line_color.rgb, divider * line_color.a), color);
    float grip_distance = rounded_distance(
        position - vec2(window_size.x * 0.5, 22.0 * zoom),
        vec2(14.0, 2.0) * zoom, 2.0 * zoom);
    float grip = 1.0 - smoothstep(-aa, aa, grip_distance);
    color = over(premultiply(muted_color.rgb, grip * 0.26), color);
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
        if (active > 0.5) {
            float broad = drop_shadow(p, half_size, vec2(0.0, 28.0), -18.0, 44.0);
            float near = drop_shadow(p, half_size, vec2(0.0, 8.0), -12.0, 16.0);
            color = over(premultiply(broad_shadow_color.rgb, broad * broad_shadow_color.a),
                premultiply(near_shadow_color.rgb, near * near_shadow_color.a));
        } else if (hovered > 0.5) {
            // --shadow-panel: the inactive window lifts without gaining the
            // active orange border. CSS lists the uppermost shadow first.
            float broad = drop_shadow(p, half_size, vec2(0.0, 24.0), -22.0, 36.0);
            float near = drop_shadow(p, half_size, vec2(0.0, 6.0), -8.0, 12.0);
            color = over(premultiply(broad_shadow_color.rgb, broad * broad_shadow_color.a),
                premultiply(near_shadow_color.rgb, near * near_shadow_color.a));
        } else {
            float broad = drop_shadow(p, half_size, vec2(0.0, 24.0), -20.0, 32.0);
            float near = drop_shadow(p, half_size, vec2(0.0, 6.0), -8.0, 10.0);
            color = over(premultiply(broad_shadow_color.rgb, broad * broad_shadow_color.a),
                premultiply(near_shadow_color.rgb, near * near_shadow_color.a));
        }
        color *= clamp(inner / max(zoom, 0.01), 0.0, 1.0);
    } else {
        // Match the browser's one-pixel border outside the existing content.
        float outer = rounded_distance(p, half_size + vec2(zoom), radius + zoom);
        float aa = max(0.5 * zoom, 0.25);
        float border = (1.0 - smoothstep(-aa, aa, outer)) * smoothstep(-aa, aa, inner);
        color = premultiply(border_color.rgb, border * border_color.a);
        // The inset highlight sits above the client, within the rounded edge.
        float inside = (1.0 - smoothstep(-aa, aa, inner));
        float line = smoothstep(-zoom - aa, -zoom + aa, inner) * inside;
        float top = 1.0 - smoothstep(zoom, 2.0 * zoom, p.y + half_size.y);
        float edge_alpha = max(edge_all_color.a, top * edge_color.a);
        // The owned rounded frame also has the reference's very faint inset
        // orange wash. External client decorations retain their own content.
        if (active > 0.5 && radius > 0.0) {
            float glow = gaussian_edge(-inner + 14.0 * zoom, 18.0 * zoom) * inset_color.a * inside;
            color = over(premultiply(inset_color.rgb, glow), color);
        }
        color = over(premultiply(edge_color.rgb, line * edge_alpha), color);
    }
#if defined(DEBUG_FLAGS)
    if (tint == 1.0) color = vec4(0.0, 0.2, 0.0, 0.2) + color * 0.8;
#endif
    gl_FragColor = color * alpha;
}
