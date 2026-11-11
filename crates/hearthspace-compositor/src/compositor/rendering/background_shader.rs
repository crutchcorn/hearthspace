//! One opaque shader path for the built-in dots and user GLSL backgrounds.
use super::GlesTexture;
use crate::{compositor::App, geometry::CanvasPoint};
use hearthspace_ipc::ColorMode;
use smithay::{
    backend::renderer::{
        ContextId, Renderer,
        element::Kind,
        gles::{
            GlesPixelProgram, GlesRenderer, Uniform, UniformName, UniformType,
            element::PixelShaderElement,
        },
    },
    utils::{Logical, Rectangle},
};
use std::{
    collections::HashMap,
    fs::File,
    io::Read,
    time::{Duration, Instant},
};

const DOTS: &str = include_str!("dots.glsl");
const FRAME_INTERVAL: Duration = Duration::from_millis(33);
const MAX_SOURCE_BYTES: u64 = 1024 * 1024;

type ViewKey = (i32, i32, i32);
struct CachedElement {
    element: PixelShaderElement,
    parameters: Vec<u32>,
}

pub(in crate::compositor) struct BackgroundShader {
    started: Instant,
    context: Option<ContextId<GlesTexture>>,
    program: Option<GlesPixelProgram>,
    views: HashMap<ViewKey, CachedElement>,
    animated: bool,
    next_frame: Instant,
    pub error: Option<String>,
}

impl Default for BackgroundShader {
    fn default() -> Self {
        Self {
            started: Instant::now(),
            context: None,
            program: None,
            views: HashMap::new(),
            animated: false,
            next_frame: Instant::now(),
            error: None,
        }
    }
}

impl BackgroundShader {
    pub fn reset(&mut self) {
        self.context = None;
        self.program = None;
        self.views.clear();
        self.animated = false;
        self.error = None;
    }

    pub fn status(&self, path: &Option<String>) -> serde_json::Value {
        serde_json::json!({"path": path, "error": self.error, "ready": self.program.is_some(), "animated": self.animated})
    }

    pub fn invalidate_views(&mut self) {
        self.views.clear();
    }

    fn prepare(&mut self, renderer: &mut GlesRenderer, path: Option<&str>) {
        let context = renderer.context_id();
        if self.context.as_ref() == Some(&context) {
            return;
        }
        self.reset();
        self.context = Some(context);
        let source = path.map(load_source).transpose();
        let result = source.and_then(|source| {
            let source = source.as_deref().unwrap_or(DOTS);
            self.animated = path.is_some() && uses_time(source);
            compile(renderer, source).map_err(|error| error.to_string())
        });
        match result {
            Ok(program) => self.program = Some(program),
            Err(error) => {
                self.animated = false;
                self.error = Some(error.chars().take(2048).collect());
                tracing::warn!(path, error = ?self.error, "GLSL background rejected; using canvas dots");
                self.program = compile(renderer, DOTS).ok();
            }
        }
    }

    fn element(
        &mut self,
        renderer: &mut GlesRenderer,
        path: Option<&str>,
        output: Rectangle<i32, Logical>,
        density: i32,
        camera: CanvasPoint,
        zoom: f64,
        mode: ColorMode,
    ) -> Option<PixelShaderElement> {
        if !zoom.is_finite() || zoom <= 0.0 {
            return None;
        }
        self.prepare(renderer, path);
        let program = self.program.as_ref()?.clone();
        let density = density.max(1);
        let area = Rectangle::new(
            output.loc,
            (output.size.w * density, output.size.h * density).into(),
        );
        let size = (
            output.size.w as f32 / zoom as f32,
            output.size.h as f32 / zoom as f32,
        );
        let origin = (
            camera.x as f64 + f64::from(output.loc.x) / zoom,
            camera.y as f64 + f64::from(output.loc.y) / zoom,
        );
        let time = if self.animated {
            (self.started.elapsed().as_secs_f64() * 30.0).floor() as f32 / 30.0
        } else {
            0.0
        };
        let (background, dot) = match mode {
            ColorMode::Light => ([229., 233., 227.], [208., 215., 209.]),
            ColorMode::Dark => ([20., 34., 37.], [41., 57., 58.]),
        };
        let values = vec![
            size.0,
            size.1,
            origin.0 as f32,
            origin.1 as f32,
            origin.0.rem_euclid(28.) as f32,
            origin.1.rem_euclid(28.) as f32,
            zoom as f32,
            density as f32,
            time,
            background[0],
            background[1],
            background[2],
            dot[0],
            dot[1],
            dot[2],
        ];
        let parameters = values.iter().map(|value| value.to_bits()).collect();
        let uniforms = vec![
            Uniform::new("size", size),
            Uniform::new("u_camera", (origin.0 as f32, origin.1 as f32)),
            Uniform::new(
                "u_grid_origin",
                (
                    origin.0.rem_euclid(28.) as f32,
                    origin.1.rem_euclid(28.) as f32,
                ),
            ),
            Uniform::new("u_zoom", zoom as f32),
            Uniform::new("u_scale", density as f32),
            Uniform::new("u_time", time),
            Uniform::new(
                "u_background_color",
                (
                    background[0] / 255.,
                    background[1] / 255.,
                    background[2] / 255.,
                ),
            ),
            Uniform::new("u_dot_color", (dot[0] / 255., dot[1] / 255., dot[2] / 255.)),
        ];
        let cached = self
            .views
            .entry((output.loc.x, output.loc.y, density))
            .or_insert_with(|| CachedElement {
                element: PixelShaderElement::new(
                    program,
                    area,
                    Some(vec![Rectangle::from_size(area.size)]),
                    1.,
                    uniforms.clone(),
                    Kind::Unspecified,
                ),
                parameters: Vec::new(),
            });
        cached
            .element
            .resize(area, Some(vec![Rectangle::from_size(area.size)]));
        if cached.parameters != parameters {
            cached.element.update_uniforms(uniforms);
            cached.parameters = parameters;
        }
        Some(cached.element.clone())
    }
}

