//! Isolate the shell before compositor-owned startup in serialized tests.
use std::{env, ffi::OsString, fs, io};

pub(super) struct ShellEnvironment {
    saved: Vec<(&'static str, Option<OsString>)>,
    _directory: tempfile::TempDir,
}

impl ShellEnvironment {
    /// The caller must hold the integration-test environment lock until Drop.
    /// Start the private bus first so its accessibility service remains available.
    pub(super) fn new() -> io::Result<Self> {
        let directory = tempfile::tempdir()?;
        for name in ["config", "data", "state", "cache"] {
            fs::create_dir(directory.path().join(name))?;
        }
        // XDG specifies the home directory itself to disable a user directory.
        fs::write(
            directory.path().join("config/user-dirs.dirs"),
            "XDG_DESKTOP_DIR=\"$HOME\"\n",
        )?;
        let values = [
            ("XDG_CONFIG_HOME", "config"),
            ("XDG_CONFIG_DIRS", "config"),
            ("XDG_DATA_HOME", "data"),
            ("XDG_DATA_DIRS", "data"),
            ("XDG_STATE_HOME", "state"),
            ("XDG_CACHE_HOME", "cache"),
        ];
        let saved = values
            .iter()
            .map(|(key, _)| (*key, env::var_os(key)))
            .collect();
        for (key, subdirectory) in values {
            // SAFETY: the integration target serializes all tests with a lock
            // held through this guard's Drop, including child-process creation.
            unsafe { env::set_var(key, directory.path().join(subdirectory)) };
        }
        Ok(Self {
            saved,
            _directory: directory,
        })
    }
}

impl Drop for ShellEnvironment {
    fn drop(&mut self) {
        for (key, value) in &self.saved {
            // SAFETY: the caller still holds the integration-test lock.
            unsafe {
                match value {
                    Some(value) => env::set_var(key, value),
                    None => env::remove_var(key),
                }
            }
        }
    }
}
