use anyhow::{Context, Result, bail, ensure};
use std::{
    os::unix::net::UnixStream,
    thread,
    time::{Duration, Instant},
};
use x11rb::{
    connection::Connection,
    protocol::{
        Event,
        xfixes::ConnectionExt as _,
        xinput::{self as xi, ConnectionExt as _},
        xproto::{
            ButtonPressEvent, ChangeWindowAttributesAux, ConnectionExt, CreateGCAux,
            CreateWindowAux, EventMask, GrabMode, GrabStatus, KeyPressEvent, MotionNotifyEvent,
            QueryPointerReply, Rectangle, WindowClass,
        },
    },
    rust_connection::RustConnection,
};

pub struct Clients {
    pub a: RustConnection,
    pub b: RustConnection,
    pub root: u32,
    pub primary: u32,
    pub neighbor: u32,
    pub child: u32,
    pub master: u16,
    pub relative: u16,
    pub hidden: bool,
    cursor: u32,
}

fn window(c: &RustConnection, parent: u32, x: i16, y: i16, w: u16, h: u16) -> Result<u32> {
    let id = c.generate_id()?;
    c.create_window(
        0,
        id,
        parent,
        x,
        y,
        w,
        h,
        0,
        WindowClass::INPUT_OUTPUT,
        0,
        &CreateWindowAux::new()
            .background_pixel(0x208040)
            .event_mask(
                EventMask::BUTTON_PRESS | EventMask::BUTTON_RELEASE | EventMask::POINTER_MOTION,
            ),
    )?
    .check()?;
    c.map_window(id)?.check()?;
    Ok(id)
}

fn visible_cursor(c: &RustConnection, root: u32, windows: [u32; 2]) -> Result<u32> {
    // A solid, opaque bitmap needs no cursor font or theme. Rootless Xwayland's
    // default may be null, which XFixesShowCursor cannot turn into a visible one.
    let bitmap = c.generate_id()?;
    let gc = c.generate_id()?;
    let cursor = c.generate_id()?;
    c.create_pixmap(1, bitmap, root, 8, 8)?.check()?;
    c.create_gc(gc, bitmap, &CreateGCAux::new().foreground(1))?
        .check()?;
    c.poly_fill_rectangle(
        bitmap,
        gc,
        &[Rectangle {
            x: 0,
            y: 0,
            width: 8,
            height: 8,
        }],
    )?
    .check()?;
    c.create_cursor(
        cursor,
        bitmap,
        bitmap,
        u16::MAX,
        u16::MAX,
        u16::MAX,
        0,
        0,
        0,
        0,
        0,
    )?
    .check()?;
    // CreateCursor copies the bits. Only the cursor must survive the scenario;
    // setup errors also release all these resources when connection A closes.
    c.free_gc(gc)?.check()?;
    c.free_pixmap(bitmap)?.check()?;
    for window in windows {
        c.change_window_attributes(window, &ChangeWindowAttributesAux::new().cursor(cursor))?
            .check()?;
    }
    Ok(cursor)
}

