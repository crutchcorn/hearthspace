//! Prepare the shell's own resources before initializing its UI and renderer.

use rustix::process::{Resource, Rlimit, getrlimit, setrlimit};

pub(super) fn raise_file_descriptor_limit() {
    let inherited = getrlimit(Resource::Nofile);
    if inherited.current == inherited.maximum {
        tracing::debug!(
            soft_limit = ?inherited.current,
            hard_limit = ?inherited.maximum,
            "shell file descriptor soft limit already matches its hard limit"
        );
        return;
    }

    // Each live Vulkan surface can retain several device and buffer descriptors.
    // Use the shell's existing allowance without changing the hard limit or the
    // supervising session's limits. Descendants inherit the shell's soft limit.
    let requested = Rlimit {
        current: inherited.maximum,
        ..inherited
    };
    match setrlimit(Resource::Nofile, requested) {
        Ok(()) => tracing::info!(
            previous_soft_limit = ?inherited.current,
            soft_limit = ?requested.current,
            hard_limit = ?requested.maximum,
            "raised shell file descriptor soft limit before UI initialization"
        ),
        Err(error) => tracing::warn!(
            %error,
            soft_limit = ?inherited.current,
            hard_limit = ?inherited.maximum,
            "could not raise shell file descriptor soft limit; continuing with the inherited limit, which may exhaust descriptors and prevent rendering with many UI surfaces"
        ),
    }
}

#[cfg(test)]
mod tests {
    use std::{env, fs::File, os::fd::AsRawFd, process::Command};

    use super::*;

    const CHILD_TEST: &str = "HEARTHSPACE_TEST_SHELL_FILE_DESCRIPTOR_LIMIT";
    const CHILD_COMPLETED: &str = "shell file descriptor regression completed:";

    #[test]
    fn opens_past_inherited_soft_limit_without_changing_hard_limit() {
        run_isolated(
            "startup_resources::tests::opens_past_inherited_soft_limit_without_changing_hard_limit",
            || {
                let inherited = lower_child_limits(64, 256);
                let mut files = Vec::new();
                loop {
                    match File::open("/dev/null") {
                        Ok(file) => files.push(file),
                        Err(error) => {
                            assert_eq!(
                                error.raw_os_error(),
                                Some(rustix::io::Errno::MFILE.raw_os_error()),
                                "the lowered soft limit must cause descriptor exhaustion"
                            );
                            break;
                        }
                    }
                }

                raise_file_descriptor_limit();

                let adjusted = getrlimit(Resource::Nofile);
                assert_eq!(adjusted.current, inherited.maximum);
                assert_eq!(adjusted.maximum, inherited.maximum);
                // Retain the exhausted descriptors, then open enough new ones
                // to exceed the old limit even without counting inherited FDs.
                while files.len() <= 64 {
                    files.push(File::open("/dev/null").expect("open beyond the old soft limit"));
                }
                assert!(files.last().unwrap().as_raw_fd() >= 64);
            },
        );
    }

    #[test]
    fn leaves_equal_soft_and_hard_limits_unchanged() {
        run_isolated(
            "startup_resources::tests::leaves_equal_soft_and_hard_limits_unchanged",
            || {
                let inherited = lower_child_limits(64, 64);

                raise_file_descriptor_limit();

                assert_eq!(getrlimit(Resource::Nofile), inherited);
                File::open("/dev/null").expect("the unchanged limit remains usable");
            },
        );
    }

    fn lower_child_limits(current: u64, maximum: u64) -> Rlimit {
        let inherited = getrlimit(Resource::Nofile);
        assert!(
            inherited.maximum.is_none_or(|limit| limit >= maximum),
            "the test child needs an inherited hard limit of at least {maximum}"
        );
        let lowered = Rlimit {
            current: Some(current),
            maximum: Some(maximum),
        };
        setrlimit(Resource::Nofile, lowered).expect("lower the isolated child's descriptor limits");
        assert_eq!(getrlimit(Resource::Nofile), lowered);
        lowered
    }

    fn run_isolated(test: &str, check: impl FnOnce()) {
        if env::var(CHILD_TEST).as_deref() == Ok(test) {
            check();
            println!("{CHILD_COMPLETED} {test}");
            return;
        }

        let parent_limits = getrlimit(Resource::Nofile);
        let output = Command::new(env::current_exe().unwrap())
            .args(["--exact", test, "--nocapture", "--test-threads=1"])
            .env(CHILD_TEST, test)
            .output()
            .expect("run the descriptor regression in an isolated child process");
        assert_eq!(
            getrlimit(Resource::Nofile),
            parent_limits,
            "the shell subprocess must not change its parent's limits"
        );
        assert!(
            output.status.success(),
            "isolated regression {test} failed ({}):\n{}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains(&format!("{CHILD_COMPLETED} {test}")),
            "the child must execute and complete {test}, not just exit successfully"
        );
    }
}
