//! A listening Wayland socket must not turn an unusable shell into a ready login.
use std::{
    fs,
    os::unix::fs::PermissionsExt,
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
   request=peer.recv(1024)
   open(root+'/status-probed','w').write(request.decode())
   payload=json.dumps({'backend':'nested','active':True,'shell_ready':False,'locked':False,'outputs':[]}).encode()
   peer.sendall(('ok %d\n'%len(payload)).encode()+payload)
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