fn compile(
    renderer: &mut GlesRenderer,
    source: &str,
) -> Result<GlesPixelProgram, smithay::backend::renderer::gles::GlesError> {
    // Backgrounds are always opaque, including shaders that output alpha zero.
    let source = format!(
        "#define main hearthspace_wallpaper_main\n{source}\n#undef main\nvoid main() {{ hearthspace_wallpaper_main(); gl_FragColor.a = 1.0; }}\n"
    );
    renderer.compile_custom_pixel_shader(
        source,
        &[
            UniformName::new("size", UniformType::_2f),
            UniformName::new("u_camera", UniformType::_2f),
            UniformName::new("u_grid_origin", UniformType::_2f),
            UniformName::new("u_zoom", UniformType::_1f),
            UniformName::new("u_scale", UniformType::_1f),
            UniformName::new("u_time", UniformType::_1f),
            UniformName::new("u_background_color", UniformType::_3f),
            UniformName::new("u_dot_color", UniformType::_3f),
        ],
    )
}

fn load_source(path: &str) -> Result<String, String> {
    let metadata = std::fs::metadata(path).map_err(|error| error.to_string())?;
    if !metadata.is_file() {
        return Err("GLSL source must be a regular file".into());
    }
    let mut source = String::new();
    File::open(path)
        .and_then(|file| file.take(MAX_SOURCE_BYTES + 1).read_to_string(&mut source))
        .map_err(|error| error.to_string())?;
    if source.len() as u64 > MAX_SOURCE_BYTES {
        return Err("GLSL file exceeds 1 MiB".into());
    }
    if source.contains('\0') {
        return Err("GLSL source contains a NUL character".into());
    }
    Ok(source)
}

// Comment text must not turn an otherwise static wallpaper into an animation.
fn uses_time(source: &str) -> bool {
    let mut text = String::new();
    let mut chars = source.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '/' && chars.peek() == Some(&'/') {
            for ch in chars.by_ref() {
                if ch == '\n' {
                    break;
                }
            }
            text.push(' ');
        } else if ch == '/' && chars.peek() == Some(&'*') {
            chars.next();
            while let Some(ch) = chars.next() {
                if ch == '*' && chars.peek() == Some(&'/') {
                    chars.next();
                    break;
                }
            }
            text.push(' ');
        } else {
            text.push(ch);
        }
    }
    text.split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
        .filter(|word| *word == "u_time")
        .count()
        > 1
}

impl App {
    pub(super) fn background_shader_element(
        &mut self,
        renderer: &mut GlesRenderer,
        output: Rectangle<i32, Logical>,
        density: i32,
    ) -> Option<PixelShaderElement> {
        self.background_shader.element(
            renderer,
            self.background_shader_path.as_deref(),
            output,
            density,
            self.viewport_offset,
            self.viewport_scale,
            self.appearance.color_mode,
        )
    }

    pub(in crate::compositor) fn background_animation_timeout(&self) -> Option<Duration> {
        if !self.background_shader.animated
            || self.session_locked()
            || (!self.canvas_preview_presenting()
                && self
                    .output_logical_rects()
                    .iter()
                    .all(|rect| self.fullscreen_on_output(*rect)))
        {
            return None;
        }
        Some(
            self.background_shader
                .next_frame
                .saturating_duration_since(Instant::now()),
        )
    }

    pub(in crate::compositor) fn advance_background_animation(&mut self) {
        if self.background_animation_timeout() == Some(Duration::ZERO) {
            self.background_shader.next_frame = Instant::now() + FRAME_INTERVAL;
            self.request_redraw();
        }
    }
}

#[cfg(test)]
#[path = "background_shader_tests.rs"]
mod tests;
