use crate::{DepIdx, Revision};
use std::ops::{Add, AddAssign};

#[derive(Copy, Clone, Debug)]
pub(crate) enum Invalidation {
    Outdated(Revision, DepIdx), // Dependency has both different output and different rev, need to recalc parent.
    Revisioned(Revision, DepIdx), // Dependency has different rev, but the output stays the same,
    // need to update rev and dep only
    Fresh, // Dependency is the same
}

impl Add for Invalidation {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        use Invalidation::*;
        match (self, rhs) {
            (Outdated(l, idx_l), Outdated(r, _idx_r)) if l > r => Outdated(l, idx_l),
            (Outdated(..), Outdated(r, idx_r)) => Outdated(r, idx_r),
            (_, Outdated(r, idx)) => Outdated(r, idx),
            (Outdated(l, idx), _) => Outdated(l, idx),

            (Revisioned(l, idx_l), Revisioned(r, _idx_r)) if l > r => Revisioned(l, idx_l),
            (Revisioned(..), Revisioned(r, idx_r)) => Revisioned(r, idx_r),
            (_, Revisioned(r, idx)) => Revisioned(r, idx),
            (Revisioned(l, idx), _) => Revisioned(l, idx),
            (Fresh, Fresh) => Fresh,
        }
    }
}

impl AddAssign for Invalidation {
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}
