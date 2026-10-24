//! Window identity survives stack changes. Only this owner mutates stack storage.
use super::ManagedWindow;
use std::{
    ops::{Index, IndexMut},
    slice,
};

pub(in crate::compositor) struct State {
    stack: Vec<ManagedWindow>,
    next_id: u64,
}

impl Default for State {
    fn default() -> Self {
        Self {
            stack: Vec::new(),
            next_id: 1,
        }
    }
}

impl State {
    /// Allocate once for this record; a removed ID is never reused.
    pub(in crate::compositor) fn insert_new(
        &mut self,
        make_window: impl FnOnce(u64) -> ManagedWindow,
    ) -> u64 {
        let id = self.next_id;
        self.next_id = self.next_id.checked_add(1).expect("window IDs exhausted");
        let mut window = make_window(id);
        window.id = id;
        self.stack.push(window);
        id
    }

    pub(in crate::compositor) fn index(&self, id: u64) -> Option<usize> {
        self.stack.iter().position(|window| window.id == id)
    }

    pub(in crate::compositor) fn by_id(&self, id: u64) -> Option<&ManagedWindow> {
        self.index(id).map(|index| &self.stack[index])
    }

    pub(in crate::compositor) fn by_id_mut(&mut self, id: u64) -> Option<&mut ManagedWindow> {
        let index = self.index(id)?;
        Some(&mut self.stack[index])
    }

    pub(in crate::compositor) fn len(&self) -> usize {
        self.stack.len()
    }
    pub(in crate::compositor) fn get(&self, index: usize) -> Option<&ManagedWindow> {
        self.stack.get(index)
    }
    pub(in crate::compositor) fn iter(&self) -> slice::Iter<'_, ManagedWindow> {
        self.stack.iter()
    }
    pub(in crate::compositor) fn iter_mut(&mut self) -> slice::IterMut<'_, ManagedWindow> {
        self.stack.iter_mut()
    }

    /// Used only by the compositor's retirement path, never by protocol adapters.
    pub(super) fn remove_retired(&mut self, id: u64) {
        if let Some(index) = self.index(id) {
            self.stack.remove(index);
        }
    }

    /// Move the ordered family before a surviving window, or to the top.
    /// All inputs are identities captured before any index can change.
    pub(super) fn restack(&mut self, family: &[u64], before: Option<u64>) {
        let mut windows = Vec::with_capacity(family.len());
        for id in family {
            if let Some(index) = self.index(*id) {
                windows.push(self.stack.remove(index));
            }
        }
        let index = before
            .and_then(|id| self.index(id))
            .unwrap_or(self.stack.len());
        self.stack.splice(index..index, windows);
    }
}

// Local synchronous traversals may update records, but cannot resize or reorder
// storage. Operations that call back into App must retain an ID, not an index.
impl Index<usize> for State {
    type Output = ManagedWindow;
    fn index(&self, index: usize) -> &Self::Output {
        &self.stack[index]
    }
}
impl IndexMut<usize> for State {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.stack[index]
    }
}
impl<'a> IntoIterator for &'a State {
    type Item = &'a ManagedWindow;
    type IntoIter = slice::Iter<'a, ManagedWindow>;
    fn into_iter(self) -> Self::IntoIter {
        self.stack.iter()
    }
}
impl<'a> IntoIterator for &'a mut State {
    type Item = &'a mut ManagedWindow;
    type IntoIter = slice::IterMut<'a, ManagedWindow>;
    fn into_iter(self) -> Self::IntoIter {
        self.stack.iter_mut()
    }
}
