//! Parallels Apple virgl needs complete native scene repaints.
use smithay::backend::renderer::gles::{GlesError, GlesRenderer, ffi};
use std::ffi::CStr;

struct RetainedContents(bool);

pub(super) fn scene_age(
    renderer: &mut GlesRenderer,
    age: usize,
    force: bool,
) -> Result<usize, GlesError> {
    let cached = renderer
        .egl_context()
        .user_data()
        .get::<RetainedContents>()
        .map(|policy| policy.0);
    let retained = if let Some(retained) = cached {
        retained
    } else {
        let name = renderer.with_context(|gl| {
            // SAFETY: with_context owns the current context; GL_RENDERER is
            // a NUL-terminated driver string valid for this context's lifetime.
            let name = unsafe { gl.GetString(ffi::RENDERER) };
            if name.is_null() {
                String::new()
            } else {
                unsafe { CStr::from_ptr(name.cast()) }
                    .to_string_lossy()
                    .into_owned()
            }
        })?;
        let retained = supports_retained_contents(&name);
        tracing::info!(
            renderer = name,
            retained_scene_contents = retained,
            "selected native scene repaint policy"
        );
        renderer
            .egl_context()
            .user_data()
            .insert_if_missing_threadsafe(|| RetainedContents(retained));
        retained
    };
    Ok(if force || !retained { 0 } else { age })
}

fn supports_retained_contents(renderer: &str) -> bool {
    let name = renderer.to_ascii_lowercase();
    !(name.starts_with("virgl (") && name.contains("apple "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn virgl_repaints_without_disabling_other_renderers_damage() {
        assert!(!supports_retained_contents("virgl (Apple M4 Max (Compat))"));
        assert!(supports_retained_contents("virgl (AMD Radeon)"));
        assert!(supports_retained_contents("AMD Radeon 890M"));
        assert!(supports_retained_contents("NVIDIA GeForce RTX 3070 Ti"));
        assert!(supports_retained_contents("llvmpipe (LLVM 21.1.0)"));
    }
}
