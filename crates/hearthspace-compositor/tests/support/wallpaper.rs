//! Saved image/shader wallpapers, live reload, and fallback through the real desktop.
use super::navigation::{action, query};
use super::*;
use hearthspace_ipc::transport::request_line;

struct Daemon(Child);
impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn start_daemon(directory: &Path) -> (Daemon, PathBuf) {
    let socket = directory.join("settings.sock");
    let binary =
        Path::new(env!("CARGO_BIN_EXE_hearthspace")).with_file_name("hearthspace-settingsd");
    let log = std::fs::File::options()
        .create(true)
        .append(true)
        .open(directory.join("settingsd.log"))
        .unwrap();
    let mut daemon = Daemon(
        Command::new(binary)
            .env(hearthspace_common::SETTINGS_SOCKET_ENV, &socket)
            .env("XDG_CONFIG_HOME", directory.join("config"))
            .env("XDG_RUNTIME_DIR", directory)
            .stdin(Stdio::null())
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .spawn()
            .unwrap(),
    );
    drop(startup::wait_for_socket(&mut daemon.0, &socket).unwrap());
    (daemon, socket)
}

fn captured_color(compositor: &HeadlessCompositor) -> [u8; 4] {
    let mut reader = png::Decoder::new(std::io::Cursor::new(compositor.take_screenshot()))
        .read_info()
        .unwrap();
    let mut bytes = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut bytes).unwrap();
    assert_eq!(info.color_type, png::ColorType::Rgba);
    // Away from the pointer and any output-edge decorations.
    let index = ((80 * info.width + 80) * 4) as usize;
    bytes[index..index + 4].try_into().unwrap()
}

fn assert_color(compositor: &HeadlessCompositor, expected: [u8; 4]) {
    assert_eq!(captured_color(compositor), expected);
}

#[test]
fn saved_glsl_renders_and_same_path_reload_recovers_from_errors_without_restart() {
    let _guard = headless_test_guard();
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("wallpaper.glsl");
    let source = |color: &str| {
        format!("precision mediump float; void main() {{gl_FragColor=vec4({color},1.);}}")
    };
    std::fs::write(&file, source("1.,0.,0.")).unwrap();
    let (_daemon, socket) = start_daemon(directory.path());
    let reply = request_line(
        &socket,
        &format!("set background-shader-path {}\n", file.display()),
        Duration::from_secs(2),
    )
    .unwrap();
    assert!(reply.starts_with("ok "), "{reply}");
    assert_eq!(
        hearthspace_ipc::client_get_settings_at(&socket)
            .unwrap()
            .background_shader_path
            .as_deref(),
        file.to_str()
    );
    let mut compositor =
        HeadlessCompositor::spawn_with_command("320x240", None, None, 1, false, |command| {
            command.env(hearthspace_common::SETTINGS_SOCKET_ENV, &socket);
        });
    drop(compositor.wait_for_socket());
    assert_color(&compositor, [255, 0, 0, 255]);
    let active = query(&compositor, "status")["background_shader"].clone();
    assert_eq!(active["path"].as_str(), file.to_str());
    assert_eq!(active["ready"], true);
    assert!(active["error"].is_null());
    assert_eq!(active["animated"], false);

    let invalid = directory.path().join("invalid.glsl");
    std::fs::write(&invalid, "invalid GLSL").unwrap();
    let rejected = compositor.send_text_command(&format!(
        "wallpaper-preview {}",
        serde_json::to_string(&invalid).unwrap()
    ));
    assert!(rejected.starts_with("err Wallpaper failed to load"));
    assert_color(&compositor, [255, 0, 0, 255]);
    assert_eq!(
        hearthspace_ipc::client_get_settings_at(&socket)
            .unwrap()
            .background_shader_path
            .as_deref(),
        file.to_str()
    );
    let candidate = directory.path().join("blue.glsl");
    std::fs::write(&candidate, source("0.,0.,1.")).unwrap();
    let preview = hearthspace_ipc::transport::request_payload(
        &compositor.command_socket_path,
        &format!(
            "wallpaper-preview {}",
            serde_json::to_string(&candidate).unwrap()
        ),
        Duration::from_secs(3),
        2 * 1024 * 1024,
    )
    .unwrap();
    let mut reader = png::Decoder::new(std::io::Cursor::new(preview))
        .read_info()
        .unwrap();
    let mut bytes = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut bytes).unwrap();
    assert_eq!((info.width, info.height), (336, 256));
    assert_eq!(&bytes[..4], &[0, 0, 255, 255]);
    assert_color(&compositor, [255, 0, 0, 255]);
    std::fs::write(&file, "invalid GLSL").unwrap();
    action(&compositor, "reload-settings");
    // A real capture waits for the newly rendered frame, rather than accepting
    // the reload acknowledgment as evidence of completed compilation.
    assert_color(&compositor, [255, 0, 0, 255]);
    assert_eq!(
        query(&compositor, "status")["background_shader"]["error"],
        "Wallpaper failed to load"
    );
    assert_eq!(
        hearthspace_ipc::client_get_settings_at(&socket)
            .unwrap()
            .background_shader_path
            .as_deref(),
        file.to_str()
    );

    std::fs::write(&file, source("0.,0.,1.")).unwrap();
    action(&compositor, "reload-settings");
    assert_color(&compositor, [0, 0, 255, 255]);
    assert!(query(&compositor, "status")["background_shader"]["error"].is_null());
    std::fs::write(&file, "precision mediump float; uniform float u_time; void main(){gl_FragColor=vec4(mod(u_time,1.),0.,1.,1.);}").unwrap();
    action(&compositor, "reload-settings");
    drop(compositor.take_screenshot());
    assert_eq!(
        query(&compositor, "status")["background_shader"]["animated"],
        true
    );
    action(&compositor, "canvas-search-motion false");
    assert_color(&compositor, [0, 0, 255, 255]);
    assert_eq!(
        query(&compositor, "status")["background_shader"]["animated"],
        false
    );
    action(&compositor, "pan-to 20 30");
    assert_color(&compositor, [0, 0, 255, 255]);
    action(&compositor, "canvas-search-motion true");
    drop(compositor.take_screenshot());
    assert_eq!(
        query(&compositor, "status")["background_shader"]["animated"],
        true
    );
    action(&compositor, "pan-to 0 0");
    let reply = request_line(
        &socket,
        "set background-image-path default\n",
        Duration::from_secs(2),
    )
    .unwrap();
    assert!(reply.starts_with("ok "));
    action(&compositor, "reload-settings");
    assert_color(&compositor, [229, 233, 227, 255]);
    assert!(query(&compositor, "status")["background_shader"]["path"].is_null());
}

