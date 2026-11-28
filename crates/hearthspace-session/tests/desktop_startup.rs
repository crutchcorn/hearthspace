//! A listening Wayland socket must not turn an unusable shell into a ready login.
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    net::Shutdown,
    os::unix::{fs::PermissionsExt, net::UnixStream},
    path::PathBuf,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

fn fixture(name: &str, shell: &str) -> PathBuf {
    let root =
        std::env::temp_dir().join(format!("hearthspace-desktop-{name}-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    for (name, script) in [
        (
            "hearthspace-settingsd",
            r#"import os,socket
with socket.socket(socket.AF_UNIX) as server:
 server.bind(os.environ['HEARTHSPACE_SETTINGS_SOCKET']); server.listen()
 while True:
  peer,_=server.accept()
  with peer:
   peer.recv(1024); peer.sendall(b'ok {}\n')
"#,
        ),
        (
            "hearthspace-compositor",
            r#"import os,socket,json
root=os.environ['HEARTHSPACE_HOST_XDG_RUNTIME_DIR']
wayland=socket.socket(socket.AF_UNIX)
wayland.bind(os.path.join(os.environ['XDG_RUNTIME_DIR'],os.environ['HEARTHSPACE_WAYLAND_DISPLAY']))
wayland.listen()
with socket.socket(socket.AF_UNIX) as server:
 server.bind(os.environ['HEARTHSPACE_COMMAND_SOCKET']); server.listen()
 while True:
  peer,_=server.accept()
  with peer:
   request=peer.makefile('rb').readline(1024)
   # A readiness deadline may expire after connect but before writing.
   # Closed probes must not erase evidence of an earlier valid status query.
   if not request: continue
   assert request == b'status\n', request
   open(root+'/status-probed','w').write(request.decode())
   payload=json.dumps({'backend':'nested','active':True,'shell_ready':False,'locked':False,'outputs':[]}).encode()
   try: peer.sendall(('ok %d\n'%len(payload)).encode()+payload)
   except (BrokenPipeError,ConnectionResetError): pass
"#,
        ),
        ("hearthspace-shell", shell),
    ] {
        let path = root.join(name);
        fs::write(&path, format!("#!/usr/bin/python3\n{script}")).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    }
    root
}

fn start(root: &PathBuf) -> Child {
    Command::new(env!("CARGO_BIN_EXE_hearthspace-session"))
        .args([
            "--installed",
            "--backend",
            "nested",
            "--no-desktop-services",
            "--no-watch-apps",
            "--startup-timeout",
            "1",
        ])
        .arg("--binary-dir")
        .arg(root)
        .arg("--session-dir")
        .arg(root.join("runtime"))
        .env("XDG_RUNTIME_DIR", root)
        .env("HEARTHSPACE_HOST_XDG_RUNTIME_DIR", root)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap()
}

fn failed_exit(child: &mut Child) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(!status.success());
            return;
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("unusable shell did not terminate login startup");
        }
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn unmapped_shell_expires_startup_and_cleans_owned_processes_and_runtime() {
    let root = fixture(
        "unmapped",
        r#"import os,time
open(os.environ['HEARTHSPACE_HOST_XDG_RUNTIME_DIR']+'/shell-pid','w').write(str(os.getpid()))
time.sleep(60)
"#,
    );
    let mut child = start(&root);
    failed_exit(&mut child);
    assert_eq!(
        fs::read_to_string(root.join("status-probed")).unwrap(),
        "status\n"
    );
    let pid = fs::read_to_string(root.join("shell-pid")).unwrap();
    assert!(
        !PathBuf::from(format!("/proc/{pid}")).exists(),
        "unmapped shell survived failed login"
    );
    assert!(!root.join("runtime").exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn shell_exit_before_mapping_fails_startup_instead_of_becoming_ready() {
    let root = fixture("exited", "import sys\nsys.exit(7)\n");
    let mut child = start(&root);
    failed_exit(&mut child);
    assert!(!root.join("runtime").exists());
    fs::remove_dir_all(root).unwrap();
}

struct FixtureChild(Child);

impl Drop for FixtureChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        self.0.wait().unwrap();
    }
}

fn status_reply(stream: &mut UnixStream) -> serde_json::Value {
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    stream.write_all(b"status\n").unwrap();
    let mut reader = BufReader::new(stream);
    let mut header = String::new();
    reader.read_line(&mut header).unwrap();
    let size = header.trim().strip_prefix("ok ").unwrap().parse().unwrap();
    let mut payload = vec![0; size];
    reader.read_exact(&mut payload).unwrap();
    serde_json::from_slice(&payload).unwrap()
}

#[test]
fn compositor_fixture_preserves_status_and_serves_requests_after_abandoned_probe() {
    let root = fixture("abandoned-probe", "");
    let socket = root.join("command.sock");
    let child = FixtureChild(
        Command::new(root.join("hearthspace-compositor"))
            .env("XDG_RUNTIME_DIR", &root)
            .env("HEARTHSPACE_HOST_XDG_RUNTIME_DIR", &root)
            .env("HEARTHSPACE_WAYLAND_DISPLAY", "wayland-test")
            .env("HEARTHSPACE_COMMAND_SOCKET", &socket)
            .stdout(Stdio::null())
            .stderr(fs::File::create(root.join("compositor.log")).unwrap())
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut initial = loop {
        match UnixStream::connect(&socket) {
            Ok(stream) => break stream,
            Err(error) => {
                assert!(Instant::now() < deadline, "fixture socket: {error}");
                thread::sleep(Duration::from_millis(5));
            }
        }
    };
    assert_eq!(status_reply(&mut initial)["shell_ready"], false);
    drop(initial);
    // Half-close to fence the empty request without sending another command
    // that could hide an accidentally overwritten status record.
    let mut abandoned = UnixStream::connect(&socket).unwrap();
    abandoned
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    abandoned.shutdown(Shutdown::Write).unwrap();
    let mut reply = Vec::new();
    abandoned.read_to_end(&mut reply).unwrap();
    assert!(reply.is_empty(), "fixture replied without a command");
    assert_eq!(
        fs::read_to_string(root.join("status-probed")).unwrap(),
        "status\n"
    );
    // Readiness can connect just before its deadline and abandon the socket
    // before writing. A later request fences handling of that closed peer.
    drop(UnixStream::connect(&socket).unwrap());
    let mut subsequent = UnixStream::connect(&socket).unwrap();
    assert_eq!(status_reply(&mut subsequent)["shell_ready"], false);
    assert_eq!(
        fs::read_to_string(root.join("status-probed")).unwrap(),
        "status\n"
    );
    drop(subsequent);
    drop(child);
    fs::remove_dir_all(root).unwrap();
}
