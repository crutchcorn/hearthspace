use std::{
    collections::HashMap,
    io,
    path::{Path, PathBuf},
};

/// The persisted MonitorConfig.id must identify the same physical connector in
/// a fresh session, regardless of discovery order or which other GPUs are live.
#[derive(Default)]
pub(super) struct ConnectorNames {
    assigned: HashMap<(String, String), String>,
}

impl ConnectorNames {
    pub(super) fn assign(&mut self, device: &str, connector: &str) -> String {
        let key = (device.to_owned(), connector.to_owned());
        self.assigned
            .entry(key)
            .or_insert_with(|| {
                format!(
                    "{}@{}",
                    qualified_component(connector),
                    qualified_component(device)
                )
            })
            .clone()
    }
}

/// Only the resolved physical device path identifies a GPU across card-number
/// reassignment. Missing sysfs identity must retry opening, never persist cardN.
pub(super) fn device_identity(path: &Path) -> io::Result<String> {
    resolve_device_identity(path, |sysfs| std::fs::canonicalize(sysfs))
}

fn resolve_device_identity(
    path: &Path,
    resolve: impl FnOnce(&Path) -> io::Result<PathBuf>,
) -> io::Result<String> {
    let card = path.file_name().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("DRM device path {} has no device name", path.display()),
        )
    })?;
    let sysfs = Path::new("/sys/class/drm").join(card).join("device");
    let identity = resolve(&sysfs).map_err(|error| {
        io::Error::new(
            error.kind(),
            format!(
                "physical DRM identity unavailable for {} via {}: {error}",
                path.display(),
                sysfs.display()
            ),
        )
    })?;
    identity.into_os_string().into_string().map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "physical DRM identity for {} via {} is not valid UTF-8",
                path.display(),
                sysfs.display()
            ),
        )
    })
}

