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

    fn paths(&self) -> SessionPaths {
        SessionPaths {
            host_runtime_dir: self.0.clone(),
            session_dir: self.0.join("private"),
            applications_dir: self.0.join("private/share/applications"),
            settings_socket: self.0.join("private/settings.sock"),
            session_socket: self.0.join("private/session.sock"),
            shell_command_socket: self.0.join("private/command.sock"),
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct RunningHelper(Entry);
impl Drop for RunningHelper {
    fn drop(&mut self) {
        self.0.stop();
    }
}

#[test]
fn portal_configuration_and_activation_overrides_stay_inside_the_session() {
    let fixture = Fixture::new("configuration");
    let paths = fixture.paths();
    prepare(&paths, true).unwrap();
    assert_eq!(
        fs::read_to_string(
            paths
                .session_dir
                .join("share/xdg-desktop-portal/hearthspace-portals.conf")
        )
        .unwrap(),
        "[preferred]\ndefault=gtk\norg.freedesktop.impl.portal.Settings=hearthspace\n"
    );
    let implementation = fs::read_to_string(
        paths
            .session_dir
            .join("share/xdg-desktop-portal/portals/hearthspace.portal"),
    )
    .unwrap();
    assert!(implementation.contains(&format!("DBusName={}\n", crate::appearance_portal::NAME)));
    assert!(implementation.contains("Interfaces=org.freedesktop.impl.portal.Settings;"));
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
    assert!(
        paths
            .session_dir
            .join(format!(
                "dbus-1/services/{}.service",
                crate::appearance_portal::NAME
            ))
            .is_file()
    );
    assert!(!fixture.0.join("dbus-1").exists());
}

#[test]
fn shared_sessions_generate_portal_configuration_without_activation_overrides() {
    let fixture = Fixture::new("shared-configuration");
    let paths = fixture.paths();
    prepare(&paths, false).unwrap();
    assert!(
        paths
            .session_dir
            .join("share/xdg-desktop-portal/hearthspace-portals.conf")
            .is_file()
    );
    assert!(
        paths
            .session_dir
            .join("share/xdg-desktop-portal/portals/hearthspace.portal")
            .is_file()
    );
    assert!(!paths.session_dir.join("dbus-1").exists());
}

#[test]
fn native_desktop_services_are_enabled_with_shared_and_private_buses() {
    use clap::Parser;
    assert!(enabled(&Args::parse_from([
        "session",
        "--backend",
        "native"
    ])));
    assert!(enabled(&Args::parse_from([
        "session",
        "--backend",
        "native",
        "--private-session-bus",
    ])));
    assert!(!enabled(&Args::parse_from([
        "session",
        "--backend",
        "native",
        "--no-desktop-services",
    ])));
    assert!(!enabled(&Args::parse_from([
        "session",
        "--backend",
        "nested"
    ])));
}

#[test]
fn stopping_a_managed_helper_also_kills_its_stubborn_descendant() {
    let fixture = Fixture::new("processes");
    let pid_file = fixture.0.join("descendant.pid");
    // Keep the publication window open deliberately: file creation is not a
    // readiness signal, and stopping here used to read an empty/truncated PID.
    let script = "import os,signal,sys,time\nif os.fork() == 0:\n signal.signal(signal.SIGTERM, signal.SIG_IGN)\n with open(sys.argv[1], 'w') as ready:\n  time.sleep(.05)\n  ready.write(str(os.getpid())+'\\n')\n time.sleep(60)\nelse:\n time.sleep(60)\n";
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
    let mut entry = RunningHelper(Entry {
        child: Some(ManagedProcess::spawn(spec.clone(), 0).unwrap()),
        spec,
        adopted: None,
        bus_name: None,
        restarts_left: 0,
    });
    let deadline = Instant::now() + Duration::from_secs(2);
    let pid = loop {
        if let Ok(contents) = fs::read_to_string(&pid_file)
            && let Some(line) = contents.strip_suffix('\n')
            && let Ok(pid) = line.parse::<u32>()
            && pid > 0
        {
            break pid;
        }
        assert!(
            Instant::now() < deadline,
            "helper never published a complete PID"
        );
        thread::sleep(Duration::from_millis(5));
    };
    entry.0.stop();
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

struct RunningServices(DesktopServices);
impl Drop for RunningServices {
    fn drop(&mut self) {
        self.0.shutdown();
    }
}

fn restarting_service(ready: &Path) -> RunningServices {
    let spec = ProcessSpec {
        name: "mock-service",
        program: "/usr/bin/python3".into(),
        args: vec![
            "-c".into(),
            "import os,pathlib,sys,time\npathlib.Path(sys.argv[1]).write_text(str(os.getpid())+'\\n')\ntime.sleep(60)".into(),
            ready.to_string_lossy().into_owned(),
        ],
        envs: Vec::new(),
        env_removals: Vec::new(),
        current_dir: None,
    };
    // The original process has already exited; both restart attempts remain.
    RunningServices(DesktopServices {
        entries: vec![Entry {
            spec,
            bus_name: None,
            child: None,
            adopted: None,
            restarts_left: 2,
        }],
        ..Default::default()
    })
}

fn assert_second_restart_recovers(services: &mut DesktopServices, ready: &Path) {
    assert!(services.entries[0].pid().is_none());
    assert_eq!(services.entries[0].restarts_left, 1);
    let cancelled = AtomicBool::new(false);
    assert!(services.poll(&cancelled));
    let pid = services.entries[0].pid().expect("second restart was lost");
    assert_eq!(services.entries[0].restarts_left, 0);

    // A live process after the short startup grace period need not have run its
    // Python code yet. Observe this particular child's publication before
    // claiming recovery; a previous attempt's file must not satisfy readiness.
    let expected = format!("{pid}\n");
    let deadline = Instant::now() + Duration::from_secs(5);
    while fs::read_to_string(ready).ok().as_deref() != Some(expected.as_str()) {
        assert!(services.entries[0].running().unwrap(), "helper exited");
        assert!(Instant::now() < deadline, "helper never became ready");
        thread::sleep(Duration::from_millis(5));
    }
    assert!(!services.poll(&cancelled));
    assert_eq!(services.entries[0].pid(), Some(pid));
}

#[test]
fn failed_first_restart_does_not_discard_the_second_attempt() {
    let fixture = Fixture::new("restart");
    let ready = fixture.0.join("ready");
    let mut services = restarting_service(&ready);
    let program = services.0.entries[0].spec.program.clone();
    services.0.entries[0].spec.program = fixture.0.join("missing").to_string_lossy().into_owned();

    // Failure is synchronous: this must not depend on Python starting and
    // exiting within wait_ready's 100 ms survival check on a busy CI runner.
    assert!(services.0.poll(&AtomicBool::new(false)));
    services.0.entries[0].spec.program = program;
    assert_second_restart_recovers(&mut services.0, &ready);
}

#[test]
fn failed_restart_readiness_does_not_discard_the_second_attempt() {
    let fixture = Fixture::new("restart-readiness");
    let ready = fixture.0.join("ready");
    let mut services = restarting_service(&ready);

    // Exercise failure after spawn as well. Cancellation makes wait_ready fail
    // regardless of when the child is scheduled, and must remove that child.
    assert!(services.0.poll(&AtomicBool::new(true)));
    assert_second_restart_recovers(&mut services.0, &ready);
}
