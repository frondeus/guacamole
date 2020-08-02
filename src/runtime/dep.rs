use crate::{Invalidation, Revision};
use std::any::TypeId;
use std::fmt;

#[derive(Copy, Clone, PartialEq)]
pub(crate) struct DepIdx {
    pub(crate) query_name: &'static str, //TODO only in debug
    pub(crate) query_type: TypeId,
    pub(crate) query_idx: usize,
}

impl fmt::Debug for DepIdx {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} @ {}", self.query_name, self.query_idx)
    }
}

#[derive(Clone)]
pub(crate) struct Dep {
    pub(crate) idx: DepIdx,
    pub(crate) query_rev: Revision,
    pub(crate) query_deps: Vec<Dep>,
}

impl Dep {
    pub fn query_type(&self) -> TypeId {
        self.idx.query_type
    }

    pub fn check_outdated(&self, current_rev: Revision) -> Invalidation {
        if self.query_rev < current_rev {
            Invalidation::Outdated(current_rev, self.idx)
        } else {
            Invalidation::Fresh
        }
    }

    pub fn deps(&self) -> &[Dep] {
        &self.query_deps
    }
}

impl fmt::Debug for Dep {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "({:?}: {:?})", self.query_rev, self.idx,)?;
        if !self.query_deps.is_empty() {
            write!(f, "=>{:?}", self.query_deps)?;
        }
        Ok(())
    }
}

pub(crate) trait DepsExt {
    fn last_rev(&self) -> Option<Revision>;
}

impl DepsExt for Vec<Dep> {
    fn last_rev(&self) -> Option<Revision> {
        self.iter().map(|d| d.query_rev).max()
    }
}
