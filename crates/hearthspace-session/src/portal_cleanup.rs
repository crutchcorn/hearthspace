use std::{
    fs, io,
    path::Path,
    process::Command,
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

pub(crate) fn unmount_documents(runtime: &Path) -> io::Result<()> {
    let mount = runtime.join("doc");
    let mounts = fs::read_to_string("/proc/self/mountinfo")?;
    if !has_document_mount(&mounts, &mount) {
        return Ok(());
    }
    let result = crate::bounded_command::output(
        Command::new("fusermount3")
            .args(["-u", "-z", "--"])
            .arg(&mount),
        Instant::now() + Duration::from_secs(2),
        &AtomicBool::new(false),
    )?;
    if !result.status.success()
        || has_document_mount(&fs::read_to_string("/proc/self/mountinfo")?, &mount)
    {
        return Err(io::Error::other(format!(
            "could not unmount owned document portal {}: {}",
            mount.display(),
            String::from_utf8_lossy(&result.stderr)
        )));
    }
    Ok(())
}

fn has_document_mount(mounts: &str, target: &Path) -> bool {
    mounts.lines().any(|line| {
        let Some((fields, filesystem)) = line.split_once(" - ") else {
            return false;
        };
        let Some(mount) = fields.split_whitespace().nth(4) else {
            return false;
        };
        filesystem.split_whitespace().next() == Some("fuse.portal")
            && decode_mount_path(mount) == target.as_os_str()
    })
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

    #[test]
    fn only_the_exact_owned_document_mount_matches() {
        let mounts = "36 25 0:32 / /run/user/1000/doc rw - fuse.portal portal rw\n37 25 0:33 / /run/user/1000/private/doc rw - fuse.portal portal rw\n";
        assert!(has_document_mount(
            mounts,
            Path::new("/run/user/1000/private/doc")
        ));
        assert!(!has_document_mount(
            mounts,
            Path::new("/run/user/1000/other/doc")
        ));
        assert!(!has_document_mount(
            &mounts.replace("fuse.portal", "ext4"),
            Path::new("/run/user/1000/private/doc")
        ));
    }

    #[test]
    fn mount_paths_with_spaces_are_decoded_without_prefix_matching() {
        let mounts = "37 25 0:33 / /tmp/private\\040session/doc rw - fuse.portal portal rw\n";
        assert!(has_document_mount(
            mounts,
            Path::new("/tmp/private session/doc")
        ));
        assert!(!has_document_mount(
            mounts,
            Path::new("/tmp/private session")
        ));
    }
}
