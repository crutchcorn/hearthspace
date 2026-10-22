//! Native file identities shared by desktop drag-and-drop and clipboard actions.
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use url::Url;

pub const URI_LIST: &str = "text/uri-list";
pub const COPIED_FILES: &str = "x-special/gnome-copied-files";
pub const MAX_FILE_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FileClipboard {
    pub cut: bool,
    pub uris: Vec<String>,
    /// Identifies a compositor-owned selection; external file lists never have one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generation: Option<u64>,
}

pub fn file_uri(path: &Path) -> Result<String, String> {
    Url::from_file_path(path)
        .map(String::from)
        .map_err(|()| "File path must be absolute.".into())
}

pub fn local_path(uri: &str) -> Result<PathBuf, String> {
    if uri.chars().any(char::is_control) {
        return Err("File URI contains unescaped control characters.".into());
    }
    let url = Url::parse(uri).map_err(|_| "Invalid file URI.")?;
    if url.scheme() != "file" || url.query().is_some() || url.fragment().is_some() {
        return Err("Only local file URIs are supported.".into());
    }
    let path = url
        .to_file_path()
        .map_err(|()| "Only local file URIs are supported.")?;
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        if path.as_os_str().as_bytes().contains(&0) {
            return Err("File path contains NUL.".into());
        }
    }
    Ok(path)
}

impl FileClipboard {
    pub fn validate(&self) -> Result<(), String> {
        if self.uris.is_empty() || self.uris.len() > crate::MAX_DESKTOP_ITEMS {
            return Err("No files or too many files were selected.".into());
        }
        let mut bytes = 5usize;
        for uri in &self.uris {
            local_path(uri)?;
            bytes = bytes.saturating_add(uri.len() + 2);
        }
        if bytes > MAX_FILE_BYTES {
            return Err("File selection is too large.".into());
        }
        Ok(())
    }

    pub fn encode(&self, mime: &str) -> Option<Vec<u8>> {
        let text = match mime {
            URI_LIST => format!("{}\r\n", self.uris.join("\r\n")),
            COPIED_FILES => format!(
                "{}\n{}",
                if self.cut { "cut" } else { "copy" },
                self.uris.join("\n")
            ),
            _ => return None,
        };
        Some(text.into_bytes())
    }

    pub fn decode(mime: &str, bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > MAX_FILE_BYTES {
            return Err("Clipboard file selection is too large.".into());
        }
        let text = std::str::from_utf8(bytes).map_err(|_| "Clipboard file list is not UTF-8.")?;
        let mut lines = text.lines();
        let cut = match mime {
            COPIED_FILES => match lines.next() {
                Some("cut") => true,
                Some("copy") => false,
                _ => return Err("Invalid clipboard file action.".into()),
            },
            URI_LIST => false,
            _ => return Err("Clipboard does not contain files.".into()),
        };
        let uris = lines
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .map(str::to_owned)
            .collect();
        let value = Self {
            cut,
            uris,
            generation: None,
        };
        value.validate()?;
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_lists_preserve_spaces_symbols_unicode_and_native_bytes() {
        use std::{ffi::OsString, os::unix::ffi::OsStringExt};
        let path = PathBuf::from(OsString::from_vec(
            b"/tmp/Music #100%/caf\xc3\xa9\n\xff.ogg".to_vec(),
        ));
        let uri = file_uri(&path).unwrap();
        assert!(uri.contains("%20%23100%25"));
        assert_eq!(local_path(&uri).unwrap(), path);
        let files = FileClipboard {
            cut: true,
            uris: vec![uri, "file:///tmp/second.ogg".into()],
            generation: None,
        };
        assert_eq!(
            FileClipboard::decode(COPIED_FILES, &files.encode(COPIED_FILES).unwrap()).unwrap(),
            files
        );
        let copy = FileClipboard::decode(URI_LIST, &files.encode(URI_LIST).unwrap()).unwrap();
        assert!(!copy.cut);
        assert_eq!(copy.uris, files.uris);
    }

    #[test]
    fn external_file_lists_reject_remote_or_ambiguous_targets() {
        for uri in [
            "https://example.com/song",
            "file://remote/tmp/song",
            "file:///tmp/song#part",
            "file:///tmp/song?q",
            "file:///tmp/%00",
        ] {
            assert!(local_path(uri).is_err(), "{uri}");
        }
        let parsed = FileClipboard::decode(URI_LIST, b"# files\r\nfile:///tmp/a%20b\r\n").unwrap();
        assert_eq!(parsed.uris, ["file:///tmp/a%20b"]);
    }
}
