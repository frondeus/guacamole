mod storage;

use self::storage::{Storage, QueryCell, QueryStorage, Dep};
use crate::{Input, Query, QueryRef};
use core::any::TypeId;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::RwLock;
use crate::system::storage::{DepsExt, Outdated};
use async_trait::async_trait;
use std::sync::Arc;
use itertools::Itertools;

#[async_trait]
pub trait System: Send + Sync + 'static {
    async fn query_ref<Q: Query>(&self, query: Q) -> QueryRef<Q::Output>;

    async fn query<Q>(&self, query: Q) -> Q::Output
        where
            Q: Query,
            Q::Output: Clone;

    fn fork(&self) -> Self;
}

#[derive(Default)]
pub struct Guacamole {
    queries: Arc<RwLock<HashMap<TypeId, Box<dyn Storage>>>>,
    counter: Arc<AtomicUsize>,
}

#[async_trait]
impl System for Guacamole {
    async fn query_ref<Q: Query>(&self, query: Q) -> QueryRef<Q::Output> {
        let output = self.query_inner(query).await.output();
        QueryRef(output)
    }

    async fn query<Q>(&self, query: Q) -> Q::Output
        where
            Q: Query,
            Q::Output: Clone,
    {
        let output = self.query_inner(query).await.output();
        (*output).clone()
    }

    fn fork(&self) -> Self {
        Self {
            counter: self.counter.clone(),
            queries: self.queries.clone(),
        }
    }
}

struct QueryTracker {
    guacamole: Guacamole,
    deps: Arc<RwLock<Vec<Dep>>>
}

#[async_trait]
impl System for QueryTracker {
    async fn query_ref<Q: Query>(&self, query: Q) -> QueryRef<<Q as Query>::Output> {
        let cell = self.guacamole.query_inner(query).await;

        let dep = cell.as_dep();
        self.deps.write().await.push(dep);
        QueryRef(cell.output())
    }

    async fn query<Q>(&self, query: Q) -> <Q as Query>::Output where
        Q: Query,
        Q::Output: Clone {
        let cell = self.guacamole.query_inner(query).await;

        let dep = cell.as_dep();
        self.deps.write().await.push(dep);

        let output = cell.output();
        (*output).clone()
    }

    fn fork(&self) -> Self {
        Self {
            guacamole: self.guacamole.fork(),
            deps: self.deps.clone()
        }
    }
}

impl Guacamole {
    pub fn current_rev(&self) -> usize {
        self.counter.load(Ordering::SeqCst)
    }

    pub async fn query_rev<Q>(&self, query: Q) -> (Q::Output, usize)
    where
        Q: Query,
        Q::Output: Clone,
    {
        let cell = self.query_inner(query).await;
        let rev = cell.rev();
        ((*cell.output()).clone(), rev)
    }

    pub async fn set_input<Q: Input + Query>(&self, query: Q, data: <Q as Query>::Output) {
        log::debug!("Set Input: {:?}", &query);
        let type_id = TypeId::of::<Q>();

        if !self.queries.read().await.contains_key(&type_id) {
            self.queries
                .write()
                .await
                .insert(type_id, Box::new(QueryStorage::<Q>::default()));
        }

        let output = data;

        let mut guard = self.queries.write().await;
        let storage = guard.get_mut(&type_id).expect("Query storage");
        let storage = storage
            .as_any_mut()
            .downcast_mut::<QueryStorage<Q>>()
            .expect("Couldn't downcast to storage");

        let rev = self.counter.fetch_add(1, Ordering::SeqCst);
        storage.insert(query, output, rev + 1, Default::default());
    }
}

impl Guacamole {
    async fn dep_rev(&self, dep: &Dep) -> usize {
        let guard = self.queries.read().await;
        let storage = guard.get(&dep.query_type()).expect("Dep storage");
        storage.dep_rev(dep)
    }

    fn track(&self) -> QueryTracker {
        QueryTracker {
            guacamole: self.fork(),
            deps: Default::default(),
        }
    }

    async fn recalc_query<Q: Query>(&self, query: Q, type_id: TypeId) -> QueryCell<Q> {
        log::debug!("Recalc query: {:?}", &query);

        let tracker = self.track();

        let output = query.calc(&tracker).await;

        let deps = Arc::try_unwrap(tracker.deps).unwrap().into_inner();
        let deps_rev = deps.last_rev();

        let mut guard = self.queries.write().await;
        let storage = guard.get_mut(&type_id).expect("Query storage");
        let storage = storage
            .as_any_mut()
            .downcast_mut::<QueryStorage<Q>>()
            .expect("Couldn't downcast to storage");

        // Todo, check recalced output with previous output
        let rev = deps_rev.unwrap_or_else(|| self.current_rev());
        storage.insert(query, output, rev, deps)
    }

    async fn check_invalidate(&self, dep: &Dep) -> Result<(), Outdated> {
        for dep in dep.deps() {
            let current_rev = self.dep_rev(dep).await;
            if dep.check_outdated(current_rev).is_err() {
                // TODO: Recalc query to see if output changed.
                return Err(Outdated);
            }
        }

        let current_rev = self.dep_rev(dep).await;
        dep.check_outdated(current_rev)
    }

    async fn query_inner<Q: Query>(&self, query: Q) -> QueryCell<Q> {
        log::debug!("Query: {:?}", &query);
        let type_id = TypeId::of::<Q>();

        let contains_query = if !self.queries.read().await.contains_key(&type_id) {
            self.queries
                .write()
                .await
                .insert(type_id, Box::new(QueryStorage::<Q>::default()));
            false
        } else {
            self.queries
                .read()
                .await
                .get(&type_id)
                .expect("Query Storage")
                .as_any()
                .downcast_ref::<QueryStorage<Q>>()
                .expect("Query storage")
                .contains_query(&query)
        };

        if !contains_query {
            self.recalc_query(query, type_id).await
        } else {
            let mut cell = {

                let guard = self.queries.read().await;
                let storage = guard
                    .get(&type_id)
                    .expect("Query storage")
                    .as_any()
                    .downcast_ref::<QueryStorage<Q>>()
                    .expect("Couldn't downcast to storage");

                storage.get(&query).clone()
            };
            log::debug!("Load cell: {:?}", cell);

            let deps_futures = cell.deps().iter().map(|dep| {
                self.check_invalidate(dep)
            }).collect_vec();

            let invalidate = futures::future::try_join_all(deps_futures).await.is_err();

            if invalidate {
                log::debug!("Invalidate query: {:?}", &query);
                cell = self.recalc_query(query, type_id).await;
            }
            cell
        }
    }
}
