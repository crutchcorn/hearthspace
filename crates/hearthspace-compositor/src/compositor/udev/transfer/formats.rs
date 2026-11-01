use super::super::gpu::GpuApi;
use smithay::backend::{
    allocator::{Format, Fourcc, Modifier, dmabuf::Dmabuf},
    drm::DrmNode,
    renderer::{
        Bind, ImportDma,
        gles::GlesRenderer,
        multigpu::{ApiDevice, GpuManager},
    },
};

pub(super) fn direct_copy_format(
    manager: &mut GpuManager<GpuApi>,
    source: DrmNode,
    target: DrmNode,
    preferred: Fourcc,
) -> Result<Option<Fourcc>, Box<dyn std::error::Error>> {
    let mut render = Vec::new();
    let mut import = Vec::new();
    for device in manager.devices_mut()? {
        if *device.node() == source && device.should_do_cross_device_exports() {
            render = <GlesRenderer as Bind<Dmabuf>>::supported_formats(device.renderer())
                .unwrap_or_default()
                .into_iter()
                .collect();
        }
        if *device.node() == target && device.can_do_cross_device_imports() {
            import = device.renderer().dmabuf_formats().into_iter().collect();
        }
    }
    Ok(direct_format(preferred, &render, &import))
}

fn direct_format(preferred: Fourcc, render: &[Format], import: &[Format]) -> Option<Fourcc> {
    [
        preferred,
        Fourcc::Argb8888,
        Fourcc::Xrgb8888,
        Fourcc::Abgr8888,
        Fourcc::Xbgr8888,
    ]
    .into_iter()
    .find(|code| {
        render.iter().any(|format| {
            format.code == *code && format.modifier != Modifier::Invalid && import.contains(format)
        })
    })
}

pub(super) fn transfer_format(scanout: Fourcc, source_formats: &[Fourcc]) -> Option<Fourcc> {
    // The intermediate is allocated on the source GPU; it need not have the
    // target's scanout format. Smithay negotiates DMA import or CPU fallback.
    [
        scanout,
        Fourcc::Argb8888,
        Fourcc::Xrgb8888,
        Fourcc::Abgr8888,
        Fourcc::Xbgr8888,
    ]
    .into_iter()
    .find(|format| source_formats.contains(format))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn format(code: Fourcc, modifier: Modifier) -> Format {
        Format { code, modifier }
    }

    #[test]
    fn reverse_direct_route_wins_even_when_scanout_format_has_no_intersection() {
        // AMD renders Linear ARGB; NVIDIA can sample it. XR24 scanout must not
        // select CPU fallback just because only a different copy Fourcc intersects.
        let amd = [format(Fourcc::Argb8888, Modifier::Linear)];
        let nvidia = [format(Fourcc::Argb8888, Modifier::Linear)];
        assert_eq!(
            direct_format(Fourcc::Xrgb8888, &amd, &nvidia),
            Some(Fourcc::Argb8888)
        );
    }

    #[test]
    fn disjoint_explicit_modifiers_and_implicit_only_advertisements_do_not_claim_direct() {
        let source = [format(Fourcc::Abgr8888, Modifier::from(0x0300000000606013))];
        let target = [format(Fourcc::Abgr8888, Modifier::Linear)];
        assert_eq!(direct_format(Fourcc::Abgr8888, &source, &target), None);
        let implicit = [format(Fourcc::Abgr8888, Modifier::Invalid)];
        assert_eq!(direct_format(Fourcc::Abgr8888, &implicit, &implicit), None);
    }

    #[test]
    fn target_scanout_format_does_not_require_source_framebuffer_support() {
        assert_eq!(
            transfer_format(Fourcc::Xrgb8888, &[Fourcc::Argb8888]),
            Some(Fourcc::Argb8888)
        );
        assert_eq!(
            transfer_format(Fourcc::Xrgb8888, &[Fourcc::Argb8888, Fourcc::Xrgb8888]),
            Some(Fourcc::Xrgb8888)
        );
        assert_eq!(transfer_format(Fourcc::Xrgb8888, &[]), None);
    }
}
