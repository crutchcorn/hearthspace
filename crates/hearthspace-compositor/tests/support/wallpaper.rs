//! Saved GLSL selection, live reload, and fallback through the real desktop.
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

fn assert_color(compositor: &HeadlessCompositor, expected: [u8; 4]) {
    let mut reader = png::Decoder::new(std::io::Cursor::new(compositor.take_screenshot()))
        .read_info()
        .unwrap();
    let mut bytes = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut bytes).unwrap();
    assert_eq!(info.color_type, png::ColorType::Rgba);
    // Away from the pointer and any output-edge decorations.
    let index = ((80 * info.width + 80) * 4) as usize;
    assert_eq!(&bytes[index..index + 4], &expected);
}

#[test]
fn saved_glsl_renders_and_same_path_reload_recovers_from_errors_without_restart() {
    let _guard = headless_test_guard();
    let directory = tempfile::tempdir().unwrap();
    let socket = directory.path().join("settings.sock");
    let file = directory.path().join("wallpaper.glsl");
    let source = |color: &str| {
        format!("precision mediump float; void main() {{gl_FragColor=vec4({color},1.);}}")
    };
    std::fs::write(&file, source("1.,0.,0.")).unwrap();
    let binary =
        Path::new(env!("CARGO_BIN_EXE_hearthspace")).with_file_name("hearthspace-settingsd");
    let log = std::fs::File::create(directory.path().join("settingsd.log")).unwrap();
    let mut daemon = Daemon(
        Command::new(binary)
            .env(hearthspace_common::SETTINGS_SOCKET_ENV, &socket)
            .env("XDG_CONFIG_HOME", directory.path().join("config"))
            .env("XDG_RUNTIME_DIR", directory.path())
            .stdin(Stdio::null())
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .spawn()
            .unwrap(),
    );
    drop(startup::wait_for_socket(&mut daemon.0, &socket).unwrap());
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

    std::fs::write(&file, "invalid GLSL").unwrap();
    action(&compositor, "reload-settings");
    // A real capture waits for the newly rendered frame, rather than accepting
    // the reload acknowledgment as evidence of completed compilation.
    assert_color(&compositor, [229, 233, 227, 255]);
    assert!(query(&compositor, "status")["background_shader"]["error"].is_string());
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
