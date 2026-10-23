//! Canvas-local shell surfaces: the client paints; the compositor owns placement and input.
use super::{App, ManagedWindowKind, windows::window_app_id};
use crate::geometry::CanvasPoint;
use hearthspace_common::{
    DESKTOP_ITEM_HEIGHT, DESKTOP_ITEM_WIDTH, MAX_DESKTOP_ITEMS, desktop_item_id_from_app_id,
};
use smithay::utils::{Logical, Point, Rectangle};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    time::Instant,
};

mod context;
mod dnd;
mod geometry;
mod input;
mod keyboard;
mod protocol;
mod render;

pub(super) use keyboard::Action;

pub(super) struct State {
    selected: BTreeSet<String>,
    focused: Option<String>,
    keyboard_focus: bool,
    select_mode: bool,
    revision: u64,
    gesture: Option<Gesture>,
    suppress_release: bool,
    last_click: Option<(String, Instant)>,
    actions: VecDeque<(u64, Instant, &'static str, String)>,
    next_action: u64,
    next_slot: usize,
    order: Vec<String>,
    fresh: BTreeSet<String>,
    edge_tick: Instant,
    selection_cache: render::Cache,
    context: context::State,
    files: BTreeMap<String, String>,
    file_drag: Option<dnd::Drag>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            selected: BTreeSet::new(),
            focused: None,
            keyboard_focus: false,
            select_mode: true,
            revision: 0,
            gesture: None,
            suppress_release: false,
            last_click: None,
            actions: VecDeque::new(),
            next_action: 0,
            next_slot: 0,
            order: Vec::new(),
            fresh: BTreeSet::new(),
            edge_tick: Instant::now(),
            selection_cache: Default::default(),
            context: Default::default(),
            files: BTreeMap::new(),
            file_drag: None,
        }
    }
}

enum Gesture {
    Move {
        id: String,
        start: Point<f64, Logical>,
        positions: BTreeMap<String, CanvasPoint>,
        moved: bool,
        clickable: bool,
    },
    Marquee {
        start: Point<f64, Logical>,
        current: Point<f64, Logical>,
        before: BTreeSet<String>,
        original: BTreeSet<String>,
    },
}

impl App {
    pub(in crate::compositor) fn desktop_gesture_active(&self) -> bool {
        self.desktop.gesture.is_some()
    }

    pub(in crate::compositor) fn desktop_ordered_ids(&self) -> &[String] {
        &self.desktop.order
    }
    pub(in crate::compositor) fn desktop_id(&self, index: usize) -> Option<String> {
        (self.windows[index].kind == ManagedWindowKind::DesktopItem)
            .then(|| window_app_id(&self.windows[index].surface))
            .flatten()
            .and_then(|id| desktop_item_id_from_app_id(&id))
    }

    fn desktop_index(&self, id: &str) -> Option<usize> {
        self.windows
            .iter()
            .enumerate()
            .find(|(index, window)| window.mapped && self.desktop_id(*index).as_deref() == Some(id))
            .map(|(index, _)| index)
    }

    pub(in crate::compositor) fn desktop_tile_rect(&self, index: usize) -> Rectangle<i32, Logical> {
        let point = self.windows[index].position;
        Rectangle::new(
            (point.x, point.y).into(),
            (DESKTOP_ITEM_WIDTH, DESKTOP_ITEM_HEIGHT).into(),
        )
    }

    pub(in crate::compositor) fn desktop_mapped(&mut self, index: usize) {
        let Some(id) = self.desktop_id(index) else {
            return;
        };
        if self.windows[index].mapped && !self.windows[index].initial_placement_done {
            let slot = self
                .desktop
                .order
                .iter()
                .position(|entry| entry == &id)
                .unwrap_or(self.desktop.next_slot);
            self.desktop.next_slot += 1;
            let point = self.layout_store.desktop_position(&id).unwrap_or_else(|| {
                if !self.desktop.order.contains(&id) {
                    self.desktop.fresh.insert(id.clone());
                }
                self.vacant_desktop_position(slot, Some(index))
            });
            self.windows[index].position = point;
            self.windows[index].initial_placement_done = true;
        } else if !self.windows[index].mapped {
            self.desktop.selected.remove(&id);
            if self.desktop.focused.as_ref() == Some(&id) {
                self.desktop.focused = None;
            }
            self.cancel_desktop_gesture();
        }
        self.desktop_changed();
    }

    fn vacant_desktop_position(&self, mut slot: usize, exclude: Option<usize>) -> CanvasPoint {
        let excluded_id = exclude.and_then(|index| self.desktop_id(index));
        loop {
            let point = CanvasPoint {
                x: 28 + 104 * (slot / 4) as i32,
                y: 112 + 104 * (slot % 4) as i32,
            };
            let rect = Rectangle::new(
                (point.x, point.y).into(),
                (DESKTOP_ITEM_WIDTH, DESKTOP_ITEM_HEIGHT).into(),
            );
            if !self.windows.iter().enumerate().any(|(index, window)| {
                Some(index) != exclude
                    && window.mapped
                    && window.initial_placement_done
                    && window.kind == ManagedWindowKind::DesktopItem
                    && rect.overlaps(self.desktop_tile_rect(index))
            }) && !self.desktop.order.iter().any(|id| {
                Some(id) != excluded_id.as_ref()
                    && !self.desktop.fresh.contains(id)
                    && self.layout_store.desktop_position(id).is_some_and(|saved| {
                        rect.overlaps(Rectangle::new(
                            (saved.x, saved.y).into(),
                            (DESKTOP_ITEM_WIDTH, DESKTOP_ITEM_HEIGHT).into(),
                        ))
                    })
            }) {
                return point;
            }
            slot += 1;
        }
    }

