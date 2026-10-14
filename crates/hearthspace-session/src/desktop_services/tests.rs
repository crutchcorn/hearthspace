use super::*;

struct Fixture(PathBuf);
impl Fixture {
    fn new(label: &str) -> Self {
        let path = env::temp_dir().join(format!(
            "hearthspace-services-{}-{label}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn portal_configuration_and_activation_overrides_stay_inside_the_session() {
    let fixture = Fixture::new("configuration");
    let paths = SessionPaths {
        host_runtime_dir: fixture.0.clone(),
        session_dir: fixture.0.join("private"),
        applications_dir: fixture.0.join("private/share/applications"),
        settings_socket: fixture.0.join("private/settings.sock"),
        session_socket: fixture.0.join("private/session.sock"),
        shell_command_socket: fixture.0.join("private/command.sock"),
    };
    prepare(&paths).unwrap();
    assert_eq!(
        fs::read_to_string(
            paths
                .session_dir
                .join("share/xdg-desktop-portal/hearthspace-portals.conf")
        )
        .unwrap(),
        "[preferred]\ndefault=gtk\n"
    );
    for (_, _, bus) in DEFINITIONS {
        if let Some(bus) = bus {
            let service = fs::read_to_string(
                paths
                    .session_dir
                    .join(format!("dbus-1/services/{bus}.service")),
            )
            .unwrap();
            assert!(service.contains(&format!("Name={bus}\n")));
            assert!(service.contains("Exec=/usr/bin/false\n"));
            assert!(!service.contains("SystemdService="));
        }
    }
    assert!(!fixture.0.join("dbus-1").exists());
}

#[test]
fn stopping_a_managed_helper_also_kills_its_stubborn_descendant() {
    let fixture = Fixture::new("processes");
    let pid_file = fixture.0.join("descendant.pid");
    let script = "import os,signal,sys,time\nif os.fork() == 0:\n signal.signal(signal.SIGTERM, signal.SIG_IGN)\n open(sys.argv[1], 'w').write(str(os.getpid()))\n time.sleep(60)\nelse:\n time.sleep(60)\n";
    let spec = ProcessSpec {
        name: "mock-service",
        program: "/usr/bin/python3".into(),
        args: vec![
            "-c".into(),
            script.into(),
            pid_file.to_string_lossy().into_owned(),
        ],
        envs: Vec::new(),
        env_removals: Vec::new(),
        current_dir: None,
    };
    let mut entry = Entry {
        child: Some(ManagedProcess::spawn(spec.clone(), 0).unwrap()),
        spec,
        adopted: None,
        bus_name: None,
        restarts_left: 0,
    };
    let deadline = Instant::now() + Duration::from_secs(2);
    while !pid_file.exists() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(5));
    }
    let pid = fs::read_to_string(&pid_file);
    entry.stop();
    let pid = pid.unwrap().parse::<u32>().unwrap();
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        let alive = fs::read_to_string(format!("/proc/{pid}/stat"))
            .is_ok_and(|stat| !stat.rsplit_once(") ").unwrap().1.starts_with('Z'));
        if !alive {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "owned helper descendant survived cleanup"
        );
        thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn failed_first_restart_does_not_discard_the_second_attempt() {
    let fixture = Fixture::new("restart");
    let attempts = fixture.0.join("attempts");
    let spec = ProcessSpec {
        name: "mock-service", program: "/usr/bin/python3".into(),
        args: vec!["-c".into(), "import pathlib,sys,time\np=pathlib.Path(sys.argv[1])\nn=int(p.read_text())+1 if p.exists() else 1\np.write_text(str(n))\nif n == 1: sys.exit(7)\ntime.sleep(60)".into(), attempts.to_string_lossy().into_owned()],
        envs: Vec::new(), env_removals: Vec::new(), current_dir: None,
    };
    // The original process has already exited; both restart attempts remain.
    let mut services = DesktopServices {
        entries: vec![Entry {
            spec,
            bus_name: None,
            child: None,
            adopted: None,
            restarts_left: 2,
        }],
        ..Default::default()
    };
    let cancelled = AtomicBool::new(false);
    let first_changed = services.poll(&cancelled);
    let first_failed = services.entries[0].pid().is_none();
    let remaining = services.entries[0].restarts_left;
    let second_changed = services.poll(&cancelled);
    let recovered = services.entries[0].pid().is_some();
    services.shutdown();
    assert!(first_changed && first_failed);
    assert_eq!(remaining, 1);
    assert!(second_changed && recovered);
    assert_eq!(fs::read_to_string(attempts).unwrap(), "2");
}
