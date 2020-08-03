use crate::{Query, QueryRef};
use async_trait::async_trait;

#[async_trait]
pub trait System: Send + Sync + 'static {
    async fn query_ref<Q: Query>(&self, query: Q) -> QueryRef<Q::Output>;

    async fn query<Q>(&self, query: Q) -> Q::Output
    where
        Q: Query,
        Q::Output: Clone;

    fn fork(&self) -> Self;
}
