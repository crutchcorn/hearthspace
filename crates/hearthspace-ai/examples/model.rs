//! Explicit local smoke test: `cargo run -p hearthspace-ai --example model -- status`.
//! `install` downloads ~1.67GB; `remove` deletes only the managed D1 model/cache.
//! For several selected titles: `gather --selected TITLE --selected TITLE -- CANDIDATE...`.
use std::sync::atomic::AtomicBool;

use hearthspace_ai::{DecisionClient, DevicePreference, ModelManager, Result, WindowCandidate};

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let action = args.next().unwrap_or_else(|| "status".into());
    let manager = ModelManager::from_env()?;
    match action.as_str() {
        "status" => println!("{:#?}", manager.status()?),
        "install" => manager.install(DevicePreference::Automatic, |progress| {
            eprintln!(
                "{:?}: {} / {:?}",
                progress.phase, progress.completed, progress.total
            );
        })?,
        "remove" => manager.remove()?,
        "warm" => manager.warm()?,
        "unload" => manager.unload()?,
        "cpu" => manager.set_device(DevicePreference::Cpu)?,
        "auto" => manager.set_device(DevicePreference::Automatic)?,
        "gather" => {
            let mut args = args.peekable();
            let mut group = Vec::new();
            while args.peek().is_some_and(|argument| argument == "--selected") {
                args.next();
                group.push(
                    args.next()
                        .ok_or_else(|| anyhow::anyhow!("--selected requires a title"))?,
                );
            }
            if group.is_empty() {
                group.push(args.next().unwrap_or_else(|| "Hearthspace editor".into()));
            }
            if args.peek().is_some_and(|argument| argument == "--") {
                args.next();
            }
            let candidates: Vec<_> = args
                .enumerate()
                .map(|(index, title)| WindowCandidate {
                    id: index as u64,
                    title,
                })
                .collect();
            let result = DecisionClient::from_env()?.gather(
                &group,
                &candidates,
                &AtomicBool::new(false),
                |done, total| eprintln!("{done}/{total}"),
            )?;
            println!("Relevant candidate indices: {result:?}");
        }
        _ => anyhow::bail!(
            "Expected status, install, remove, warm, unload, cpu, auto, or gather GROUP CANDIDATE..."
        ),
    }
    Ok(())
}
