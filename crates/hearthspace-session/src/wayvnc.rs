//! Explicit development-only sharing, isolated from user WayVNC configuration.
use std::{io::Read, net::TcpStream, path::PathBuf, time::Duration};

use crate::{
    cli::{Args, Backend},
    compositor::{CompositorBackend, select_compositor_backend},
    process::ProcessSpec,
};

pub(crate) struct Configuration {
    pub(crate) program: PathBuf,
    pub(crate) port: u16,
}

pub(crate) fn prepare(args: &Args) -> Result<Option<Configuration>, Box<dyn std::error::Error>> {
    if !args.vnc {
        return Ok(None);
    }
    let native = match args.backend {
        Some(Backend::Native) => true,
        Some(Backend::Nested) => false,
        None => select_compositor_backend() == CompositorBackend::NativeTty,
    };
    if !native {
        return Err("--vnc requires the native backend to mirror the physical desktop".into());
    }
    let program = which::which("wayvnc")
        .map_err(|_| "--vnc requires WayVNC; install wayvnc before starting the desktop")?;
    Ok(Some(Configuration {
        program,
        port: args.vnc_port,
    }))
}

impl Configuration {
    pub(crate) fn spec(
        &self,
        envs: Vec<(String, String)>,
        runtime: &std::path::Path,
    ) -> ProcessSpec {
        ProcessSpec {
            name: "wayvnc",
            program: self.program.to_string_lossy().into_owned(),
            args: vec![
                "--config".into(),
                "/dev/null".into(),
                "--render-cursor".into(),
                "--disable-resizing".into(),
                "--socket".into(),
                runtime
                    .join("wayvnc-control.sock")
                    .to_string_lossy()
                    .into_owned(),
                "127.0.0.1".into(),
                self.port.to_string(),
            ],
            envs,
            env_removals: vec!["WAYLAND_SOCKET".into(), "DISPLAY".into()],
            current_dir: None,
        }
    }

    pub(crate) fn ready(&self, timeout: Duration) -> bool {
        let address = ([127, 0, 0, 1], self.port).into();
        let Ok(mut socket) = TcpStream::connect_timeout(&address, timeout) else {
            return false;
        };
        if socket.set_read_timeout(Some(timeout)).is_err() {
            return false;
        }
        let mut greeting = [0; 12];
        socket.read_exact(&mut greeting).is_ok()
            && &greeting[..4] == b"RFB "
            && greeting[11] == b'\n'
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn sharing_requires_explicit_dev_and_a_valid_port() {
        for args in [
            vec!["session", "--vnc"],
            vec!["session", "--installed", "--vnc"],
            vec!["session", "--dev", "--vnc-port", "5901"],
            vec!["session", "--dev", "--vnc", "--vnc-port", "0"],
            vec!["session", "--dev", "--vnc", "--vnc-port", "65536"],
        ] {
            assert!(Args::try_parse_from(args).is_err());
        }
        let args = Args::parse_from(["session", "--dev", "--vnc"]);
        assert_eq!(args.vnc_port, 5900);
        let args = Args::parse_from(["session", "--dev", "--vnc", "--backend", "nested"]);
        assert!(prepare(&args).is_err());
    }

    #[test]
    fn wayvnc_keeps_input_enabled_and_ignores_user_configuration() {
        let config = Configuration {
            program: "/usr/bin/wayvnc".into(),
            port: 5911,
        };
        let envs = vec![("WAYLAND_DISPLAY".into(), "wayland-private".into())];
        let spec = config.spec(envs.clone(), std::path::Path::new("/run/private"));
        assert_eq!(spec.envs, envs);
        assert_eq!(
            spec.args,
            [
                "--config",
                "/dev/null",
                "--render-cursor",
                "--disable-resizing",
                "--socket",
                "/run/private/wayvnc-control.sock",
                "127.0.0.1",
                "5911"
            ]
        );
        // GPU capture is opt-in in WayVNC; omitting --gpu selects the SHM path.
        assert!(
            !spec
                .args
                .iter()
                .any(|arg| arg == "--gpu" || arg == "--disable-input")
        );
    }
}
