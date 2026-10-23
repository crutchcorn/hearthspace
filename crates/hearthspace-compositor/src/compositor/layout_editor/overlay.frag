precision highp float;
varying vec2 v_coords;
uniform float alpha;
uniform vec2 area_size;
uniform vec4 shape_rect;
uniform vec4 ink;
uniform vec4 shape; // radius, stroke width, fill coverage, dash length
uniform mat3 tex_matrix;
#if defined(DEBUG_FLAGS)
uniform float tint;
#endif

const float PI = 3.14159265359;

float rounded_distance(vec2 point, vec2 half_size, float radius) {
    vec2 q = abs(point) - half_size + radius;
    return length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - radius;
}

// Distance clockwise around a rounded rectangle. Keeping the dash phase continuous
// through all four arcs avoids diagonal stripes or gaps at the selection's corners.
float perimeter_distance(vec2 p, vec2 size, float r) {
    float horizontal = max(0.0, size.x - 2.0*r);
    float vertical = max(0.0, size.y - 2.0*r);
    if (p.y < r) {
        if (p.x > size.x-r) return horizontal + r*(atan(p.y-r,p.x-size.x+r)+PI*0.5);
        if (p.x < r) return 2.0*horizontal+2.0*vertical+1.5*PI*r+r*(atan(p.y-r,p.x-r)+PI);
        return p.x-r;
    }
    if (p.y > size.y-r) {
        if (p.x > size.x-r) return horizontal+vertical+PI*r*0.5+r*atan(p.y-size.y+r,p.x-size.x+r);
        if (p.x < r) return 2.0*horizontal+vertical+PI*r+r*(atan(p.y-size.y+r,p.x-r)-PI*0.5);
        return horizontal+vertical+PI*r+size.x-r-p.x;
    }
    return p.x > size.x*0.5 ? horizontal+PI*r*0.5+p.y-r : 2.0*horizontal+vertical+1.5*PI*r+size.y-r-p.y;
}

void main() {
    vec2 local = v_coords*area_size-shape_rect.xy;
    vec2 half_size = shape_rect.zw*0.5;
    vec2 point = local-half_size;
    float radius = min(shape.x,min(half_size.x,half_size.y));
    float pixel_size = max(length(tex_matrix[0].xy*area_size),length(tex_matrix[1].xy*area_size));
    float aa = max(pixel_size*0.5,0.0001);
    float outer = 1.0-smoothstep(-aa,aa,rounded_distance(point,half_size,radius));
    float inner = min(half_size.x,half_size.y)<=shape.y ? 0.0 :
        1.0-smoothstep(-aa,aa,rounded_distance(point,half_size-shape.y,max(0.0,radius-shape.y)));
    float dash = 1.0;
    if (shape.w > 0.0) {
        float phase = mod(perimeter_distance(local,shape_rect.zw,radius),shape.w*2.0);
        dash = 1.0-smoothstep(shape.w-aa,shape.w+aa,phase);
    }
    float opacity = (outer*shape.z+max(outer-inner,0.0)*(1.0-shape.z)*dash)*ink.a*alpha;
    gl_FragColor = vec4(ink.rgb*opacity,opacity);
#if defined(DEBUG_FLAGS)
    if (tint == 1.0) gl_FragColor = vec4(0.0,0.2,0.0,0.2)+gl_FragColor*0.8;
#endif
}
