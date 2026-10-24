use super::*;
use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::PathBuf,
    process::{Child, Stdio},
    sync::atomic::{AtomicU64, Ordering},
};

use crate::desktop_services::DesktopServices;

pub(super) struct Fixture {
    helper: ChildGuard,
    _broker: ChildGuard,
    directory: Directory,
    pub(super) bus: String,
    pub(super) runtime: PathBuf,
    manager: PathBuf,
}

impl Fixture {
    pub(super) fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "hearthspace-shared-services-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        let directory = Directory(root);
        let root = &directory.0;
        let config = root.join("bus.conf");
        // No activation directories or connection to the real user manager.
        fs::write(
            &config,
            "<busconfig><type>session</type><listen>unix:tmpdir=/tmp</listen>\
             <auth>EXTERNAL</auth><policy context=\"default\">\
             <allow own=\"*\"/><allow send_destination=\"*\"/>\
             <allow receive_sender=\"*\"/></policy></busconfig>",
        )
        .unwrap();
        let address = root.join("address");
        let mut broker = ChildGuard(
            Command::new("dbus-daemon")
                .args(["--nofork", "--print-address=1", "--nopidfile"])
                .arg(format!("--config-file={}", config.display()))
                .stdout(fs::File::create(&address).unwrap())
                .spawn()
                .unwrap(),
        );
        let deadline = Instant::now() + Duration::from_secs(5);
        let bus = loop {
            if let Ok(text) = fs::read_to_string(&address)
                && let Some(line) = text.strip_suffix('\n')
            {
                break line.to_owned();
            }
            assert!(broker.0.try_wait().unwrap().is_none(), "broker exited");
            assert!(Instant::now() < deadline, "broker did not publish its bus");
            thread::sleep(Duration::from_millis(5));
        };
        let mut helper = ChildGuard(
            Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "desktop_services::shared::tests::support::borrowed_portal_helper",
                    "--ignored",
                    "--nocapture",
                ])
                .env("HEARTHSPACE_TEST_SHARED_SERVICE_ROOT", root)
                .env("HEARTHSPACE_TEST_SHARED_SERVICE_BUS", &bus)
                .stdout(Stdio::null())
                .spawn()
                .unwrap(),
        );
        let deadline = Instant::now() + Duration::from_secs(5);
        while !root.join("helper-ready").exists() {
            assert!(helper.0.try_wait().unwrap().is_none(), "helper exited");
            assert!(Instant::now() < deadline, "helper did not become ready");
            thread::sleep(Duration::from_millis(5));
        }
        let manager = root.join("systemctl");
        fs::write(&manager, include_str!("systemctl.py")).unwrap();
        fs::set_permissions(&manager, fs::Permissions::from_mode(0o700)).unwrap();
        fs::create_dir(root.join("host-runtime")).unwrap();
        let runtime = root.join("host-link");
        symlink(root.join("host-runtime"), &runtime).unwrap();
        Self {
            helper,
            _broker: broker,
            directory,
            bus,
            runtime,
            manager,
        }
    }

    pub(super) fn root(&self) -> &Path {
        &self.directory.0
    }

    pub(super) fn pid(&self) -> u32 {
        self.helper.0.id()
    }

    pub(super) fn start(
        &self,
        services: &mut DesktopServices,
        deadline: Instant,
        cancelled: &AtomicBool,
    ) -> io::Result<()> {
        services.shared.start_with_systemctl(
            &self.manager,
            &self.runtime,
            &self.bus,
            deadline,
            cancelled,
        )
    }

    pub(super) fn publish(&self, unit: &str, bus_name: &str) {
        fs::write(self.root().join(unit), "").unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if owner_pid(&self.bus, bus_name, deadline, &AtomicBool::new(false)).unwrap()
                == Some(self.pid())
            {
                return;
            }
            assert!(Instant::now() < deadline, "service never acquired its name");
            thread::sleep(Duration::from_millis(5));
        }
    }

    pub(super) fn calls(&self) -> Vec<serde_json::Value> {
        fs::read_to_string(self.root().join("commands.jsonl"))
            .unwrap_or_default()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    pub(super) fn assert_running(&mut self) {
        assert!(self.helper.0.try_wait().unwrap().is_none());
    }

    pub(super) fn stop_helper(&mut self) {
        self.helper.0.kill().unwrap();
        self.helper.0.wait().unwrap();
    }
}

struct ChildGuard(Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct Directory(PathBuf);
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
#[ignore = "child process used by isolated shared desktop service tests"]
fn borrowed_portal_helper() {
    let Ok(root) = std::env::var("HEARTHSPACE_TEST_SHARED_SERVICE_ROOT") else {
        return;
    };
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let bus = std::env::var("HEARTHSPACE_TEST_SHARED_SERVICE_BUS").unwrap();
            let connection = zbus::connection::Builder::address(bus.as_str())
                .unwrap()
                .method_timeout(Duration::from_secs(2))
                .build()
                .await
                .unwrap();
            let root = Path::new(&root);
            fs::write(root.join("helper-ready"), "").unwrap();
            let mut pending: Vec<_> = DATA_SERVICES
                .iter()
                .chain(DISPLAY_SERVICES)
                .copied()
                .collect();
            loop {
                for &(_, unit, name) in &pending {
                    if root.join(unit).exists() {
                        connection.request_name(name).await.unwrap();
                        fs::write(root.join(format!("{unit}.owned")), "").unwrap();
                    }
                }
                pending.retain(|(_, unit, _)| !root.join(format!("{unit}.owned")).exists());
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        });
}
