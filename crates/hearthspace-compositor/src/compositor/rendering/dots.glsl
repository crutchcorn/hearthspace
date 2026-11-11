// Hearthspace's canvas grid. Coordinates and spacing are logical canvas pixels.
precision highp float;
varying vec2 v_coords;
uniform vec2 size;
uniform vec2 u_grid_origin;
uniform vec3 u_background_color;
uniform vec3 u_dot_color;

void main() {
    // The host reduces the camera modulo the grid period before converting to
    // float, retaining the dot phase even far from the canvas origin.
    vec2 cell = mod(v_coords * size + u_grid_origin - vec2(14.0), 28.0);
    float dot = (1.0 - step(1.0, cell.x)) * (1.0 - step(1.0, cell.y));
    gl_FragColor = vec4(mix(u_background_color, u_dot_color, dot), 1.0);
}
