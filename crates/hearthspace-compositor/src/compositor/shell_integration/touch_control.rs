//! Frame-accurate touchscreen input for the same private control socket as pointer input.
use smithay::backend::input::{InputTime, TouchSlot};

use super::ControlReply;
use crate::compositor::CalloopData;

enum Contact {
    Down(u32, f64, f64),
    Motion(u32, f64, f64),
    Up(u32),
    Cancel(Option<u32>),
    Frame,
    Remove,
}

fn parse(text: &str) -> Option<(String, Contact)> {
    let mut words = text.split_whitespace();
    let command = words.next()?;
    let device = words.next()?;
    if device.len() > 128 {
        return None;
    }
    let slot = |text: &str| text.parse::<u32>().ok().filter(|n| *n <= i32::MAX as u32);
    let coordinate = |text: &str| text.parse::<f64>().ok().filter(|n| n.is_finite());
    let contact = match command {
        "touch-down" | "touch-motion" => {
            let slot = slot(words.next()?)?;
            let x = coordinate(words.next()?)?;
            let y = coordinate(words.next()?)?;
            if command == "touch-down" {
                Contact::Down(slot, x, y)
            } else {
                Contact::Motion(slot, x, y)
            }
        }
        "touch-up" => Contact::Up(slot(words.next()?)?),
        "touch-cancel" => Contact::Cancel(match words.next() {
            Some(value) => Some(slot(value)?),
            None => None,
        }),
        "touch-frame" => Contact::Frame,
        "touch-remove" => Contact::Remove,
        _ => return None,
    };
    // Keep injected devices distinct from hardware identifiers.
    words
        .next()
        .is_none()
        .then(|| (format!("control:{device}"), contact))
}

impl CalloopData {
    pub(super) fn run_touch_command(&mut self, text: &str) -> Option<ControlReply> {
        if !text.starts_with("touch-") {
            return None;
        }
        let Some((device, contact)) = parse(text) else {
            return Some(ControlReply::Err("invalid touch command".into()));
        };
        let time = InputTime::now();
        let slot = |id| TouchSlot::from(Some(id));
        match contact {
            Contact::Down(id, x, y) => {
                self.state
                    .touch_down(&device, slot(id), (x, y).into(), time)
            }
            Contact::Motion(id, x, y) => {
                self.state
                    .touch_motion(&device, slot(id), (x, y).into(), time)
            }
            Contact::Up(id) => self.state.touch_up(&device, slot(id), time),
            Contact::Cancel(id) => self.state.touch_cancel(&device, id.into(), time),
            Contact::Frame => self.state.flush_touch_frame(&device, time),
            Contact::Remove => self.state.remove_touch_device(&device),
        }
        Some(ControlReply::Ok)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malformed_touch_frames_cannot_inject_partial_input() {
        for text in [
            "touch-down finger 0 NaN 0",
            "touch-motion finger 0 0 inf",
            "touch-up finger -1",
            "touch-cancel finger 4294967295",
            "touch-frame finger trailing",
            "touch-down finger 0 1",
        ] {
            assert!(parse(text).is_none(), "{text}");
        }
        assert!(matches!(
            parse("touch-cancel finger").unwrap().1,
            Contact::Cancel(None)
        ));
        assert!(matches!(
            parse("touch-motion finger 2 -5.25 300").unwrap().1,
            Contact::Motion(2, -5.25, 300.0)
        ));
    }
}
