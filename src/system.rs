use crate::{Query, QueryRef};
use async_trait::async_trait;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::fmt;

#[async_trait]
pub trait System: Send + Sync + 'static {
    async fn query_ref<Q: Query>(&self, query: Q) -> QueryRef<Q::Output>;

    async fn query<Q>(&self, query: Q) -> Q::Output
    where
        Q: Query,
        Q::Output: Clone;

    fn fork(&self) -> Self;
}

#[derive(Clone, Copy, PartialEq, Eq, Ord, PartialOrd)]
pub(crate) struct ForkId(usize);
impl ForkId {
    pub(crate) fn new(counter: &Arc<AtomicUsize>) -> Self {
        let id = counter.fetch_add(1, Ordering::SeqCst);
        Self(id + 1)
    }
}

impl fmt::Debug for ForkId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "F{:?}", &self.0)
    }
}

impl Default for ForkId {
    fn default() -> Self {
        Self(1)
    }
}