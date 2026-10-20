void main() {
    vec4 color = texture2D(tex, v_coords);
#if defined(NO_ALPHA)
    color.a = 1.0;
#endif
    color *= alpha * window_mask();
#if defined(DEBUG_FLAGS)
    if (tint == 1.0) color = vec4(0.0, 0.2, 0.0, 0.2) * color.a + color * 0.8;
#endif
    gl_FragColor = color;
}
