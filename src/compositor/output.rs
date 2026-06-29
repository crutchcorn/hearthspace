use smithay::{
    output::{Mode, Output, PhysicalProperties, Scale, Subpixel},
    utils::{Logical, Physical, Point, Size, Transform},
};
use wayland_server::{DisplayHandle, backend::GlobalId, protocol::wl_surface::WlSurface};

use super::App;

pub(in crate::compositor) struct OutputSet {
    primary: OutputRecord,
    secondary: Vec<OutputRecord>,
}

pub(in crate::compositor) struct OutputRecord {
    #[cfg_attr(not(feature = "udev"), allow(dead_code))]
    name: String,
    output: Output,
    #[cfg_attr(not(feature = "udev"), allow(dead_code))]
    global_id: GlobalId,
    pub(in crate::compositor) size: Size<i32, Physical>,
    scale: i32,
    refresh: i32,
    location: Point<i32, Logical>,
}

pub(in crate::compositor) struct OutputDescriptor {
    pub(in crate::compositor) name: String,
    pub(in crate::compositor) properties: PhysicalProperties,
    pub(in crate::compositor) size: Size<i32, Physical>,
    pub(in crate::compositor) scale: i32,
    pub(in crate::compositor) refresh: i32,
}

impl OutputSet {
    pub(in crate::compositor) fn new(primary: OutputRecord) -> Self {
        Self {
            primary,
            secondary: Vec::new(),
        }
    }

    #[cfg(feature = "udev")]
    fn sync_secondary_outputs(&mut self, dh: &DisplayHandle, descriptors: Vec<OutputDescriptor>) {
        let mut next_x = self.primary.size.to_logical(self.primary.scale).w;
        let mut existing = std::mem::take(&mut self.secondary);
        let mut next_secondary = Vec::with_capacity(existing.len());

        for descriptor in descriptors {
            if descriptor.name == self.primary.name {
                continue;
            }

            let location = (next_x, 0).into();
            let output = if let Some(index) = existing
                .iter()
                .position(|output| output.name == descriptor.name)
            {
                let mut output = existing.remove(index);
                output.update(descriptor, location);
                output
            } else {
                let output = create_output_record(dh, descriptor, location);
                println!(
                    "Advertising newly connected Wayland output {} at {:?}",
                    output.name, location
                );
                output
            };
            next_x += output.size.to_logical(output.scale).w;
            next_secondary.push(output);
        }

        for removed in existing {
            println!("Disabling disconnected Wayland output {}", removed.name);
            dh.disable_global::<App>(removed.global_id);
        }

        self.secondary = next_secondary;
    }

    fn logical_size(&self) -> Size<i32, Logical> {
        let primary_size = self.primary.logical_size();
        let mut width = primary_size.w;
        let mut height = primary_size.h;
        for output in &self.secondary {
            let size = output.logical_size();
            width = width.max(output.location.x + size.w);
            height = height.max(output.location.y + size.h);
        }
        Size::from((width.max(1), height.max(1)))
    }
}

impl OutputRecord {
    fn logical_size(&self) -> Size<i32, Logical> {
        self.size.to_logical(self.scale)
    }

    #[cfg(feature = "udev")]
    fn update(&mut self, descriptor: OutputDescriptor, location: Point<i32, Logical>) {
        self.size = descriptor.size;
        self.scale = descriptor.scale;
        self.refresh = descriptor.refresh;
        self.location = location;
        update_output_mode_with_refresh_at(
            &self.output,
            self.size,
            self.scale,
            self.refresh,
            self.location,
        );
    }
}

impl App {
    pub(super) fn output_size(&self) -> Size<i32, Physical> {
        self.outputs.primary.size
    }

    pub(super) fn output_logical_size(&self) -> Size<i32, Logical> {
        self.outputs.logical_size()
    }

    #[cfg_attr(not(feature = "winit"), allow(dead_code))]
    pub(super) fn set_primary_output_size(&mut self, size: Size<i32, Physical>) {
        self.outputs.primary.size = size;
        self.outputs.primary.refresh = 60_000;
        self.outputs.primary.location = (0, 0).into();
        update_output_mode(
            &self.outputs.primary.output,
            size,
            self.outputs.primary.scale,
        );
    }

    pub(super) fn enter_primary_output(&self, surface: &WlSurface) {
        self.outputs.primary.output.enter(surface);
    }

    pub(super) fn cleanup_outputs(&mut self) {
        self.outputs.primary.output.cleanup();
        for output in &mut self.outputs.secondary {
            output.output.cleanup();
        }
    }

    #[cfg(feature = "udev")]
    pub(in crate::compositor) fn sync_connector_outputs(
        &mut self,
        dh: &DisplayHandle,
        descriptors: Vec<OutputDescriptor>,
    ) {
        self.outputs.sync_secondary_outputs(dh, descriptors);
    }
}

pub(in crate::compositor) fn create_output(
    dh: &DisplayHandle,
    size: Size<i32, Physical>,
    scale: i32,
) -> OutputRecord {
    create_output_record(
        dh,
        OutputDescriptor {
            name: "hearthspace-0".into(),
            properties: PhysicalProperties {
                size: (340, 190).into(),
                subpixel: Subpixel::Unknown,
                make: "Hearthspace".into(),
                model: "Nested Canvas".into(),
            },
            size,
            scale,
            refresh: 60_000,
        },
        (0, 0).into(),
    )
}

#[cfg_attr(not(feature = "udev"), allow(dead_code))]
pub(in crate::compositor) fn create_output_with_properties(
    dh: &DisplayHandle,
    name: String,
    properties: PhysicalProperties,
    size: Size<i32, Physical>,
    scale: i32,
    refresh: i32,
) -> OutputRecord {
    create_output_record(
        dh,
        OutputDescriptor {
            name,
            properties,
            size,
            scale,
            refresh,
        },
        (0, 0).into(),
    )
}

fn create_output_record(
    dh: &DisplayHandle,
    descriptor: OutputDescriptor,
    location: Point<i32, Logical>,
) -> OutputRecord {
    let OutputDescriptor {
        name,
        properties,
        size,
        scale,
        refresh,
    } = descriptor;
    let output = Output::new(name.clone(), properties);
    let global_id = output.create_global::<App>(dh);
    update_output_mode_with_refresh_at(&output, size, scale, refresh, location);
    OutputRecord {
        name,
        output,
        global_id,
        size,
        scale,
        refresh,
        location,
    }
}

#[cfg_attr(not(feature = "winit"), allow(dead_code))]
fn update_output_mode(output: &Output, size: Size<i32, Physical>, scale: i32) {
    update_output_mode_with_refresh(output, size, scale, 60_000);
}

fn update_output_mode_with_refresh(
    output: &Output,
    size: Size<i32, Physical>,
    scale: i32,
    refresh: i32,
) {
    update_output_mode_with_refresh_at(output, size, scale, refresh, (0, 0).into());
}

fn update_output_mode_with_refresh_at(
    output: &Output,
    size: Size<i32, Physical>,
    scale: i32,
    refresh: i32,
    location: Point<i32, Logical>,
) {
    let mode = Mode { size, refresh };
    output.set_preferred(mode);
    output.change_current_state(
        Some(mode),
        Some(Transform::Normal),
        Some(Scale::Integer(scale)),
        Some(location),
    );
}
