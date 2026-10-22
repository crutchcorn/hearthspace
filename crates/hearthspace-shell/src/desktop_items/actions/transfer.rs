//! Copy into a temporary sibling and publish only complete trees, without overwriting.
use std::{
    fs, io,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

pub fn paste(paths: &[PathBuf], directory: &Path, cut: bool) -> Result<(), String> {
    let directory = directory.canonicalize().map_err(|e| e.to_string())?;
    if !directory.is_dir() {
        return Err("The destination is not a folder.".into());
    }
    for path in paths {
        let metadata =
            fs::symlink_metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let parent = path
            .parent()
            .ok_or("The source has no parent folder.")?
            .canonicalize()
            .map_err(|e| e.to_string())?;
        if cut && parent == directory {
            continue;
        }
        if metadata.is_dir()
            && directory.starts_with(path.canonicalize().map_err(|e| e.to_string())?)
        {
            return Err("A folder cannot be copied into itself.".into());
        }
        let name = path.file_name().ok_or("The source has no file name.")?;
        let destination = available_destination(&directory, name, metadata.is_dir())
            .map_err(|e| format!("Could not choose a destination: {e}"))?;
        if cut {
            match rename_without_replace(path, &destination) {
                Ok(()) => continue,
                Err(error) if error.kind() == io::ErrorKind::CrossesDevices => {}
                Err(error) => return Err(format!("Could not move {}: {error}", path.display())),
            }
        }
        copy_and_publish(path, &directory, &destination)
            .map_err(|e| format!("Could not copy {}: {e}", path.display()))?;
        if cut {
            remove(path).map_err(|e| {
                format!(
                    "Copied {}, but could not remove the original: {e}",
                    path.display()
                )
            })?;
        }
    }
    Ok(())
}

pub(super) fn rename_without_replace(source: &Path, destination: &Path) -> io::Result<()> {
    rustix::fs::renameat_with(
        rustix::fs::CWD,
        source,
        rustix::fs::CWD,
        destination,
        rustix::fs::RenameFlags::NOREPLACE,
    )
    .map_err(Into::into)
}

fn copy_and_publish(source: &Path, directory: &Path, destination: &Path) -> io::Result<()> {
    // Own the staging directory before writing anything or scheduling cleanup.
    // A predictable sibling could already belong to the user (or be a symlink).
    let staging = tempfile::Builder::new()
        .prefix(".hearthspace-copy-")
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir_in(directory)?;
    let payload = staging.path().join("item");
    copy_tree(source, &payload, 0)?;
    rename_without_replace(&payload, destination)
}

fn available_destination(
    directory: &Path,
    name: &std::ffi::OsStr,
    is_directory: bool,
) -> io::Result<PathBuf> {
    let initial = directory.join(name);
    if missing(&initial)? {
        return Ok(initial);
    }
    let path = Path::new(name);
    for number in 1..10_000 {
        let mut candidate = if is_directory {
            name
        } else {
            path.file_stem().unwrap_or(name)
        }
        .to_os_string();
        candidate.push(if number == 1 {
            " (copy)".into()
        } else {
            format!(" (copy {number})")
        });
        if let Some(extension) = path.extension().filter(|_| !is_directory) {
            candidate.push(".");
            candidate.push(extension);
        }
        let candidate = directory.join(candidate);
        if missing(&candidate)? {
            return Ok(candidate);
        }
    }
    Err(io::Error::other("There are too many files with this name."))
}

fn missing(path: &Path) -> io::Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(false),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(true),
        Err(error) => Err(error),
    }
}

fn copy_tree(source: &Path, target: &Path, depth: usize) -> io::Result<()> {
    if depth > 128 {
        return Err(io::Error::other("Folder nesting is too deep"));
    }
    let metadata = fs::symlink_metadata(source)?;
    if metadata.is_symlink() {
        return std::os::unix::fs::symlink(fs::read_link(source)?, target);
    }
    if metadata.is_dir() {
        fs::create_dir(target)?;
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            copy_tree(&entry.path(), &target.join(entry.file_name()), depth + 1)?;
        }
        fs::set_permissions(target, metadata.permissions())
    } else if metadata.is_file() {
        let mut source = fs::File::open(source)?;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(target)?;
        io::copy(&mut source, &mut file)?;
        file.set_permissions(metadata.permissions())
    } else {
        Err(io::Error::other("This file type cannot be copied"))
    }
}

