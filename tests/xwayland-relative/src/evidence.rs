//! Structured observations keep negative classification independent of labels.
use crate::client::{Clients, Events, Seen};
use serde_json::{Value, json};
use x11rb::protocol::{
    xinput as xi,
    xproto::{self, KeyButMask, QueryPointerReply},
};

pub fn pointer(reply: &QueryPointerReply) -> Value {
    json!({"root": reply.root, "child": reply.child, "same_screen": reply.same_screen,
        "root_xy": [reply.root_x, reply.root_y], "window_xy": [reply.win_x, reply.win_y]})
}

pub fn seen(seen: &Seen) -> Value {
    json!({"a": events(&seen.a), "b": events(&seen.b)})
}

fn events(events: &Events) -> Value {
    let motions: Vec<_> = events
        .motions
        .iter()
        .map(|e| {
            json!({"recipient": e.event, "root": e.root, "child": e.child,
            "root_xy": [e.root_x, e.root_y], "event_xy": [e.event_x, e.event_y],
            "same_screen": e.same_screen, "response_type": e.response_type})
        })
        .collect();
    let buttons: Vec<_> = events
        .buttons
        .iter()
        .map(|(pressed, e)| {
            json!({"pressed": pressed, "recipient": e.event, "root": e.root, "child": e.child,
            "root_xy": [e.root_x, e.root_y], "event_xy": [e.event_x, e.event_y],
            "same_screen": e.same_screen, "response_type": e.response_type,
            "detail": e.detail, "button1": e.state.contains(KeyButMask::BUTTON1)})
        })
        .collect();
    let raw: Vec<_> = events
        .raw
        .iter()
        .map(|e| {
            let values: Vec<_> = e
                .axisvalues_raw
                .iter()
                .map(|v| (v.integral, v.frac))
                .collect();
            json!({"master": e.deviceid, "source": e.sourceid, "event_type": e.event_type,
            "response_type": e.response_type, "valuator_mask": e.valuator_mask,
            "delta_raw": values})
        })
        .collect();
    json!({"motions": motions, "buttons": buttons, "raw": raw})
}

pub fn motion_integrity(events: &Events, c: &Clients) -> bool {
    events.motions.len() == 1
        && events.motions.iter().all(|e| {
            e.event == c.primary
                && e.root == c.root
                && e.child == c.child
                && e.same_screen
                && e.response_type == xproto::MOTION_NOTIFY_EVENT
        })
}

pub fn raw_integrity(events: &Events, c: &Clients, dx: f64) -> bool {
    events.raw.len() == 1
        && events.raw.iter().all(|e| {
            let values: Vec<_> = e
                .axisvalues_raw
                .iter()
                .map(|v| (v.integral, v.frac))
                .collect();
            e.deviceid == c.master
                && e.sourceid == c.relative
                && e.event_type == xi::RAW_MOTION_EVENT
                && e.response_type == 35
                && e.valuator_mask.first() == Some(&3)
                && e.valuator_mask[1..].iter().all(|mask| *mask == 0)
                && values == vec![(dx as i32, 0), (0, 0)]
        })
}

pub fn button_integrity(events: &Events, root: u32, window: u32) -> bool {
    events.buttons.len() == 2
        && events
            .buttons
            .iter()
            .zip([true, false])
            .all(|((pressed, e), want)| {
                *pressed == want
                    && e.event == window
                    && e.root == root
                    && e.detail == 1
                    && e.same_screen
                    && e.response_type
                        == if want {
                            xproto::BUTTON_PRESS_EVENT
                        } else {
                            xproto::BUTTON_RELEASE_EVENT
                        }
                    && e.state.contains(KeyButMask::BUTTON1) != *pressed
            })
}

// Coordinates are checked independently. Every call site must also require the
// non-vacuous integrity check (including exact count and balanced state).
pub fn button_coordinates(events: &Events, point: [i16; 2]) -> bool {
    events
        .buttons
        .iter()
        .all(|(_, e)| [e.event_x, e.event_y] == point && [e.root_x, e.root_y] == point)
}
