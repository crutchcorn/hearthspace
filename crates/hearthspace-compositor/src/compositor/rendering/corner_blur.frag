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
uniform vec2 step_uv;
uniform vec2 texture_size;
uniform vec2 corner_size;
uniform vec2 crop_origin;
uniform float radius;
uniform float rounded;
uniform vec2 sample_min;
uniform vec2 sample_max;
varying vec2 v_coords;

#if defined(DEBUG_FLAGS)
uniform float tint;
#endif

vec4 sample_clamped(vec2 uv) {
    return texture2D(tex, clamp(uv, sample_min, sample_max));
}

void main() {
    // The generated Gaussian covers every physical pixel with bilinear pairs.
    //_KERNEL_

#if defined(NO_ALPHA)
    color.a = 1.0;
#endif

    if (rounded > 0.5) {
        vec2 half_size = corner_size * 0.5;
        float r = min(radius, min(half_size.x, half_size.y));
        vec2 position = v_coords * texture_size - crop_origin;
        vec2 q = abs(position - half_size) - (half_size - vec2(r));
        float distance = length(max(q, vec2(0.0))) + min(max(q.x, q.y), 0.0) - r;
        // A one-physical-pixel transition keeps curved edges antialiased at any DPI.
        color *= 1.0 - smoothstep(-0.5, 0.5, distance);
    }
    color *= alpha;

#if defined(DEBUG_FLAGS)
    if (tint == 1.0)
        color = vec4(0.0, 0.2, 0.0, 0.2) * color.a + color * 0.8;
#endif

    gl_FragColor = color;
}
