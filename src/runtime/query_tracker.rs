use async_trait::async_trait;
use crate::{Runtime, System, Query, QueryRef};
use std::sync::Arc;
use tokio::sync::RwLock;
use super::storage::Dep;

pub(super) struct QueryTracker {
    runtime: Runtime,
    deps: Arc<RwLock<Vec<Dep>>>,
}


impl QueryTracker {
    pub fn new(runtime: &Runtime) -> Self {
        Self { runtime: runtime.fork(), deps: Default::default() }
    }

    pub fn into_deps(self) -> Vec<Dep> {
        Arc::try_unwrap(self.deps).unwrap().into_inner()
    }
}

#[async_trait]
impl System for QueryTracker {
    async fn query_ref<Q: Query>(&self, query: Q) -> QueryRef<<Q as Query>::Output> {
        let cell = self.runtime.query_inner(query).await;

        let dep = cell.as_dep();
        self.deps.write().await.push(dep);
        QueryRef(cell.output())
    }

    async fn query<Q>(&self, query: Q) -> <Q as Query>::Output
        where
            Q: Query,
            Q::Output: Clone,
    {
        let cell = self.runtime.query_inner(query).await;

        let dep = cell.as_dep();
        self.deps.write().await.push(dep);

        let output = cell.output();
        (*output).clone()
    }

    fn fork(&self) -> Self {
        Self {
            runtime: self.runtime.fork(),
            deps: self.deps.clone(),
        }
    }
}
