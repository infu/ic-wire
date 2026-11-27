use crate::{Error, LimitKind, Limits};
use core::cell::Cell;

pub(super) struct Budget {
    nodes: Cell<usize>,
    units: Cell<usize>,
    depth: usize,
    refused: Cell<Option<LimitKind>>,
}

impl Budget {
    pub(super) fn new(limits: Limits) -> Self {
        Self {
            nodes: Cell::new(limits.nodes),
            units: Cell::new(limits.allocation.saturating_sub(limits.bytes) / 8),
            depth: limits.depth,
            refused: Cell::new((limits.allocation < limits.bytes).then_some(LimitKind::Allocation)),
        }
    }

    #[inline(always)]
    pub(super) fn enter(&self, depth: usize, units: usize) -> bool {
        self.enter_many(depth, units, 1)
    }

    // Projection steps do not allocate. Leave the allocation ledger untouched
    // on success and reuse the full path for first-error classification.
    #[inline(never)]
    pub(super) fn enter_scope(&self, depth: usize) -> bool {
        let nodes = self.nodes.get();
        if depth <= self.depth && nodes != 0 && self.refused.get().is_none() {
            self.nodes.set(nodes - 1);
            true
        } else {
            self.enter(depth, 0)
        }
    }

    #[inline(never)]
    pub(super) fn enter_nested_scopes(&self, outer: usize, inner: usize) -> bool {
        let nodes = self.nodes.get();
        if inner <= self.depth && nodes >= 2 && self.refused.get().is_none() {
            self.nodes.set(nodes - 2);
            true
        } else {
            self.enter_scope(outer) && self.enter_scope(inner)
        }
    }

    // Keep one copy of the reason-selecting debit across codec visitors at opt-z.
    #[inline(never)]
    pub(super) fn enter_many(&self, depth: usize, units: usize, count: usize) -> bool {
        let nodes = self.nodes.get();
        let available = self.units.get();
        if (depth <= self.depth)
            & (nodes >= count)
            & (available >= units)
            & self.refused.get().is_none()
        {
            self.units.set(available - units);
            self.nodes.set(nodes - count);
            true
        } else {
            self.refuse_enter(depth, count);
            false
        }
    }

    #[cold]
    #[inline(never)]
    fn refuse_enter(&self, depth: usize, count: usize) {
        if self.refused.get().is_none() {
            let reason = if depth > self.depth {
                Some(LimitKind::Depth)
            } else if self.nodes.get() < count {
                Some(LimitKind::Nodes)
            } else {
                Some(LimitKind::Allocation)
            };
            self.refused.set(reason);
        }
    }

    pub(super) fn refuse(&self) {
        if self.refused.get().is_none() {
            self.refused.set(Some(LimitKind::Allocation));
        }
    }

    #[inline(always)]
    pub(super) fn charge_buffer(&self, units: usize) -> bool {
        // A buffer debit can succeed after a prior refusal without clearing it.
        // Refusal remains sticky for subsequent visits and error classification.
        match self.units.get().checked_sub(units) {
            Some(remaining) => {
                self.units.set(remaining);

                true
            }
            None => {
                self.refuse();

                false
            }
        }
    }

    pub(super) fn error(&self) -> Error {
        self.refused.get().map_or(Error::Malformed, Error::Limit)
    }

    pub(super) fn check(&self) -> Result<(), Error> {
        self.refused
            .get()
            .map_or(Ok(()), |kind| Err(Error::Limit(kind)))
    }

    pub(super) fn refuse_with(&self, kind: LimitKind) -> Error {
        let first = self.refused.get().unwrap_or(kind);
        self.refused.set(Some(first));
        Error::Limit(first)
    }

    pub(super) fn scratch_headroom(&self) -> usize {
        self.units.get() * 8
    }

    // A failed projection dropped its temporary views. Its visits still
    // count, and heap scratch is handled by the caller's checkpoint.
    pub(super) fn resume_after_scratch(&self, allocation_checkpoint: usize) {
        if self.refused.get() == Some(LimitKind::Scratch) {
            self.units.set(allocation_checkpoint / 8);
            self.refused.set(None);
        }
    }
}
