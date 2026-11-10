//! Managed Satellite entry point: a private compositor socket and readiness pipe.
use std::{
    io::{self, Write},
    os::{fd::AsFd, unix::net::UnixStream},
    process::Stdio,
    sync::Mutex,
};

mod cursor_defaults;
mod dependency;

const UPSTREAM_REVISION: &str = "7bf5e0dcb0d4250860b24b04a6cb3ddeada515d0";

struct Embedded {
    server: Mutex<Option<UnixStream>>,
    flags: Vec<String>,
    xwayland: std::path::PathBuf,
}

impl xwayland_satellite::RunData for Embedded {
    fn xwayland_binary(&self) -> &std::path::Path {
        &self.xwayland
    }
    fn display(&self) -> Option<&str> {
        None
    }

    fn listenfds(&mut self) -> Vec<std::os::fd::OwnedFd> {
        Vec::new()
    }

    fn flags(&self) -> &[String] {
        &self.flags
    }

    fn server(&self) -> Option<UnixStream> {
        self.server.lock().unwrap().take()
    }

    // Only this helper owns the private host socket and readiness pipe. The
    // nested Xwayland must not keep either connection alive after helper exit.
    fn xwayland_stdin(&self) -> Stdio {
        Stdio::null()
    }
    fn xwayland_stdout(&self) -> Stdio {
        Stdio::null()
    }

    fn xwayland_ready(&self, display: String, pid: u32) {
        if let Some(number) = display.strip_prefix(':').and_then(|n| n.parse().ok()) {
            let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
            if let Err(error) = cursor_defaults::initialize(
                number,
                home.as_deref(),
                hearthspace_common::DEFAULT_CURSOR_SIZE,
            ) {
                eprintln!("Satellite cursor default initialization failed: {error}");
            }
        }
        // stdout is exclusively the bounded readiness record; logs use stderr.
        writeln!(io::stdout().lock(), "{display} {pid}").expect("publish Satellite readiness");
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    pretty_env_logger::init();
    let mut arguments = std::env::args().skip(1);
    match arguments.next().as_deref() {
        Some("--version") => {
            println!("hearthspace-satellite 0.1.0 (xwayland-satellite {UPSTREAM_REVISION})");
            return Ok(());
        }
        Some("--embedded") if arguments.next().is_none() => {}
        _ => return Err("usage: hearthspace-satellite --embedded | --version".into()),
    }
    let xwayland = dependency::Dependency::current()?;
    xwayland.validate()?;
    let server = UnixStream::from(io::stdin().as_fd().try_clone_to_owned()?);
    let data = Embedded {
        server: Mutex::new(Some(server)),
        flags: vec![
            "-nolisten".into(),
            "tcp".into(),
            "-surface-input-targets".into(),
        ],
        xwayland: xwayland.binary,
    };
    xwayland_satellite::main(data).ok_or("Satellite exited")?;
    Ok(())
}
