use super::super::identity::ConnectorNames;
use smithay::{
    backend::drm::DrmDeviceFd,
    output::{PhysicalProperties, Subpixel},
    reexports::drm::control::{Device as ControlDevice, Mode, connector, crtc},
    utils::{Physical, Size},
};
use tracing::{debug, info, warn};

#[derive(Debug, Clone, PartialEq)]
pub(in crate::compositor::udev) struct KmsOutputTarget {
    pub(in crate::compositor::udev) connector: connector::Handle,
    pub(in crate::compositor::udev) crtc: crtc::Handle,
    pub(in crate::compositor::udev) mode: Mode,
    connector_name: String,
    connector_label: String,
    physical_size_mm: (i32, i32),
}

impl KmsOutputTarget {
    pub(in crate::compositor::udev) fn output_size(&self) -> Size<i32, Physical> {
        let (width, height) = self.mode.size();
        Size::from((i32::from(width), i32::from(height)))
    }

    pub(in crate::compositor::udev) fn output_descriptor(
        &self,
    ) -> crate::compositor::OutputDescriptor {
        crate::compositor::OutputDescriptor {
            name: self.connector_name.clone(),
            properties: PhysicalProperties {
                size: self.physical_size_mm.into(),
                subpixel: Subpixel::Unknown,
                make: "DRM".into(),
                model: self.connector_label.clone(),
                serial_number: self.connector_name.clone(),
            },
            size: self.output_size(),
            scale: 1,
            refresh: self.refresh_millihz(),
        }
    }

    pub(in crate::compositor::udev) fn connector_name(&self) -> &str {
        &self.connector_name
    }

    fn refresh_millihz(&self) -> i32 {
        let refresh_hz = self.mode.vrefresh();
        if refresh_hz == 0 {
            return 60_000;
        }
        i32::try_from(refresh_hz.saturating_mul(1000)).unwrap_or(60_000)
    }
}

pub(in crate::compositor::udev) fn discover_outputs(
    drm_fd: &DrmDeviceFd,
    path: &std::path::Path,
    identity: &str,
    names: &mut ConnectorNames,
) -> Result<(Vec<KmsOutputTarget>, bool), Box<dyn std::error::Error>> {
    let resources = drm_fd.resource_handles()?;

    let crtcs = resources.crtcs();
    debug!(
        path = %path.display(),
        connector_count = resources.connectors().len(),
        crtc_count = crtcs.len(),
        "DRM device resources queried"
    );

    let mut targets = Vec::new();
    let mut complete = true;
    let mut used_crtcs = Vec::new();
    for connector_handle in resources.connectors() {
        let info = drm_fd.get_connector(*connector_handle, true)?;
        let connected = info.state() == connector::State::Connected;
        if connected && info.modes().is_empty() {
            complete = false;
        }
        debug!(
            connector = ?connector_handle,
            interface = ?info.interface(),
            connected,
            mode_count = info.modes().len(),
            "DRM connector queried"
        );
        if connected && let Some(mode) = info.modes().first() {
            debug!(connector = ?connector_handle, ?mode, "selected preferred/first DRM mode");
            let current_crtc = info
                .current_encoder()
                .and_then(|encoder| drm_fd.get_encoder(encoder).ok())
                .and_then(|encoder| encoder.crtc());
            let crtc_candidates =
                info.encoders()
                    .iter()
                    .filter_map(|encoder| {
                        drm_fd.get_encoder(*encoder).ok().map(|encoder_info| {
                            resources.filter_crtcs(encoder_info.possible_crtcs())
                        })
                    })
                    .flatten()
                    .collect::<Vec<_>>();
            let crtc = current_crtc
                .filter(|crtc| !used_crtcs.contains(crtc))
                .or_else(|| {
                    crtc_candidates
                        .iter()
                        .copied()
                        .find(|crtc| !used_crtcs.contains(crtc))
                });
            debug!(
                connector = ?connector_handle,
                current_crtc = ?current_crtc,
                crtcs = ?crtc_candidates,
                selected_crtc = ?crtc,
                "DRM CRTC candidates"
            );
            if let Some(crtc) = crtc {
                used_crtcs.push(crtc);
                let physical_size_mm = info
                    .size()
                    .map(|(width, height)| {
                        (
                            i32::try_from(width).unwrap_or(0),
                            i32::try_from(height).unwrap_or(0),
                        )
                    })
                    .unwrap_or((0, 0));
                let connector_label =
                    format!("{}-{}", info.interface().as_str(), info.interface_id());
                targets.push(KmsOutputTarget {
                    connector: *connector_handle,
                    crtc,
                    mode: *mode,
                    connector_name: names.assign(identity, &connector_label),
                    connector_label,
                    physical_size_mm,
                });
            } else {
                complete = false;
                warn!(connector = ?connector_handle, "connected DRM output has no usable CRTC");
            }
        }
    }
    if let Some(target) = targets.first() {
        info!(connector = ?target.connector, crtc = ?target.crtc, mode = ?target.mode, "selected KMS output target");
    } else {
        warn!("no connected DRM connector with a usable CRTC was found");
    }
    Ok((targets, complete))
}