    fn register_desktop_ids(&mut self, offset: usize, ids: Vec<String>) -> Result<(), String> {
        if offset > MAX_DESKTOP_ITEMS
            || ids.len() > MAX_DESKTOP_ITEMS - offset
            || ids
                .iter()
                .any(|id| id.is_empty() || id.len() > 1024 || id.chars().any(char::is_control))
        {
            return Err("invalid desktop registration".into());
        }
        if offset != 0 && offset != self.desktop.order.len() {
            return Err("desktop registration offset is stale".into());
        }
        let mut seen = if offset == 0 {
            BTreeSet::new()
        } else {
            self.desktop.order.iter().cloned().collect()
        };
        if ids.iter().any(|id| !seen.insert(id.clone())) {
            return Err("duplicate desktop registration".into());
        }
        if offset == 0 {
            self.desktop.order.clear();
        }
        self.desktop.order.extend(ids);
        let fresh: Vec<_> = self
            .desktop
            .order
            .iter()
            .enumerate()
            .filter_map(|(slot, id)| {
                self.desktop
                    .fresh
                    .contains(id)
                    .then(|| self.desktop_index(id).map(|index| (slot, index)))
                    .flatten()
            })
            .collect();
        // Remove the uncommitted seed positions before assigning the reference's ordered grid.
        for (_, index) in &fresh {
            self.windows[*index].initial_placement_done = false;
        }
        for (slot, index) in fresh {
            self.windows[index].position = self.vacant_desktop_position(slot, Some(index));
            self.windows[index].initial_placement_done = true;
            if let Some(id) = self.desktop_id(index) {
                self.desktop.fresh.remove(&id);
            }
        }
        self.desktop.next_slot = self.desktop.order.len().max(self.desktop.next_slot);
        self.desktop_changed();
        Ok(())
    }

    pub(in crate::compositor) fn desktop_focus_changed(&mut self, index: Option<usize>) {
        let focused = index.and_then(|index| self.desktop_id(index));
        if self.desktop.focused != focused {
            self.desktop.focused = focused;
            self.desktop_changed();
        }
    }

    fn focus_desktop(&mut self, id: &str, keyboard: bool) -> Result<(), String> {
        self.check_navigation_available()?;
        let index = self
            .desktop_index(id)
            .ok_or("desktop item is unavailable")?;
        self.desktop.keyboard_focus = keyboard;
        if keyboard {
            let bounds = self.window_screen_bounds(index);
            let outputs = self.output_logical_rects();
            if !outputs
                .iter()
                .any(|output| self.usable_output(*output).contains_rect(bounds))
            {
                let output = outputs
                    .iter()
                    .copied()
                    .find(|output| output.contains(self.pointer_location.to_i32_round()))
                    .unwrap_or_else(|| self.primary_output_logical_rect());
                self.viewport_animation = None;
                self.viewport_offset = super::navigation::reveal_offset(
                    self.desktop_tile_rect(index),
                    self.usable_output(output),
                    self.viewport_scale,
                );
            }
        }
        if let Some(surface) = self.windows[index].surface.wl_surface() {
            self.set_keyboard_focus_to_window(index, surface);
        }
        self.desktop_changed();
        Ok(())
    }

    fn desktop_positions(&self) -> BTreeMap<String, CanvasPoint> {
        self.windows
            .iter()
            .enumerate()
            .filter_map(|(index, window)| {
                let id = self.desktop_id(index)?;
                (window.mapped && self.desktop.selected.contains(&id))
                    .then_some((id, window.position))
            })
            .collect()
    }

    fn restore_desktop_positions(&mut self, positions: &BTreeMap<String, CanvasPoint>) {
        for (id, position) in positions {
            if let Some(index) = self.desktop_index(id) {
                self.windows[index].position = *position;
            }
        }
        self.request_redraw();
    }

    pub(in crate::compositor) fn cancel_desktop_gesture(&mut self) {
        self.cancel_desktop_drop();
        if let Some(gesture) = self.desktop.gesture.take() {
            match gesture {
                Gesture::Move { positions, .. } => self.restore_desktop_positions(&positions),
                Gesture::Marquee { original, .. } => self.desktop.selected = original,
            }
            self.desktop.suppress_release = true;
            self.desktop.last_click = None;
            self.desktop_changed();
        }
    }

    pub(in crate::compositor) fn clear_desktop_for_canvas_search(&mut self) -> bool {
        self.clear_desktop_selection();
        // Search owns the cancelled gesture's eventual release. Leaving this
        // latch here would consume a later field or desktop click instead.
        std::mem::take(&mut self.desktop.suppress_release)
    }

    pub(in crate::compositor) fn clear_desktop_selection(&mut self) {
        // Cancel before clearing: a cancelled marquee restores its old selection.
        // Keep ownership of any held pointer release until it reaches the desktop.
        self.cancel_desktop_gesture();
        self.desktop.selected.clear();
        self.desktop.keyboard_focus = false;
        self.desktop.select_mode = false;
        self.desktop_changed();
    }
    pub(in crate::compositor) fn disable_desktop_selection(&mut self) {
        self.cancel_desktop_gesture();
        self.desktop.select_mode = false;
        self.desktop_changed();
    }

    pub(in crate::compositor) fn reset_desktop_input(&mut self) {
        self.cancel_desktop_gesture();
        self.reset_desktop_context();
        self.desktop.suppress_release = false;
        self.desktop.actions.clear();
        self.desktop.last_click = None;
    }
}
