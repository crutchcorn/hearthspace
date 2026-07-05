pub(crate) fn dev_cargo_args(binary: &str, binary_args: &[&str]) -> Vec<String> {
    let mut args = vec!["run".to_string()];
    if cargo_package_exists(binary) {
        args.extend(["-p".to_string(), binary.to_string()]);
    } else {
        args.extend(["--bin".to_string(), binary.to_string()]);
    }
    if binary == "hearthspace-compositor" {
        args.extend(["--features".to_string(), "udev".to_string()]);
    }
    args.push("--".to_string());
    args.extend(binary_args.iter().map(|arg| (*arg).to_string()));
    args
}

fn cargo_package_exists(package: &str) -> bool {
    matches!(
        package,
        "hearthspace-compositor"
            | "hearthspace-settings"
            | "hearthspace-settingsd"
            | "hearthspace-shell"
            | "hearthspace-ui-storybook"
    )
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
                "-p",
                "hearthspace-compositor",
                "--features",
                "udev",
                "--",
                hearthspace_common::NO_SHELL_FLAG,
            ]
        );
    }
}
