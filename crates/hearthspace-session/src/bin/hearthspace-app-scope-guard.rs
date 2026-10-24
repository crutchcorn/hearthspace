use hearthspace_desktop::activation as activation_cleanup;
#[path = "../app_scopes/cleanup.rs"]
mod app_scope_cleanup;
#[path = "../bounded_command.rs"]
mod bounded_command;
#[path = "../app_scopes/guard.rs"]
mod guard;
#[path = "../app_scopes/identity.rs"]
mod identity;
#[path = "../app_scopes/ptyxis_run.rs"]
mod ptyxis_run;

// The standalone helper reuses the same deadline/cancellation contract without
// pulling in the supervisor's desktop readiness implementation.
use activation_cleanup::operations as readiness;

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
    if let [flag, runtime] = args.as_slice()
        && flag == "--retire-activation"
    {
        return activation_cleanup::retire_from_session_dir(std::path::Path::new(runtime), None);
    }
    if let [flag, runtime, host_runtime] = args.as_slice()
        && flag == "--stop-app-scopes"
    {
        return app_scope_cleanup::shutdown(
            std::path::Path::new(runtime),
            std::path::Path::new(host_runtime),
        );
    }
    guard::run(&args)
}
