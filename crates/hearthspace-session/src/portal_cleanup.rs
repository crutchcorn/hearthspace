//! Detach only recognized desktop FUSE mounts before deleting a private runtime.
use std::{
    fs, io,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

pub(crate) fn unmount_runtime(runtime: &Path) -> io::Result<()> {
    if fs::symlink_metadata(runtime).is_ok_and(|meta| meta.file_type().is_symlink()) {
        return Err(io::Error::other("refusing symlink desktop runtime"));
    }
    let deadline = Instant::now() + Duration::from_secs(2);
    cleanup_mounts(
        runtime,
        || fs::read_to_string("/proc/self/mountinfo"),
        |mount| {
            let result = crate::bounded_command::output(
                Command::new("fusermount3")
                    .args(["-u", "-z", "--"])
                    .arg(mount),
                deadline,
                &AtomicBool::new(false),
            )?;
            if !result.status.success() {
                return Err(io::Error::other(format!(
                    "could not unmount owned desktop mount {}: {}",
                    mount.display(),
                    String::from_utf8_lossy(&result.stderr)
                )));
            }
            Ok(())
        },
    )
}

fn cleanup_mounts(
    runtime: &Path,
    mut read_mounts: impl FnMut() -> io::Result<String>,
    mut unmount: impl FnMut(&Path) -> io::Result<()>,
) -> io::Result<()> {
    let mounts = runtime_mounts(&read_mounts()?, runtime);
    for (target, filesystem) in &mounts {
        let known = (target == &runtime.join("doc") && filesystem == "fuse.portal")
            || (target == &runtime.join("gvfs") && filesystem == "fuse.gvfsd-fuse");
        if !known {
            return Err(io::Error::other(format!(
                "unexpected mounted filesystem in owned runtime: {}",
                target.display()
            )));
        }
    }
    for (target, _) in mounts {
        unmount(&target)?;
    }
    if !runtime_mounts(&read_mounts()?, runtime).is_empty() {
        return Err(io::Error::other(format!(
            "owned desktop runtime still contains mounted filesystems: {}",
            runtime.display()
        )));
    }
    Ok(())
}

fn runtime_mounts(mounts: &str, runtime: &Path) -> Vec<(PathBuf, String)> {
    mounts
        .lines()
        .filter_map(|line| {
            let (fields, filesystem) = line.split_once(" - ")?;
            let mount = PathBuf::from(decode_mount_path(fields.split_whitespace().nth(4)?));
            let filesystem = filesystem.split_whitespace().next()?;
            mount
                .starts_with(runtime)
                .then(|| (mount, filesystem.to_owned()))
        })
        .collect()
}

fn decode_mount_path(path: &str) -> std::ffi::OsString {
    use std::os::unix::ffi::OsStringExt;
    let mut decoded = Vec::new();
    let bytes = path.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\\'
            && index + 3 < bytes.len()
            && bytes[index + 1..index + 4]
                .iter()
                .all(|byte| matches!(byte, b'0'..=b'7'))
        {
            let number = (u16::from(bytes[index + 1] - b'0') * 64)
                + (u16::from(bytes[index + 2] - b'0') * 8)
                + u16::from(bytes[index + 3] - b'0');
            if let Ok(byte) = u8::try_from(number) {
                decoded.push(byte);
                index += 4;
                continue;
            }
        }
        decoded.push(bytes[index]);
        index += 1;
    }
    std::ffi::OsString::from_vec(decoded)
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOST: &str = "36 25 0:32 / /run/user/1000/doc rw - fuse.portal portal rw\n38 25 0:34 / /run/user/1000/gvfs rw - fuse.gvfsd-fuse gvfsd-fuse rw\n";
    const DOCUMENT: &str = "37 25 0:33 / /run/user/1000/private/doc rw - fuse.portal portal rw\n";
    const GVFS: &str =
        "39 25 0:35 / /run/user/1000/private/gvfs rw - fuse.gvfsd-fuse gvfsd-fuse rw\n";
    const RUNTIME: &str = "/run/user/1000/private";

    #[test]
    fn detaches_both_owned_mounts_and_preserves_host_mounts() {
        let mut snapshots = [format!("{HOST}{DOCUMENT}{GVFS}"), HOST.to_owned()].into_iter();
        let mut detached = Vec::new();
        cleanup_mounts(
            Path::new(RUNTIME),
            || Ok(snapshots.next().unwrap()),
            |target| {
                detached.push(target.to_path_buf());
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(
            detached,
            [
                Path::new(RUNTIME).join("doc"),
                Path::new(RUNTIME).join("gvfs")
            ]
        );
    }

    #[test]
    fn host_only_mounts_are_never_detached() {
        cleanup_mounts(
            Path::new(RUNTIME),
            || Ok(HOST.into()),
            |_| panic!("host mount detached"),
        )
        .unwrap();
    }

    #[test]
    fn wrong_filesystem_and_unknown_mounts_are_rejected_before_detaching() {
        for unexpected in [
            GVFS.replace("fuse.gvfsd-fuse", "ext4"),
            GVFS.replace("/private/gvfs", "/private/other"),
            GVFS.replace("/private/gvfs", "/private/gvfs/nested"),
            GVFS.replace("/private/gvfs", "/private"),
        ] {
            let error = cleanup_mounts(
                Path::new(RUNTIME),
                || Ok(format!("{DOCUMENT}{unexpected}")),
                |_| panic!("unexpected mount detached"),
            )
            .unwrap_err();
            assert!(error.to_string().contains("unexpected mounted"));
        }
    }

    #[test]
    fn remaining_mount_prevents_runtime_removal() {
        let error = cleanup_mounts(Path::new(RUNTIME), || Ok(GVFS.into()), |_| Ok(())).unwrap_err();
        assert!(error.to_string().contains("still contains mounted"));
    }

    #[test]
    fn unmount_failure_stops_cleanup() {
        let error = cleanup_mounts(
            Path::new(RUNTIME),
            || Ok(GVFS.into()),
            |_| Err(io::Error::other("helper failed")),
        )
        .unwrap_err();
        assert_eq!(error.to_string(), "helper failed");
    }

    #[test]
    fn mount_paths_with_spaces_are_decoded_without_prefix_matching() {
        let mounts = "37 25 0:33 / /tmp/private\\040session/doc rw - fuse.portal portal rw\n";
        assert_eq!(
            runtime_mounts(mounts, Path::new("/tmp/private session")),
            vec![(
                PathBuf::from("/tmp/private session/doc"),
                "fuse.portal".into()
            )]
        );
        assert!(runtime_mounts(mounts, Path::new("/tmp/private")).is_empty());
        assert!(runtime_mounts("invalid - \n", Path::new(RUNTIME)).is_empty());
    }
}
