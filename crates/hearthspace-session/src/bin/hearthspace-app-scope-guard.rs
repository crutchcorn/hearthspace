#[path = "../app_scopes/guard.rs"]
mod guard;
#[path = "../app_scopes/identity.rs"]
mod identity;
#[path = "../app_scopes/ptyxis_run.rs"]
mod ptyxis_run;

fn main() -> std::io::Result<()> {
    let mut args = std::env::args_os();
    if args.next().is_some_and(|path| {
        std::path::Path::new(&path)
            .file_name()
            .is_some_and(|name| name == "systemd-run")
    }) {
        return ptyxis_run::run(&args.collect::<Vec<_>>());
    }
    let args = args
        .map(|arg| {
            arg.into_string()
                .map_err(|_| std::io::Error::other("invalid guard argument"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    guard::run(&args)
}