fn assert_jpeg_color(compositor: &HeadlessCompositor, expected: [u8; 3]) {
    let pixel = captured_color(compositor);
    for (actual, expected) in pixel[..3].iter().zip(expected) {
        assert!(actual.abs_diff(expected) <= 3, "JPEG wallpaper: {pixel:?}");
    }
    assert_eq!(pixel[3], 255);
}

#[test]
fn saved_jpg_and_jpeg_wallpapers_render_reload_and_survive_session_restart() {
    let _guard = headless_test_guard();
    let directory = tempfile::tempdir().unwrap();
    let jpg = directory.path().join("wallpaper.jpg");
    let jpeg = directory.path().join("wallpaper.jpeg");
    let first_color = [30, 100, 200];
    let second_color = [200, 70, 30];
    for (file, color) in [(&jpg, first_color), (&jpeg, second_color)] {
        let image = image::RgbImage::from_pixel(160, 120, image::Rgb(color));
        image::codecs::jpeg::JpegEncoder::new_with_quality(
            std::fs::File::create(file).unwrap(),
            100,
        )
        .encode_image(&image)
        .unwrap();
    }
    let (daemon, socket) = start_daemon(directory.path());
    let reply = request_line(
        &socket,
        &format!("set background-image-path {}\n", jpg.display()),
        Duration::from_secs(2),
    )
    .unwrap();
    assert!(reply.starts_with("ok "), "{reply}");
    let mut compositor =
        HeadlessCompositor::spawn_with_command("320x240", None, None, 1, false, |command| {
            command.env(hearthspace_common::SETTINGS_SOCKET_ENV, &socket);
        });
    drop(compositor.wait_for_socket());
    assert_jpeg_color(&compositor, first_color);
    for mode in ["cover", "contains", "stretch", "repeat-with-canvas"] {
        let reply = request_line(
            &socket,
            &format!("set background-mode {mode}\n"),
            Duration::from_secs(2),
        )
        .unwrap();
        assert!(reply.starts_with("ok "), "{reply}");
        action(&compositor, "reload-settings");
        assert_jpeg_color(&compositor, first_color);
    }
    let reply = request_line(
        &socket,
        &format!("set background-image-path {}\n", jpeg.display()),
        Duration::from_secs(2),
    )
    .unwrap();
    assert!(reply.starts_with("ok "), "{reply}");
    action(&compositor, "reload-settings");
    assert_jpeg_color(&compositor, second_color);
    drop(compositor);
    drop(daemon);

    let (_restarted_daemon, socket) = start_daemon(directory.path());
    assert_eq!(
        hearthspace_ipc::client_get_settings_at(&socket)
            .unwrap()
            .background_image_path
            .as_deref(),
        jpeg.to_str(),
    );
    let mut restarted =
        HeadlessCompositor::spawn_with_command("320x240", None, None, 1, false, |command| {
            command.env(hearthspace_common::SETTINGS_SOCKET_ENV, &socket);
        });
    drop(restarted.wait_for_socket());
    assert_jpeg_color(&restarted, second_color);
}