fn qualified_component(value: &str) -> String {
    value
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || b"-_.:".contains(&byte) {
                char::from(byte).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct DeviceToken {
    pub(super) id: u64,
    pub(super) incarnation: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn physical_identity_survives_card_reassignment_without_devnode_fallback() {
        let physical = PathBuf::from("/sys/devices/pci0000:00/0000:01:00.0");
        for card in ["card0", "card7"] {
            let path = Path::new("/dev/dri").join(card);
            let identity = resolve_device_identity(&path, |sysfs| {
                assert_eq!(sysfs, Path::new("/sys/class/drm").join(card).join("device"));
                Ok(physical.clone())
            })
            .unwrap();
            assert_eq!(identity, physical.to_str().unwrap());
        }
        for kind in [io::ErrorKind::NotFound, io::ErrorKind::PermissionDenied] {
            let error = resolve_device_identity(Path::new("/dev/dri/card7"), |_| {
                Err(io::Error::new(kind, "identity lookup failed"))
            })
            .unwrap_err();
            assert_eq!(error.kind(), kind);
            assert!(
                error
                    .to_string()
                    .contains("physical DRM identity unavailable")
            );
            assert!(error.to_string().contains("/dev/dri/card7"));
            assert!(error.to_string().contains("/sys/class/drm/card7/device"));
        }
    }

    #[test]
    fn physical_identity_rejects_lossy_path_aliases() {
        use std::{ffi::OsString, os::unix::ffi::OsStringExt};
        let error = resolve_device_identity(Path::new("/dev/dri/card0"), |_| {
            Ok(PathBuf::from(OsString::from_vec(
                b"/sys/devices/\xff".to_vec(),
            )))
        })
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn duplicate_connector_does_not_rename_live_or_disconnected_output() {
        let mut names = ConnectorNames::default();
        assert_eq!(names.assign("pci-a", "DP-1"), "DP-1@pci-a");
        assert_eq!(names.assign("pci-b", "DP-1"), "DP-1@pci-b");
        assert_eq!(names.assign("pci-a", "DP-1"), "DP-1@pci-a");
        assert_eq!(names.assign("pci-b", "DP-1"), "DP-1@pci-b");
        assert_eq!(names.assign("pci-b", "eDP-1"), "eDP-1@pci-b");
    }

    #[test]
    fn qualified_name_also_cannot_steal_an_existing_name() {
        let mut names = ConnectorNames::default();
        let first = names.assign("a", "DP-1@b");
        assert_eq!(first, "DP-1%40b@a");
        assert_eq!(names.assign("b", "DP-1"), "DP-1@b");
        assert_ne!(names.assign("b@a", "DP-1"), first);
    }

    #[test]
    fn recycled_device_id_does_not_accept_old_notifier() {
        let old = DeviceToken {
            id: 226,
            incarnation: 1,
        };
        let replacement = DeviceToken {
            id: 226,
            incarnation: 2,
        };
        let other_gpu = DeviceToken {
            id: 227,
            incarnation: 1,
        };
        assert_ne!(old, replacement);
        assert_ne!(old, other_gpu);
    }

    #[test]
    fn topology_paths_cannot_alias_through_name_escaping() {
        let mut names = ConnectorNames::default();
        names.assign("original", "DP-1");
        let a = names.assign("/devices/a-b/c", "DP-1");
        let b = names.assign("/devices/a/b-c", "DP-1");
        let literal_escape = names.assign("%2Fdevices/a-b/c", "DP-1");
        assert_ne!(a, b);
        assert_ne!(a, literal_escape);
        assert_eq!(names.assign("/devices/a-b/c", "DP-1"), a);
    }

    #[test]
    fn fresh_reversed_discovery_preserves_retained_monitor_preferences() {
        use hearthspace_common::output_identity::{SavedOutput, resolve_preferences};
        use hearthspace_ipc::MonitorConfig;

        let mut first_session = ConnectorNames::default();
        let b = first_session.assign("/devices/pci-b", "DP-1");
        let a = first_session.assign("/devices/pci-a", "DP-1");
        let preferences = [(b.clone(), 2), (a.clone(), 3)].map(|(id, scale)| MonitorConfig {
            id,
            name: "DP-1".into(),
            width: 1920,
            height: 1080,
            scale,
            x: 0,
            y: 0,
        });
        // Exercise the persisted preference type, not just a session-local map.
        let stored = serde_json::to_string(&preferences).unwrap();
        let retained: Vec<MonitorConfig> = serde_json::from_str(&stored).unwrap();
        let mut next_session = ConnectorNames::default();
        let next_a = next_session.assign("/devices/pci-a", "DP-1");
        let next_b = next_session.assign("/devices/pci-b", "DP-1");
        assert_eq!(next_a, a);
        assert_eq!(next_b, b);
        let saved = retained
            .iter()
            .map(|monitor| SavedOutput {
                id: &monitor.id,
                name: &monitor.name,
                placeholder: false,
            })
            .collect::<Vec<_>>();
        let selected = resolve_preferences(&saved, &[&next_a, &next_b], Some(&next_a));
        assert_eq!(selected, [Some(1), Some(0)]);
        assert_eq!(retained[selected[0].unwrap()].scale, 3);
        assert_eq!(retained[selected[1].unwrap()].scale, 2);
        // Disconnecting B cannot make its friendly name a preference for A.
        assert_eq!(
            resolve_preferences(&saved[..1], &[&next_a], Some(&next_a)),
            [None]
        );
    }

    #[test]
    fn qualified_identity_roundtrips_shell_app_ids_and_output_commands() {
        use hearthspace_ipc::command::ShellCommand;
        let id = ConnectorNames::default().assign("/devices/pci0000:00/0000:01:00.0", "DP-1");
        let app_id = hearthspace_common::shell_bar_app_id_for_output(&id);
        assert_eq!(
            hearthspace_common::shell_bar_output_name_from_app_id(&app_id),
            Some(Some(id.clone()))
        );
        let command = ShellCommand::LaunchAppOnOutput {
            app_id: "org.example.App".into(),
            output_name: id,
        };
        assert_eq!(ShellCommand::parse(&command.wire_name()), Some(command));
    }
}
