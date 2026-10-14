//! Native presentation probe using the same protocol client as the regressions.
//! This owns its own test lock; it cannot unlock swaylock or any other client.
use std::{
    io::Write,
    path::Path,
    time::{Duration, Instant},
};

#[path = "../tests/support/lock_client.rs"]
mod lock_client;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if !(2..=4).contains(&args.len()) {
        return Err("usage: session-lock-fixture SOCKET RELEASE_FILE [unlock|disconnect|destroy-surfaces] [TIMEOUT_SECONDS]".into());
    }
    let mode = args.get(2).map(String::as_str).unwrap_or("unlock");
    if !matches!(mode, "unlock" | "disconnect" | "destroy-surfaces") {
        return Err("unknown fixture mode".into());
    }
    let timeout: u64 = args
        .get(3)
        .map(|value| value.parse())
        .transpose()?
        .unwrap_or(60);
    if !(1..=600).contains(&timeout) {
        return Err("timeout must be between 1 and 600 seconds".into());
    }
    let release = Path::new(&args[1]);
    if release.exists() {
        return Err("release file already exists; use a fresh owned test path".into());
    }
    let mut locker = lock_client::LockClient::connect(Path::new(&args[0]));
    locker.wait_locked();
    let mut competing = lock_client::LockClient::connect(Path::new(&args[0]));
    competing.wait_rejected();
    drop(competing);
    if mode == "destroy-surfaces" {
        locker.destroy_surfaces();
    }
    println!("locked; competing client rejected; mode={mode}");
    std::io::stdout().flush()?;
    let deadline = Instant::now() + Duration::from_secs(timeout);
    while !release.exists() {
        if Instant::now() >= deadline {
            return Err("release deadline expired; disconnecting without unlocking".into());
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    if mode == "disconnect" {
        drop(locker);
        println!("disconnected without unlocking");
    } else {
        locker.unlock();
        println!("own test lock unlocked");
    }
    Ok(())
}
