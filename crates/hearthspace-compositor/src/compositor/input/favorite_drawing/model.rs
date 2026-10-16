//! Bounded stroke ownership, independent of Wayland and device dispatch.
use std::{
    collections::HashSet,
    time::{Duration, Instant},
};

use serde::Serialize;
use smithay::backend::input::TouchSlot;

pub(super) const MAX_EVENTS: u32 = 2048;
const MAX_DURATION: Duration = Duration::from_secs(15);
const ACCEPT_LIFETIME: Duration = Duration::from_secs(1);
const SAMPLE_INTERVAL: Duration = Duration::from_millis(8);

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Arm {
    pub generation: u64,
    pub window: u64,
    pub output: Option<String>,
    pub bounds: [f64; 4],
    pub geometry: [f64; 5],
}

impl Arm {
    pub(super) fn local(&self, screen: [f64; 2]) -> Option<[f64; 2]> {
        let [ox, oy, scale, _, _] = self.geometry;
        let p = [
            (screen[0] - ox) / scale - self.bounds[0],
            (screen[1] - oy) / scale - self.bounds[1],
        ];
        self.contains(p).then_some(p)
    }

    pub(super) fn contains(&self, point: [f64; 2]) -> bool {
        point[0].is_finite()
            && point[1].is_finite()
            && point[0] >= 0.0
            && point[0] < self.bounds[2]
            && point[1] >= 0.0
            && point[1] < self.bounds[3]
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Source {
    Pointer,
    Touch(String, TouchSlot),
}

struct Stroke {
    arm: Arm,
    id: u64,
    source: Source,
    sequence: u32,
    point: [f64; 2],
    emitted_point: [f64; 2],
    last_sample: Instant,
    started: Instant,
}

struct Completed {
    generation: u64,
    id: u64,
    expires: Instant,
}

#[derive(Debug, Serialize)]
pub(super) struct Event {
    version: u32,
    action: &'static str,
    pub generation: u64,
    pub stroke: u64,
    pub sequence: u32,
    pub phase: &'static str,
    pub x: f64,
    pub y: f64,
    output: Option<String>,
}

impl Stroke {
    fn event(&self, phase: &'static str) -> Event {
        Event {
            version: 1,
            action: "favorite-gesture",
            generation: self.arm.generation,
            stroke: self.id,
            sequence: self.sequence,
            phase,
            x: self.point[0],
            y: self.point[1],
            output: self.arm.output.clone(),
        }
    }
}

#[derive(Default)]
pub(in crate::compositor) struct FavoriteDrawing {
    pub(super) arm: Option<Arm>,
    last_arm: Option<(u64, u64)>,
    stroke: Option<Stroke>,
    next_stroke: u64,
    completed: Option<Completed>,
    pub(super) pointer_held: bool,
    pub(super) contacts: HashSet<(String, TouchSlot)>,
    events: Vec<Event>,
}

impl FavoriteDrawing {
    pub(super) fn arm(&mut self, arm: Arm) -> Result<(), String> {
        if let Some(previous) = &self.arm {
            if previous == &arm {
                return Ok(());
            }
            if previous.window == arm.window && previous.generation > arm.generation {
                return Err("drawing generation was superseded".into());
            }
        }
        if self.arm.is_none()
            && self.last_arm.is_some_and(|(window, generation)| {
                window == arm.window && generation >= arm.generation
            })
        {
            return Err("drawing generation was closed or cancelled".into());
        }
        self.cancel();
        self.last_arm = Some((arm.window, arm.generation));
        self.arm = Some(arm);
        Ok(())
    }

    pub(super) fn disarm(&mut self, generation: u64) -> Result<(), String> {
        if self
            .arm
            .as_ref()
            .is_none_or(|arm| arm.generation != generation)
        {
            return Err("drawing mode was superseded or closed".into());
        }
        self.clear();
        Ok(())
    }

    pub(super) fn clear(&mut self) {
        self.cancel();
        self.arm = None;
    }

    pub(super) fn reconfigure(&mut self) {
        self.cancel();
        if let Some(arm) = self.arm.take() {
            self.events.push(Event {
                version: 1,
                action: "favorite-gesture",
                generation: arm.generation,
                stroke: 0,
                sequence: 0,
                phase: "reconfigure",
                x: 0.0,
                y: 0.0,
                output: arm.output,
            });
        }
    }

    pub(super) fn begin(&mut self, source: Source, point: [f64; 2], now: Instant) -> bool {
        let Some(arm) = self.arm.clone().filter(|arm| arm.contains(point)) else {
            return false;
        };
        if self.stroke.is_some() {
            self.cancel();
            return false;
        }
        self.completed = None;
        self.next_stroke = self.next_stroke.wrapping_add(1).max(1);
        let stroke = Stroke {
            arm,
            id: self.next_stroke,
            source,
            sequence: 0,
            point,
            emitted_point: point,
            last_sample: now,
            started: now,
        };
        self.events.push(stroke.event("begin"));
        self.stroke = Some(stroke);
        true
    }

    pub(super) fn owns(&self, source: &Source) -> bool {
        self.stroke
            .as_ref()
            .is_some_and(|stroke| &stroke.source == source)
    }

    pub(super) fn update(&mut self, source: &Source, point: [f64; 2], now: Instant) {
        if !self.owns(source) {
            return;
        }
        let stroke = self.stroke.as_mut().unwrap();
        if !stroke.arm.contains(point)
            || now.duration_since(stroke.started) >= MAX_DURATION
            || stroke.sequence >= MAX_EVENTS
        {
            self.cancel();
            return;
        }
        if stroke.point == point {
            return;
        }
        stroke.point = point;
        if now.duration_since(stroke.last_sample) < SAMPLE_INTERVAL {
            return;
        }
        stroke.last_sample = now;
        stroke.emitted_point = point;
        stroke.sequence += 1;
        self.events.push(stroke.event("update"));
    }

    pub(super) fn end(&mut self, source: &Source, cancelled: bool, now: Instant) {
        if !self.owns(source) {
            return;
        }
        if cancelled || now.duration_since(self.stroke.as_ref().unwrap().started) >= MAX_DURATION {
            self.cancel();
            return;
        }
        let mut stroke = self.stroke.take().unwrap();
        // Rate limiting must never discard the exact endpoint on release.
        if stroke.point != stroke.emitted_point {
            stroke.sequence += 1;
            self.events.push(stroke.event("update"));
        }
        stroke.sequence += 1;
        self.completed = Some(Completed {
            generation: stroke.arm.generation,
            id: stroke.id,
            expires: now + ACCEPT_LIFETIME,
        });
        self.events.push(stroke.event("end"));
    }

    pub(super) fn cancel(&mut self) {
        self.completed = None;
        if let Some(mut stroke) = self.stroke.take() {
            stroke.sequence += 1;
            self.events.push(stroke.event("cancel"));
        }
    }

    pub(super) fn accept(&mut self, generation: u64, id: u64, now: Instant) -> Result<(), String> {
        if self
            .arm
            .as_ref()
            .is_none_or(|arm| arm.generation != generation)
            || self.completed.as_ref().is_none_or(|done| {
                done.generation != generation || done.id != id || now >= done.expires
            })
        {
            return Err("drawing stroke expired, cancelled, or superseded".into());
        }
        self.completed = None;
        Ok(())
    }

    pub(super) fn events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }

    pub(in crate::compositor) fn status(&self) -> serde_json::Value {
        match &self.arm {
            Some(arm) => serde_json::json!({ "armed": true, "generation": arm.generation,
                "output": arm.output, "bounds": arm.bounds,
                "stroke": self.stroke.as_ref().map(|stroke| stroke.id) }),
            None => serde_json::json!({"armed": false}),
        }
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
