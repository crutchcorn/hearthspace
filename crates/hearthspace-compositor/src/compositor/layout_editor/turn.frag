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
uniform mat3 tex_matrix;
uniform vec3 turn_x;
uniform vec3 turn_y;
uniform vec2 turn_size;
uniform vec2 turn_aa;
varying vec2 v_coords;
#if defined(DEBUG_FLAGS)
uniform float tint;
#endif
void main(){
    vec3 pixel=vec3(gl_FragCoord.xy,1.0);
    vec2 local=vec2(dot(turn_x,pixel),dot(turn_y,pixel));
    vec2 edge=smoothstep(-turn_aa,turn_aa,local)*(1.0-smoothstep(1.0-turn_aa,1.0+turn_aa,local));
    vec2 sample_at=(tex_matrix*vec3(local*turn_size,1.0)).xy;
    vec4 color=texture2D(tex,sample_at);
#if defined(NO_ALPHA)
    color.a=1.0;
#endif
    color*=alpha*edge.x*edge.y;
#if defined(DEBUG_FLAGS)
    if(tint==1.0)color=vec4(0.0,0.2,0.0,0.2)*color.a+color*0.8;
#endif
    gl_FragColor=color;
}
