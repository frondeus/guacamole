use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Revision(usize);

impl Revision {
    pub(crate) fn new(counter: &Arc<AtomicUsize>) -> Self {
        let id = counter.fetch_add(1, Ordering::SeqCst);
        Self(id + 1)
    }

    pub(crate) fn current(counter: &Arc<AtomicUsize>) -> Self {
        let id = counter.load(Ordering::SeqCst);
        Self(id)
    }
}

impl fmt::Debug for Revision {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "R{:?}", &self.0)
    }
}
