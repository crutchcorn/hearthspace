use super::{Decoration, Scene, covering_windows, geometry::intersect};

const NONE: usize = usize::MAX;

/// Return displaced decorations to clear homes, without adding any new peeks.
///
/// Windows/obstacles block complete bounds; peers block artwork. The reference's
/// ascending rejection, simultaneous cycles and reverse fallback are preserved.
/// Work is O(N² + NW), with O(N + W) auxiliary space, including fallback rounds.
pub fn restore_background(scene: &mut Scene) {
    restore(scene, &mut Work::default());
}

fn restore(scene: &mut Scene, work: &mut Work) {
    if !scene.decorations.iter().any(|item| item.home.is_some()) {
        return;
    }
    let homes: Vec<_> = scene
        .decorations
        .iter()
        .map(|item| {
            let mut home = item.home.map_or(*item, |position| item.at(position));
            home.home = None;
            home
        })
        .collect();
    let windows = [
        covering_windows(scene, false),
        covering_windows(scene, true),
    ];
    let fixed: Vec<_> = homes
        .iter()
        .map(|home| {
            windows[usize::from(home.raised)].iter().any(|other| {
                work.step();
                intersect(home.bounds, *other).is_some()
            })
        })
        .collect();
    let groups = Groups::new(&scene.decorations, &homes, work);
    let count = homes.len();
    let mut state = Restoration {
        items: &mut scene.decorations,
        homes,
        groups,
        fixed,
        base: vec![false; count],
        permanent: vec![false; count],
        actual: vec![0; count],
        early: vec![false; count],
        earlier: vec![0; count],
        work,
    };
    state.initialize();
    let mut batch = Vec::with_capacity(count);
    loop {
        state.work.round();
        batch.clear();
        // The component graph is acyclic. Zero-count peeling finds exactly the
        // candidates outside the dependency closure of first-sweep rejections.
        loop {
            let ready = (0..state.groups.head.len()).find(|&group| {
                state.work.step();
                state.groups.active[group] && state.groups.blockers[group] == 0
            });
            let Some(group) = ready else { break };
            state.remove_group(group);
            let mut index = state.groups.head[group];
            while index != NONE {
                batch.push(index);
                index = state.groups.next[index];
            }
        }
        state.commit(&batch);
        let mut fallback = false;
        for index in (0..count).rev() {
            state.work.step();
            if state.pending(index) && !state.fixed[index] && state.actual[index] == 0 {
                // A nontrivial component cannot return individually: another
                // still-displaced member would occupy this member's home.
                state.remove_group(state.groups.of[index]);
                state.commit(&[index]);
                fallback = true;
            }
        }
        if !fallback {
            break;
        }
    }
}

struct Restoration<'a> {
    items: &'a mut [Decoration],
    homes: Vec<Decoration>,
    groups: Groups,
    fixed: Vec<bool>,
    base: Vec<bool>,
    permanent: Vec<bool>,
    actual: Vec<usize>,
    early: Vec<bool>,
    earlier: Vec<usize>,
    work: &'a mut Work,
}

impl Restoration<'_> {
    fn pending(&self, index: usize) -> bool {
        self.groups.of[index] != NONE && self.groups.active[self.groups.of[index]]
    }

    fn initialize(&mut self) {
        for index in 0..self.items.len() {
            if !self.pending(index) {
                continue;
            }
            self.base[index] = self.fixed[index];
            for other in 0..self.items.len() {
                self.work.step();
                if index == other {
                    continue;
                }
                let current = blocks(self.homes[index], self.items[other]);
                self.actual[index] += usize::from(current);
                if !self.pending(other) {
                    self.base[index] |= current;
                } else {
                    self.base[index] |=
                        other > index && blocks(self.homes[index], self.homes[other]);
                    if self.groups.of[index] != self.groups.of[other] && current {
                        self.groups.blockers[self.groups.of[index]] += 1;
                    }
                }
            }
            self.groups.blockers[self.groups.of[index]] += usize::from(self.base[index]);
        }
        // Raised candidates ignore ordinary peers. An earlier raised survivor
        // can nevertheless reject an ordinary home during the first sweep,
        // even if that raised candidate is rejected in a subsequent sweep.
        for index in 0..self.items.len() {
            self.work.step();
            if self.pending(index) && self.items[index].raised {
                self.early[index] = self.base[index] || self.earlier[index] != 0;
                self.adjust_raised(index, true);
            }
        }
    }

    fn remove_group(&mut self, group: usize) {
        self.groups.active[group] = false;
        self.work.removed();
        let mut member = self.groups.head[group];
        while member != NONE {
            for other in 0..self.items.len() {
                self.work.step();
                if self.pending(other) && blocks(self.homes[other], self.items[member]) {
                    self.groups.blockers[self.groups.of[other]] -= 1;
                }
            }
            member = self.groups.next[member];
        }
    }

    fn commit(&mut self, batch: &[usize]) {
        if batch.is_empty() {
            return;
        }
        // Add ALL permanent home conflicts before removing old contributions.
        // This avoids transient raised-flag toggles inside a simultaneous move.
        for &index in batch {
            for other in 0..self.items.len() {
                self.work.step();
                if self.pending(other)
                    && !self.permanent[other]
                    && blocks(self.homes[other], self.homes[index])
                {
                    self.permanent[other] = true;
                    self.groups.blockers[self.groups.of[other]] += 1;
                }
            }
        }
        self.refresh_raised();
        for &index in batch {
            if self.items[index].raised {
                self.adjust_raised(index, false);
            }
            for other in 0..self.items.len() {
                self.work.step();
                if self.pending(other) {
                    self.actual[other] -= usize::from(blocks(self.homes[other], self.items[index]));
                    self.actual[other] += usize::from(blocks(self.homes[other], self.homes[index]));
                }
            }
            self.items[index] = self.homes[index];
        }
        self.refresh_raised();
    }

    fn refresh_raised(&mut self) {
        // Same-layer home conflicts with a later candidate are already in base;
        // the only variable first-sweep inputs are earlier rejected currents.
        for index in 0..self.items.len() {
            self.work.step();
            if !self.pending(index) || !self.items[index].raised {
                continue;
            }
            let rejected = self.base[index] || self.permanent[index] || self.earlier[index] != 0;
            if rejected != self.early[index] {
                self.adjust_raised(index, false);
                self.early[index] = rejected;
                self.adjust_raised(index, true);
                self.work.flip();
            }
        }
        // A surviving -> rejected change originates in a permanent home
        // conflict and propagates through current dependencies: it cannot clear.
        // Each raised flag changes at most twice, so these scans total O(N²).
    }

    fn adjust_raised(&mut self, index: usize, add: bool) {
        for other in index + 1..self.items.len() {
            self.work.step();
            if !self.pending(other) {
                continue;
            }
            let count = if self.early[index]
                && self.items[other].raised
                && blocks(self.homes[other], self.items[index])
            {
                &mut self.earlier[other]
            } else if !self.early[index]
                && !self.items[other].raised
                && blocks(self.homes[other], self.homes[index])
            {
                &mut self.groups.blockers[self.groups.of[other]]
            } else {
                continue;
            };
            if add {
                *count += 1;
            } else {
                *count -= 1;
            }
        }
    }
}