fn remove(path: &Path) -> io::Result<()> {
    if fs::symlink_metadata(path)?.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn copying_a_tree_preserves_originals_links_and_existing_destination() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("album");
        let target = root.path().join("desktop");
        fs::create_dir_all(source.join("disc")).unwrap();
        fs::create_dir(&target).unwrap();
        fs::write(source.join("disc/song.wav"), b"music").unwrap();
        std::os::unix::fs::symlink("disc/song.wav", source.join("favorite")).unwrap();
        paste(std::slice::from_ref(&source), &target, false).unwrap();
        paste(std::slice::from_ref(&source), &target, false).unwrap();
        assert_eq!(
            fs::read(target.join("album/disc/song.wav")).unwrap(),
            b"music"
        );
        assert_eq!(
            fs::read_link(target.join("album/favorite")).unwrap(),
            Path::new("disc/song.wav")
        );
        assert!(target.join("album (copy)/disc/song.wav").exists());
        assert!(source.join("disc/song.wav").exists());
    }
    #[test]
    fn cut_moves_and_self_copy_leaves_source_intact() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("album");
        let target = root.path().join("desktop");
        fs::create_dir(&source).unwrap();
        fs::create_dir(&target).unwrap();
        fs::write(source.join("song"), b"music").unwrap();
        assert!(paste(std::slice::from_ref(&source), &source, false).is_err());
        paste(std::slice::from_ref(&source), &target, true).unwrap();
        assert!(!source.exists());
        assert_eq!(fs::read(target.join("album/song")).unwrap(), b"music");
    }

    #[test]
    fn failed_copy_does_not_publish_partial_content_or_remove_unowned_staging() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        let target = root.path().join("desktop");
        fs::create_dir(&source).unwrap();
        fs::create_dir(&target).unwrap();
        fs::write(source.join("notes"), b"keep the original").unwrap();
        let _socket = std::os::unix::net::UnixListener::bind(source.join("socket")).unwrap();
        let existing = target.join(format!(".hearthspace-copy-{}-1", std::process::id()));
        fs::create_dir(&existing).unwrap();
        fs::write(existing.join("notes"), b"unrelated user data").unwrap();

        assert!(paste(std::slice::from_ref(&source), &target, false).is_err());
        assert_eq!(
            fs::read(source.join("notes")).unwrap(),
            b"keep the original"
        );
        assert_eq!(
            fs::read(existing.join("notes")).unwrap(),
            b"unrelated user data"
        );
        assert_eq!(fs::read_dir(&target).unwrap().count(), 1);
    }

    #[test]
    fn a_destination_occupied_before_publication_is_never_replaced() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        let target = root.path().join("desktop");
        fs::create_dir(&target).unwrap();
        fs::write(&source, b"source").unwrap();
        let destination = target.join("source");
        std::os::unix::fs::symlink("missing", &destination).unwrap();

        assert!(copy_and_publish(&source, &target, &destination).is_err());
        assert_eq!(fs::read_link(&destination).unwrap(), Path::new("missing"));
        assert_eq!(fs::read(&source).unwrap(), b"source");
        assert_eq!(fs::read_dir(&target).unwrap().count(), 1);
    }

    #[test]
    fn copying_and_cutting_directory_links_preserves_the_referenced_directory() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("original");
        let target = root.path().join("desktop");
        let link = root.path().join("shortcut");
        fs::create_dir(&source).unwrap();
        fs::create_dir(&target).unwrap();
        fs::write(source.join("notes"), b"keep me").unwrap();
        std::os::unix::fs::symlink(&source, &link).unwrap();

        paste(std::slice::from_ref(&link), &target, false).unwrap();
        paste(std::slice::from_ref(&link), &target, true).unwrap();
        assert!(fs::symlink_metadata(&link).is_err());
        assert_eq!(fs::read_link(target.join("shortcut")).unwrap(), source);
        assert_eq!(
            fs::read_link(target.join("shortcut (copy)")).unwrap(),
            source
        );
        assert_eq!(fs::read(source.join("notes")).unwrap(), b"keep me");
    }

    #[test]
    fn duplicate_folder_names_do_not_treat_dots_as_file_extensions() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("project.v1");
        fs::create_dir(&source).unwrap();
        paste(std::slice::from_ref(&source), root.path(), false).unwrap();
        assert!(root.path().join("project.v1 (copy)").is_dir());
        assert!(source.is_dir());
    }
}
