use crate::runtime::Dep;
use crate::{Query, QueryRef, Runtime, System};
use async_trait::async_trait;
use std::fmt;
use std::sync::Arc;
use tokio::sync::RwLock;

pub(super) struct QueryTracker {
    runtime: Runtime,
    deps: Arc<RwLock<Vec<Dep>>>,
}

impl fmt::Debug for QueryTracker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", &self.runtime)
    }
}

impl QueryTracker {
    pub fn new(runtime: &Runtime) -> Self {
        Self {
            runtime: runtime.fork(),
            deps: Default::default(),
        }
    }

    pub fn into_deps(self) -> Vec<Dep> {
        Arc::try_unwrap(self.deps).unwrap().into_inner()
    }

    #[tracing::instrument(skip(dep))]
    async fn add_dep(&self, dep: Dep) {
        tracing::trace!("WRITE RW LOCK");
        self.deps.write().await.push(dep);
    }
}

#[async_trait]
impl System for QueryTracker {
    async fn query_ref<Q: Query>(&self, query: Q) -> QueryRef<<Q as Query>::Output> {
        let cell = self.runtime.query_inner(query).await;

        let dep = cell.as_dep();
        self.add_dep(dep).await;
        QueryRef(cell.output())
    }

    async fn query<Q>(&self, query: Q) -> <Q as Query>::Output
    where
        Q: Query,
        Q::Output: Clone,
    {
        let cell = self.runtime.query_inner(query).await;

        let dep = cell.as_dep();
        self.add_dep(dep).await;

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
