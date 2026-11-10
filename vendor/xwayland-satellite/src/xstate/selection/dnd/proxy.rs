//! XDND proxy validation mirrors the direct XWM: a proxy must point to itself.
use super::*;

pub(super) fn resolve(connection: &xcb::Connection, atoms: &Atoms, target: x::Window) -> x::Window {
    let read = |window| {
        let reply = connection
            .wait_for_reply(connection.send_request(&x::GetProperty {
                delete: false,
                window,
                property: atoms.proxy,
                r#type: x::ATOM_WINDOW,
                long_offset: 0,
                long_length: 1,
            }))
            .ok()?;
        (reply.r#type() == x::ATOM_WINDOW && reply.format() == 32)
            .then(|| reply.value::<x::Window>().first().copied())
            .flatten()
    };
    let Some(proxy) = read(target).filter(|proxy| *proxy != x::WINDOW_NONE) else {
        return target;
    };
    if read(proxy) != Some(proxy) {
        return target;
    }
    // Retain existing subscriptions. Child proxies need their own destroy
    // subscription; the root's SubstructureNotify covers only direct children.
    let Ok(attrs) = connection
        .wait_for_reply(connection.send_request(&x::GetWindowAttributes { window: proxy }))
    else {
        return target;
    };
    if connection
        .send_and_check_request(&x::ChangeWindowAttributes {
            window: proxy,
            value_list: &[x::Cw::EventMask(
                attrs.your_event_mask() | x::EventMask::STRUCTURE_NOTIFY,
            )],
        })
        .is_err()
    {
        return target;
    }
    proxy
}
