use super::*;
use crate::{dev_cargo::dev_cargo_args, process::ProcessSpec, session_env::repo_root};

impl Supervisor {
    pub(super) fn record_processes(&self) -> std::io::Result<()> {
        let pid =
            |process: &Option<ManagedProcess>| process.as_ref().map_or(0, ManagedProcess::pid);
        fs::write(
            self.paths.session_dir.join("processes.json"),
            format!(
                "{{\"supervisor\":{},\"settingsd\":{},\"compositor\":{},\"shell\":{},\"notifications\":{},\"session_bus\":{},\"wayvnc\":{}{}}}\n",
                std::process::id(),
                pid(&self.settingsd),
                pid(&self.compositor),
                pid(&self.shell),
                pid(&self.notifications),
                pid(&self.session_bus),
                pid(&self.wayvnc),
                self.desktop_services.process_json_fields(),
            ),
        )
    }

    pub(super) fn binary_spec(
        &self,
        binary: &'static str,
        binary_args: &[&str],
        include_client_wayland: bool,
    ) -> ProcessSpec {
        let mut envs = self.child_env(include_client_wayland);
        if !include_client_wayland {
            envs.push((
                hearthspace_common::HEARTHSPACE_WAYLAND_DISPLAY_ENV.to_string(),
                self.session_env.wayland_display.clone(),
            ));
        }

        let current_dir = (self.mode == RunMode::DevCargo).then(repo_root);
        match self.mode {
            RunMode::DevCargo => {
                let args = dev_cargo_args(binary, binary_args);
                ProcessSpec {
                    name: binary,
                    program: "cargo".to_string(),
                    args,
                    envs,
                    env_removals: self.child_env_removals(),
                    current_dir,
                }
            }
            RunMode::InstalledBinaries => ProcessSpec {
                name: binary,
                program: if binary == "hearthspace-compositor" {
                    self.compositor_binary.clone()
                } else {
                    None
                }
                .or_else(|| self.binary_dir.as_ref().map(|dir| dir.join(binary)))
                .map_or_else(
                    || binary.to_string(),
                    |path| path.to_string_lossy().into_owned(),
                ),
                args: binary_args.iter().map(|arg| (*arg).to_string()).collect(),
                envs,
                env_removals: self.child_env_removals(),
                current_dir,
            },
        }
    }

    fn child_env_removals(&self) -> Vec<String> {
        if self
            .session_env
            .vars
            .iter()
            .any(|(key, _)| key == "HEARTHSPACE_SESSION_BUS_ADDRESS")
        {
            [
                "DISPLAY",
                "WAYLAND_SOCKET",
                "AT_SPI_BUS_ADDRESS",
                "DBUS_SESSION_BUS_PID",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect()
        } else {
            Vec::new()
        }
    }

    pub(super) fn child_env(&self, include_client_wayland: bool) -> Vec<(String, String)> {
        self.session_env
            .vars
            .iter()
            .filter(|(key, _)| {
                include_client_wayland || key != hearthspace_common::WAYLAND_DISPLAY_ENV
            })
            .cloned()
            .collect()
    }

    pub(super) fn watcher_spec(&self) -> Option<ProcessSpec> {
        let envs = self.child_env(true);
        let current_dir = Some(repo_root());
        if which::which("cargo-watch").is_ok() {
            return Some(ProcessSpec {
                name: "settings-watch",
                program: "cargo".to_string(),
                args: vec![
                    "watch".to_string(),
                    "-w".to_string(),
                    "crates/hearthspace-settings".to_string(),
                    "-w".to_string(),
                    "crates/hearthspace-ipc".to_string(),
                    "-w".to_string(),
                    "crates/hearthspace-common".to_string(),
                    "-x".to_string(),
                    "build -p hearthspace-settings".to_string(),
                ],
                envs,
                env_removals: self.child_env_removals(),
                current_dir,
            });
        }
        if which::which("watchexec").is_ok() {
            return Some(ProcessSpec {
                name: "settings-watch",
                program: "watchexec".to_string(),
                args: vec![
                    "-w".to_string(),
                    "crates/hearthspace-settings".to_string(),
                    "-w".to_string(),
                    "crates/hearthspace-ipc".to_string(),
                    "-w".to_string(),
                    "crates/hearthspace-common".to_string(),
                    "--".to_string(),
                    "cargo".to_string(),
                    "build".to_string(),
                    "-p".to_string(),
                    "hearthspace-settings".to_string(),
                ],
                envs,
                env_removals: self.child_env_removals(),
                current_dir,
            });
        }
        None
    }
}