impl Clients {
    pub fn new() -> Result<Self> {
        let a = crate::xwm::connection(UnixStream::connect("x11")?)?;
        let b = crate::xwm::connection(UnixStream::connect("x11")?)?;
        let root = a.setup().roots[0].root;
        ensure!(
            root == b.setup().roots[0].root,
            "clients must share private X server"
        );
        // Same X root rectangle; only the fixture's wl_pointer focus distinguishes
        // them. Wm::pump maps the managed top-levels without reparenting/raising.
        let neighbor = window(&b, root, 0, 0, 320, 240)?;
        let primary = window(&a, root, 0, 0, 320, 240)?;
        let child = window(&a, primary, 100, 100, 50, 50)?;
        a.xfixes_query_version(5, 0)?.reply()?;
        // XI2.1+ delivers subscribed raw events even while another client grabs
        // the device. Negotiate on BOTH connections; B's raw control must be live.
        for connection in [&a, &b] {
            let version = connection.xinput_xi_query_version(2, 2)?.reply()?;
            ensure!(
                (version.major_version, version.minor_version) >= (2, 2),
                "XI2.2 is required on both clients"
            );
        }
        a.query_pointer(primary)?.reply()?;
        let pointer = a.xinput_xi_get_client_pointer(primary)?.reply()?;
        ensure!(pointer.set, "no core master pointer");
        b.query_pointer(neighbor)?.reply()?;
        let observer = b.xinput_xi_get_client_pointer(neighbor)?.reply()?;
        ensure!(
            observer.set && observer.deviceid == pointer.deviceid,
            "both clients must observe the same core master pointer"
        );
        let devices = a.xinput_xi_query_device(0u16)?.reply()?;
        ensure!(
            devices.infos.iter().any(|d| d.deviceid == pointer.deviceid
                && d.enabled
                && d.type_ == xi::DeviceType::MASTER_POINTER),
            "invalid core master pointer"
        );
        let relative = devices
            .infos
            .iter()
            .find(|d| {
                d.enabled
                    && d.type_ == xi::DeviceType::SLAVE_POINTER
                    && d.attachment == pointer.deviceid
                    && d.name.starts_with(b"xwayland-relative-pointer:")
            })
            .context("Xwayland did not create the genuine relative device")?
            .deviceid;
        let cursor = visible_cursor(&a, root, [primary, neighbor])?;
        Ok(Self {
            a,
            b,
            root,
            primary,
            neighbor,
            child,
            master: pointer.deviceid,
            relative,
            hidden: false,
            cursor,
        })
    }

    pub fn keyboard_marker(&self) -> Result<()> {
        let reply = self
            .b
            .grab_keyboard(
                false,
                self.neighbor,
                x11rb::CURRENT_TIME,
                GrabMode::ASYNC,
                GrabMode::ASYNC,
            )?
            .reply()?;
        ensure!(
            reply.status == GrabStatus::SUCCESS,
            "keyboard grab failed: {reply:?}"
        );
        Ok(())
    }

    pub fn hide_and_warp(&mut self) -> Result<()> {
        self.a.xfixes_hide_cursor(self.primary)?.check()?;
        self.hidden = true;
        // No pointer grab yet: Xwayland may lock the focused top-level. Starting
        // at 130 then warping to 140 guarantees this is a real WarpPointer request.
        self.a
            .warp_pointer(x11rb::NONE, self.primary, 0, 0, 0, 0, 140, 120)?
            .check()?;
        Ok(())
    }

    pub fn grab(&self) -> Result<()> {
        let reply = self
            .a
            .grab_pointer(
                false,
                self.primary,
                EventMask::POINTER_MOTION | EventMask::BUTTON_PRESS | EventMask::BUTTON_RELEASE,
                GrabMode::ASYNC,
                GrabMode::ASYNC,
                self.child,
                x11rb::NONE,
                x11rb::CURRENT_TIME,
            )?
            .reply()?;
        ensure!(
            reply.status == GrabStatus::SUCCESS,
            "pointer grab failed: {reply:?}"
        );
        for connection in [&self.a, &self.b] {
            connection
                .xinput_xi_select_events(
                    self.root,
                    &[xi::EventMask {
                        deviceid: self.master,
                        mask: vec![xi::XIEventMask::RAW_MOTION],
                    }],
                )?
                .check()?;
        }
        Ok(())
    }

    pub fn pointer(&self) -> Result<QueryPointerReply> {
        Ok(self.b.query_pointer(self.primary)?.reply()?)
    }

    pub fn release(&mut self) -> Result<()> {
        self.a.ungrab_pointer(x11rb::CURRENT_TIME)?.check()?;
        if self.hidden {
            self.a.xfixes_show_cursor(self.primary)?.check()?;
            self.hidden = false;
        }
        Ok(())
    }

