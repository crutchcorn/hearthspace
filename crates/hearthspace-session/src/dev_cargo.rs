//! Package and feature selection comes from the install/harness artifact contract.
use std::{io, sync::OnceLock};

use serde::Deserialize;

mod prepare;
pub(crate) use prepare::prepare_helper;

#[derive(Deserialize)]
struct Package {
    package: String,
    binaries: Vec<String>,
    native_features: Vec<String>,
}

fn package(binary: &str) -> io::Result<&'static Package> {
    static INVENTORY: OnceLock<Vec<Package>> = OnceLock::new();
    INVENTORY
        .get_or_init(|| {
            serde_json::from_str(include_str!("../../../scripts/runtime-artifacts.json"))
                .expect("checked-in runtime artifact inventory must be valid")
        })
        .iter()
        .find(|item| item.binaries.iter().any(|name| name == binary))
        .ok_or_else(|| io::Error::other(format!("undeclared development binary: {binary}")))
}

fn selection(binary: &str) -> io::Result<Vec<String>> {
    let package = package(binary)?;
    let mut args = vec![
        "--locked".into(),
        "-p".into(),
        package.package.clone(),
        "--bin".into(),
        binary.into(),
    ];
    for feature in &package.native_features {
        args.extend(["--features".into(), feature.clone()]);
    }
    Ok(args)
}

pub(crate) fn dev_cargo_args(binary: &str, binary_args: &[&str]) -> Vec<String> {
    let mut args = vec!["run".to_string()];
    args.extend(selection(binary).expect("supervisor binaries must be in the runtime inventory"));
    args.push("--".to_string());
    args.extend(binary_args.iter().map(|arg| (*arg).to_string()));
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dev_compositor_child_enables_udev_feature() {
        assert_eq!(
            dev_cargo_args(
                "hearthspace-compositor",
                &[hearthspace_common::NO_SHELL_FLAG]
            ),
            vec![
                "run",
                "--locked",
                "-p",
                "hearthspace-compositor",
                "--bin",
                "hearthspace-compositor",
                "--features",
                "udev",
                "--",
                hearthspace_common::NO_SHELL_FLAG,
            ]
        );
    }

    #[test]
    fn helper_binary_uses_its_owning_package_and_no_compositor_features() {
        assert_eq!(
            selection("hearthspace-app-scope-guard").unwrap(),
            [
                "--locked",
                "-p",
                "hearthspace-session",
                "--bin",
                "hearthspace-app-scope-guard"
            ]
        );
        assert!(selection("not-a-runtime-binary").is_err());
    }
}
