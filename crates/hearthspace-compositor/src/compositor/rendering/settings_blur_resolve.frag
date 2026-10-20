#version 100

//_DEFINES_
#if defined(EXTERNAL)
#extension GL_OES_EGL_image_external : require
#endif
precision highp float;
#if defined(EXTERNAL)
uniform samplerExternalOES tex;
#else
uniform sampler2D tex;
#endif
uniform float alpha;
uniform vec2 source_size;
uniform float sample_scale;
uniform vec2 window_size;
uniform vec2 window_offset;
uniform float radius;
uniform float saturation;
varying vec2 v_coords;
#if defined(DEBUG_FLAGS)
uniform float tint;
#endif

void main() {
    vec2 half_texel = 0.5 / source_size;
    vec4 color = texture2D(tex, clamp(v_coords, half_texel, vec2(1.0) - half_texel));
    // The reference uses less saturation with a photographic wallpaper.
    float luminance = dot(color.rgb, vec3(0.213, 0.715, 0.072));
    color.rgb = clamp(mix(vec3(luminance), color.rgb, saturation), 0.0, 1.0);
    vec2 position = v_coords * source_size * sample_scale + window_offset;
    vec2 half_size = window_size * 0.5;
    float r = min(radius, min(half_size.x, half_size.y));
    vec2 q = abs(position - half_size) - (half_size - vec2(r));
    float distance = length(max(q, vec2(0.0))) + min(max(q.x, q.y), 0.0) - r;
    color *= 1.0 - smoothstep(-0.5, 0.5, distance);
    color *= alpha;
#if defined(DEBUG_FLAGS)
    if (tint == 1.0) color = vec4(0.0, 0.2, 0.0, 0.2) * color.a + color * 0.8;
#endif
    gl_FragColor = color;
}
