//! Pair the independent X11 and Wayland streams in either arrival order.
use super::*;

/// Window contents wait for a matching X window, mapping, and XDG configure.
/// Cursor and subsurface buffers continue through their existing paths.
pub(super) struct AwaitingWindow;

impl<S: X11Selection + 'static> InnerServerState<S> {
    pub fn set_window_serial(&mut self, window: x::Window, serial: [u32; 2]) {
        let Some(entity) = self.windows.get(&window).copied() else {
            warn!("Tried to set serial for unknown window {window:?}");
            return;
        };
        self.world
            .insert_one(entity, SurfaceSerial(serial))
            .unwrap();
        let surface = self
            .world
            .query::<(&SurfaceSerial, &WlSurface)>()
            .without::<&x::Window>()
            .iter()
            .find(|(_, (candidate, _))| candidate.0 == serial)
            .map(|(e, _)| e);
        if let Some(surface) = surface {
            self.associate_window_surface(entity, surface);
        }
    }

    pub(super) fn set_surface_serial(&mut self, surface: Entity, serial: [u32; 2]) {
        self.world
            .insert_one(surface, SurfaceSerial(serial))
            .unwrap();
        let window = self
            .world
            .query::<(&SurfaceSerial, &x::Window)>()
            .without::<&WlSurface>()
            .iter()
            .find(|(_, (candidate, _))| candidate.0 == serial)
            .map(|(e, _)| e);
        if let Some(window) = window {
            self.associate_window_surface(window, surface);
        }
    }

    fn associate_window_surface(&mut self, window: Entity, surface: Entity) {
        let bundle = self.world.take(window).unwrap();
        let mut builder = hecs::EntityBuilder::new();
        builder.add_bundle(bundle);
        self.world.insert(surface, builder.build()).unwrap();
        self.world.remove_one::<SurfaceSerial>(surface).unwrap();
        let window = *self.world.get::<&x::Window>(surface).unwrap();
        let id = self.world.get::<&WlSurface>(surface).unwrap().id();
        self.windows.insert(window, surface);
        debug!("associate {id} with {window:?}");
        self.create_mapped_window_role(window);
    }

    pub(super) fn create_mapped_window_role(&mut self, window: x::Window) {
        let Some(entity) = self.windows.get(&window).copied() else {
            return;
        };
        let data = self.world.entity(entity).unwrap();
        if !data.has::<WlSurface>()
            || data.has::<SurfaceRole>()
            || !data.get::<&WindowData>().unwrap().mapped
        {
            return;
        }
        if self.create_role_window(window, entity) {
            self.activate_window(window);
        }
    }
}
