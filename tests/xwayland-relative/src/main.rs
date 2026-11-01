//! A supervised, private Xwayland regression, with no desktop backend.
mod cancellation;
mod client;
mod evidence;
mod owned;
mod scenario;
mod server;
mod state;
mod xwm;

use anyhow::{Context, Result, ensure};
use std::{fs, path::PathBuf, process::ExitCode};

pub const SELECTOR: &str =
    "x11_live_async_child_confinement_clamps_each_warp_emulated_relative_frame";

struct Options {
    binary: PathBuf,
    targets: bool,
    worker: bool,
    artifacts: Option<PathBuf>,
    output: Option<PathBuf>,
}

impl Options {
    fn parse() -> Result<Self> {
        let mut args = std::env::args_os().skip(1);
        let mut binary = None;
        let mut targets = false;
        let mut worker = false;
        let mut artifacts = None;
        let mut output = None;
        while let Some(arg) = args.next() {
            match arg.to_str() {
                Some("--xwayland") => binary = args.next().map(PathBuf::from),
                Some("--surface-input-targets") => targets = true,
                Some("--worker") => worker = true,
                Some("--output") => output = args.next().map(PathBuf::from),
                Some("--artifacts") => artifacts = args.next().map(PathBuf::from),
                _ => anyhow::bail!(
                    "usage: relative-confinement-fixture --xwayland /absolute/ELF --output /fresh/evidence [--surface-input-targets]"
                ),
            }
        }
        let binary = binary.context("--xwayland is required; no PATH lookup")?;
        ensure!(binary.is_absolute(), "Xwayland path must be absolute");
        let binary = binary.canonicalize()?;
        use std::io::Read;
        let mut magic = [0; 4];
        fs::File::open(&binary)?.read_exact(&mut magic)?;
        ensure!(&magic == b"\x7fELF", "supply the binary, not a wrapper");
        ensure!(
            worker == artifacts.is_some(),
            "--artifacts is internal to supervision"
        );
        ensure!(unsafe { libc::geteuid() } != 0, "run as a normal user");
        ensure!(
            worker != output.is_some(),
            "--output is required by the supervisor only"
        );
        if let Some(path) = &output {
            ensure!(path.is_absolute(), "--output must be absolute");
            ensure!(!path.try_exists()?, "--output must be fresh");
        }
        Ok(Self {
            binary,
            targets,
            worker,
            artifacts,
            output,
        })
    }
}

fn worker(opts: &Options) -> Result<i32> {
    let artifacts = opts.artifacts.as_ref().unwrap();
    let mut process = owned::Xwayland::spawn(&opts.binary, opts.targets, artifacts)?;
    let server = server::Server::start(process.wayland.take().unwrap())?;
    let wm = xwm::Wm::connect(process.wm.take().unwrap())?;
    let result = scenario::run(&server, wm, artifacts);
    // The scenario sends explicit balanced input and releases its grabs first.
    // Stopping the owned process is also the fallback on any setup/protocol error.
    let stopped = server.stop();
    let reaped = process.stop();
    stopped?;
    reaped?;
    match result {
        Ok(mut report) => {
            report.protocol_log(&artifacts.join("xwayland.log"))?;
            let passed = report.failures.is_empty();
            fs::write(
                artifacts.join("report.json"),
                serde_json::to_vec_pretty(&report)?,
            )?;
            println!("{SELECTOR}: {}", if passed { "PASS" } else { "FAIL" });
            Ok(if passed { 0 } else { 1 })
        }
        Err(error) => {
            fs::write(artifacts.join("setup-error.txt"), format!("{error:#}\n"))?;
            Err(error)
        }
    }
}

fn main() -> ExitCode {
    let result = Options::parse().and_then(|opts| {
        if opts.worker {
            worker(&opts)
        } else {
            owned::supervise(&opts)
        }
    });
    match result {
        Ok(code) => ExitCode::from(code as u8),
        Err(error) => {
            eprintln!("{SELECTOR}: SETUP ERROR: {error:#}");
            ExitCode::from(2)
        }
    }
}
