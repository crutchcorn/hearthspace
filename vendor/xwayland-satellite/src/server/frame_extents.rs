//! GTK's X pixel insets become host XDG geometry, not canvas coordinates.
use super::*;
use wayland_protocols::wp::viewporter::client::wp_viewport::WpViewport;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Extents([u16; 4]);

impl Extents {
    pub(crate) fn parse(values: &[u32]) -> Option<Self> {
        Some(Self([
            u16::try_from(*values.first()?).ok()?,
            u16::try_from(*values.get(1)?).ok()?,
            u16::try_from(*values.get(2)?).ok()?,
            u16::try_from(*values.get(3)?).ok()?,
        ]))
    }
}

fn effective(attrs: &WindowAttributes) -> [u16; 4] {
    attrs.frame_extents.map_or([0; 4], |Extents(v)| {
        if u32::from(v[0]) + u32::from(v[1]) < u32::from(attrs.dims.width)
            && u32::from(v[2]) + u32::from(v[3]) < u32::from(attrs.dims.height)
        {
            v
        } else {
            [0; 4]
        }
    })
}

pub(super) fn padding(attrs: &WindowAttributes) -> (u16, u16) {
    let [left, right, top, bottom] = effective(attrs);
    (left + right, top + bottom)
}

pub(super) fn geometry(attrs: &WindowAttributes, scale: f64) -> (i32, i32, i32, i32) {
    let [left, right, top, bottom] = effective(attrs);
    let x = (f64::from(left) / scale).round() as i32;
    let y = (f64::from(top) / scale).round() as i32;
    let end_x = (f64::from(attrs.dims.width - right) / scale).ceil() as i32;
    let end_y = (f64::from(attrs.dims.height - bottom) / scale).ceil() as i32;
    (x, y, (end_x - x).max(1), (end_y - y).max(1))
}

impl<S: X11Selection + 'static> InnerServerState<S> {
    pub(crate) fn set_frame_extents(&mut self, window: x::Window, extents: Option<Extents>) {
        let Some(&entity) = self.windows.get(&window) else {
            return;
        };
        {
            let mut data = self.world.get::<&mut WindowData>(entity).unwrap();
            if data.attrs.frame_extents == extents {
                return;
            }
            data.attrs.frame_extents = extents;
        }
        if self.world.get::<&WpViewport>(entity).is_ok() {
            event::update_surface_viewport(&self.world, self.world.query_one(entity).unwrap());
            self.world
                .get::<&client::wl_surface::WlSurface>(entity)
                .unwrap()
                .commit();
        }
    }
}