    pub fn cleanup_probe(&self) -> Result<()> {
        // A different connection must be able to acquire BOTH devices now.
        self.b.ungrab_keyboard(x11rb::CURRENT_TIME)?.check()?;
        let pointer = self
            .b
            .grab_pointer(
                false,
                self.neighbor,
                EventMask::POINTER_MOTION,
                GrabMode::ASYNC,
                GrabMode::ASYNC,
                x11rb::NONE,
                x11rb::NONE,
                x11rb::CURRENT_TIME,
            )?
            .reply()?;
        ensure!(pointer.status == GrabStatus::SUCCESS, "pointer grab leaked");
        self.b.ungrab_pointer(x11rb::CURRENT_TIME)?.check()?;
        let keyboard = self
            .a
            .grab_keyboard(
                false,
                self.primary,
                x11rb::CURRENT_TIME,
                GrabMode::ASYNC,
                GrabMode::ASYNC,
            )?
            .reply()?;
        ensure!(
            keyboard.status == GrabStatus::SUCCESS,
            "keyboard grab leaked"
        );
        self.a.ungrab_keyboard(x11rb::CURRENT_TIME)?.check()?;
        Ok(())
    }
}

impl Drop for Clients {
    fn drop(&mut self) {
        // Best effort on early setup failure; the supervisor also reaps the
        // server. Never block in Drop waiting for an unresponsive X server.
        let _ = self.a.ungrab_pointer(x11rb::CURRENT_TIME);
        let _ = self.b.ungrab_pointer(x11rb::CURRENT_TIME);
        let _ = self.a.ungrab_keyboard(x11rb::CURRENT_TIME);
        let _ = self.b.ungrab_keyboard(x11rb::CURRENT_TIME);
        if self.hidden {
            let _ = self.a.xfixes_show_cursor(self.primary);
        }
        let _ = self.a.destroy_window(self.primary);
        let _ = self.b.destroy_window(self.neighbor);
        let _ = self.a.free_cursor(self.cursor);
        let _ = self.a.flush();
        let _ = self.b.flush();
    }
}

#[derive(Default, Debug)]
pub struct Events {
    pub motions: Vec<MotionNotifyEvent>,
    pub raw: Vec<xi::RawMotionEvent>,
    pub buttons: Vec<(bool, ButtonPressEvent)>,
    pub keys: Vec<(bool, KeyPressEvent)>,
}

impl Events {
    pub fn drain(&mut self, c: &RustConnection) -> Result<()> {
        while let Some(event) = c.poll_for_event()? {
            match event {
                Event::MotionNotify(e) => self.motions.push(e),
                Event::XinputRawMotion(e) => self.raw.push(e),
                Event::ButtonPress(e) => self.buttons.push((true, e)),
                Event::ButtonRelease(e) => self.buttons.push((false, e)),
                Event::KeyPress(e) => self.keys.push((true, e)),
                Event::KeyRelease(e) => self.keys.push((false, e)),
                Event::Error(e) => bail!("test client protocol error: {e:?}"),
                _ => {}
            }
        }
        Ok(())
    }
}

#[derive(Default, Debug)]
pub struct Seen {
    pub a: Events,
    pub b: Events,
}

impl Seen {
    pub fn sync(&mut self, c: &Clients) -> Result<()> {
        c.a.get_input_focus()?.reply()?;
        c.b.get_input_focus()?.reply()?;
        self.a.drain(&c.a)?;
        self.b.drain(&c.b)?;
        Ok(())
    }

    pub fn fenced(c: &Clients) -> Result<Self> {
        let mut seen = Self::default();
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            seen.a.drain(&c.a)?;
            seen.b.drain(&c.b)?;
            if seen.b.keys.len() >= 2 {
                break;
            }
            ensure!(Instant::now() < deadline, "missing DIX key fence: {seen:?}");
            thread::sleep(Duration::from_millis(1));
        }
        seen.sync(c)?;
        ensure!(
            seen.a.keys.is_empty() && seen.b.keys.len() == 2,
            "key recipient/count: {seen:?}"
        );
        for ((down, e), expected) in seen.b.keys.iter().zip([true, false]) {
            ensure!(
                *down == expected
                    && e.event == c.neighbor
                    && e.root == c.root
                    && e.detail == 38
                    && e.response_type & 0x80 == 0,
                "bad key fence: {e:?}"
            );
        }
        Ok(seen)
    }
}
