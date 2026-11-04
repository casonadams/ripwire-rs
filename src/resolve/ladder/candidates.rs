use crate::domain::{NodeId, Symbol};
use lasso::Spur;

#[derive(Debug, Clone)]
pub struct CandBuffer {
    inline: [NodeId; 8],
    len: usize,
    heap: Vec<NodeId>,
}

impl CandBuffer {
    #[inline]
    #[must_use]
    pub fn new() -> Self {
        Self {
            inline: [0; 8],
            len: 0,
            heap: Vec::new(),
        }
    }

    #[inline]
    pub fn push(&mut self, id: NodeId) {
        if self.heap.is_empty() && self.len < 8 {
            self.inline[self.len] = id;
            self.len += 1;
        } else {
            if self.heap.is_empty() {
                self.heap.reserve(self.len + 8);
                self.heap.extend_from_slice(&self.inline[..self.len]);
            }
            self.heap.push(id);
        }
    }

    #[inline]
    #[must_use]
    pub fn as_slice(&self) -> &[NodeId] {
        if self.heap.is_empty() {
            &self.inline[..self.len]
        } else {
            &self.heap
        }
    }

    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0 && self.heap.is_empty()
    }
}

impl Default for CandBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl std::ops::Deref for CandBuffer {
    type Target = [NodeId];

    #[inline]
    fn deref(&self) -> &[NodeId] {
        self.as_slice()
    }
}

impl AsRef<[NodeId]> for CandBuffer {
    #[inline]
    fn as_ref(&self) -> &[NodeId] {
        self.as_slice()
    }
}

impl<'a> IntoIterator for &'a CandBuffer {
    type Item = &'a NodeId;
    type IntoIter = std::slice::Iter<'a, NodeId>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.as_slice().iter()
    }
}

pub(super) fn filter_matching_scope(cands: &CandBuffer, symbols: &[Symbol], q: Spur) -> CandBuffer {
    let mut qualified = CandBuffer::default();
    for &id in cands {
        let s = &symbols[id as usize];
        if s.scope == Some(q) || s.name == q {
            qualified.push(id);
        }
    }
    qualified
}

pub(super) fn filter_in_file(cands: &CandBuffer, symbols: &[Symbol], file_id: u32) -> CandBuffer {
    let mut in_file = CandBuffer::default();
    for &id in cands {
        if symbols[id as usize].file_id == file_id {
            in_file.push(id);
        }
    }
    in_file
}

pub(super) fn filter_unscoped(cands: &CandBuffer, symbols: &[Symbol]) -> CandBuffer {
    let mut free = CandBuffer::default();
    for &id in cands {
        if symbols[id as usize].scope.is_none() {
            free.push(id);
        }
    }
    free
}