fn blocks(home: Decoration, peer: Decoration) -> bool {
    (!home.raised || peer.raised) && intersect(home.body, peer.body).is_some()
}

struct Groups {
    of: Vec<usize>,
    head: Vec<usize>,
    next: Vec<usize>,
    active: Vec<bool>,
    blockers: Vec<usize>,
}

impl Groups {
    // Iterative Kosaraju on implicit geometry edges: two O(N²) scans, no edge
    // matrix, recursion or per-component allocation. An original dependency
    // component must return as a whole, so components never need rebuilding.
    fn new(items: &[Decoration], homes: &[Decoration], work: &mut Work) -> Self {
        let count = items.len();
        let mut seen = vec![false; count];
        let mut order = Vec::with_capacity(count);
        let mut stack = Vec::with_capacity(count);
        for start in 0..count {
            if items[start].home.is_none() || seen[start] {
                continue;
            }
            seen[start] = true;
            stack.push((start, 0));
            while let Some((index, cursor)) = stack.last_mut() {
                if *cursor == count {
                    order.push(*index);
                    stack.pop();
                    continue;
                }
                work.step();
                let (from, to) = (*index, *cursor);
                *cursor += 1;
                if !seen[to] && items[to].home.is_some() && blocks(homes[from], items[to]) {
                    seen[to] = true;
                    stack.push((to, 0));
                }
            }
        }
        let mut result = Self {
            of: vec![NONE; count],
            next: vec![NONE; count],
            head: Vec::new(),
            active: Vec::new(),
            blockers: Vec::new(),
        };
        for start in order.into_iter().rev() {
            if result.of[start] != NONE {
                continue;
            }
            let group = result.head.len();
            result.head.push(NONE);
            result.of[start] = group;
            stack.push((start, 0));
            while let Some((index, _)) = stack.pop() {
                result.next[index] = result.head[group];
                result.head[group] = index;
                for other in 0..count {
                    work.step();
                    if result.of[other] == NONE
                        && items[other].home.is_some()
                        && blocks(homes[other], items[index])
                    {
                        result.of[other] = group;
                        stack.push((other, 0));
                    }
                }
            }
        }
        result.active.resize(result.head.len(), true);
        result.blockers.resize(result.head.len(), 0);
        result
    }
}

// Instrument actual loop work only in tests; production carries no counters.
#[derive(Default)]
struct Work {
    #[cfg(test)]
    steps: usize,
    #[cfg(test)]
    rounds: usize,
    #[cfg(test)]
    removed: usize,
    #[cfg(test)]
    raised_changes: usize,
}

impl Work {
    fn step(&mut self) {
        #[cfg(test)]
        {
            self.steps += 1;
        }
    }
    fn round(&mut self) {
        #[cfg(test)]
        {
            self.rounds += 1;
        }
    }
    fn removed(&mut self) {
        #[cfg(test)]
        {
            self.removed += 1;
        }
    }
    fn flip(&mut self) {
        #[cfg(test)]
        {
            self.raised_changes += 1;
        }
    }
}

#[cfg(test)]
#[path = "restore_perf_tests.rs"]
mod perf_tests;
