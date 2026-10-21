use super::{App, ManagedWindowKind, Rect, ResultItem, motion::Pose, window_app_id, window_title};
use hearthspace_common::canvas_search::{layout, matches, park};
use smithay::utils::{Logical, Rectangle};
use std::{
    collections::{BTreeMap, HashMap, HashSet, hash_map::DefaultHasher},
    hash::{Hash, Hasher},
};
pub(super) fn rect(rect: Rectangle<i32, Logical>) -> Rect {
    Rect {
        x: f64::from(rect.loc.x),
        y: f64::from(rect.loc.y),
        width: f64::from(rect.size.w),
        height: f64::from(rect.size.h),
    }
}
impl App {
    fn canvas_search_sources(&self) -> Vec<ResultItem> {
        let mut items = Vec::new();
        for (index, window) in self.windows.iter().enumerate() {
            if !window.mapped || window.kind != ManagedWindowKind::DesktopItem {
                continue;
            }
            let Some(id) = self.desktop_id(index) else {
                continue;
            };
            let title = window_title(&window.surface).unwrap_or_else(|| id.clone());
            items.push(ResultItem {
                id,
                search_text: title.clone(),
                title,
                kind: "item",
                window: Some(window.id),
                natural: rect(self.base_window_screen_bounds(index)),
            });
        }
        let order: HashMap<_, _> = self
            .desktop_ordered_ids()
            .iter()
            .enumerate()
            .map(|(index, id)| (id, index))
            .collect();
        items.sort_by_key(|item| order.get(&item.id).copied().unwrap_or(usize::MAX));
        for (index, window) in self.windows.iter().enumerate().rev() {
            if !self.is_navigable_window(index) {
                continue;
            }
            let title = window_title(&window.surface)
                .or_else(|| window_app_id(&window.surface))
                .unwrap_or_default();
            items.push(ResultItem {
                id: format!("window:{}", window.id),
                search_text: format!(
                    "{title} window {}",
                    window_app_id(&window.surface).unwrap_or_default()
                ),
                title,
                kind: "window",
                window: Some(window.id),
                natural: rect(self.base_window_screen_bounds(index)),
            });
        }
        for entry in &self.canvas_search.entries {
            if entry.id.starts_with("app:") || entry.id.starts_with("file:") {
                continue;
            }
            let window = self.windows.iter().enumerate().find_map(|(index, w)| {
                (w.mapped
                    && self
                        .canvas_search_result_identity(index)
                        .is_some_and(|(session, id)| {
                            self.canvas_search
                                .session
                                .as_ref()
                                .is_some_and(|s| session == 0 || s.id == session)
                                && id == entry.id
                        }))
                .then_some(w.id)
            });
            items.push(ResultItem {
                id: entry.id.clone(),
                title: entry.title.clone(),
                search_text: entry.search_text.clone(),
                kind: if entry.id.starts_with("tool:") {
                    "tool"
                } else {
                    "control"
                },
                window,
                natural: Rect {
                    x: 0.,
                    y: 0.,
                    width: 240.,
                    height: 72.,
                },
            });
        }
        items
    }
    pub(in crate::compositor) fn rebuild_canvas_search(&mut self) {
        let Some(session) = self.canvas_search.session.as_ref() else {
            return;
        };
        let view = session.view;
        let anchor = session.anchor;
        let query = session.query.clone();
        let sources = self.canvas_search_sources();
        let mut signature = DefaultHasher::new();
        query.hash(&mut signature);
        for item in &sources {
            item.id.hash(&mut signature);
            item.title.hash(&mut signature);
            item.search_text.hash(&mut signature);
            item.window.hash(&mut signature);
            for v in [
                item.natural.x,
                item.natural.y,
                item.natural.width,
                item.natural.height,
            ] {
                v.to_bits().hash(&mut signature);
            }
        }
        for entry in self
            .canvas_search
            .entries
            .iter()
            .chain(&self.canvas_search.desktop_entries)
        {
            entry.id.hash(&mut signature);
            entry.search_text.hash(&mut signature);
        }
        let signature = signature.finish();
        if session.signature == Some(signature) {
            return;
        }
        self.canvas_search.session.as_mut().unwrap().signature = Some(signature);
        let metadata: HashMap<_, _> = self
            .canvas_search
            .desktop_entries
            .iter()
            .chain(&self.canvas_search.entries)
            .map(|e| (&e.id, e))
            .collect();
        let windows: HashMap<_, _> = self
            .windows
            .iter()
            .enumerate()
            .map(|(i, w)| (w.id, i))
            .collect();
        let results: Vec<_> = sources
            .iter()
            .filter(|item| {
                if item.kind == "window" && query.trim().is_empty() {
                    return false;
                }
                let entry = metadata.get(&item.id);
                let text = entry.map_or_else(
                    || item.search_text.clone(),
                    |e| format!("{} {}", item.title, e.search_text),
                );
                matches(&query, &text)
            })
            .cloned()
            .collect();
        let natural_sizes: Vec<_> = results
            .iter()
            .map(|r| {
                if r.kind == "item" {
                    (88., 96.)
                } else if r.kind == "window" {
                    let i = windows[&r.window.unwrap()];
                    let rect = self.window_canvas_rect(i);
                    (f64::from(rect.size.w), f64::from(rect.size.h))
                } else {
                    (240., 72.)
                }
            })
            .collect();
        let layout = layout(
            view,
            anchor,
            &natural_sizes,
            results.iter().any(|r| r.kind != "item"),
            false,
        );
        let mut targets = BTreeMap::new();
        for ((item, p), size) in results.iter().zip(&layout.placements).zip(&natural_sizes) {
            targets.insert(
                item.id.clone(),
                Rect {
                    x: p.x,
                    y: p.y,
                    width: size.0 * p.scale,
                    height: size.1 * p.scale,
                },
            );
        }
        let now = self.canvas_search_motion_start();
        let mut changed = false;
        for item in &sources {
            let Some(window) = item.window else { continue };
            let is_match = targets.contains_key(&item.id);
            let to = targets.get(&item.id).copied().unwrap_or_else(|| {
                let p = park(item.natural, view);
                Rect {
                    x: p.x,
                    y: p.y,
                    ..item.natural
                }
            });
            let opacity = if is_match { 1. } else { 0. };
            let previous = self.canvas_search.poses.get(&window);
            if previous
                .is_some_and(|p| p.to == to && p.to_opacity == opacity && p.natural == item.natural)
            {
                continue;
            }
            let (from, from_opacity) = previous.map(|p| p.current(now)).unwrap_or(
                if item.kind == "control" || item.kind == "tool" {
                    (to, 0.)
                } else {
                    (item.natural, 1.)
                },
            );
            self.canvas_search.poses.insert(
                window,
                Pose {
                    from,
                    to,
                    from_opacity,
                    to_opacity: opacity,
                    started: now,
                    natural: item.natural,
                    matches: is_match,
                    reveal: false,
                },
            );
            changed = true;
        }
        let mapped: HashSet<_> = self
            .windows
            .iter()
            .filter(|w| w.mapped)
            .map(|w| w.id)
            .collect();
        self.canvas_search.poses.retain(|id, _| mapped.contains(id));
        let s = self.canvas_search.session.as_mut().unwrap();
        let mut field = layout.field.rect;
        if results.is_empty() {
            field.height += 42.;
            if layout.field.results_above {
                field.y -= 42.;
            }
        }
        let result_changed = s
            .results
            .iter()
            .map(|r| (&r.id, &r.title, r.window))
            .ne(results.iter().map(|r| (&r.id, &r.title, r.window)))
            || s.field != field;
        if !results.iter().any(|r| Some(&r.id) == s.selected.as_ref()) {
            s.selected = results.first().map(|r| r.id.clone());
        }
        s.results = results;
        s.field = field;
        s.above = layout.field.results_above;
        if result_changed {
            self.canvas_search_changed();
        } else if changed {
            self.request_redraw();
        }
    }
}
