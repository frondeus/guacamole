mod dep;
mod query_tracker;
mod storage;

pub(crate) use self::dep::{Dep, DepIdx, DepsExt};
use self::storage::{QueryCell, QueryStorage, Storage, CycleDetection};
use crate::runtime::query_tracker::QueryTracker;
use crate::{ForkId, Input, Invalidation, Query, QueryRef, Revision, System, Reservation};
use async_trait::async_trait;
use core::any::TypeId;
use futures::future::{abortable, AbortHandle, Abortable, BoxFuture};
use futures::{Future, FutureExt};
use std::collections::HashMap;
use std::fmt;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;
use tokio::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};
use tracing_futures::Instrument;

type QueriesMap = HashMap<TypeId, Box<dyn Storage>>;

#[derive(Default)]
pub struct Runtime {
    queries: Arc<RwLock<QueriesMap>>,
    handles: Arc<RwLock<Vec<AbortHandle>>>,
    rev_counter: Arc<AtomicUsize>,
    fork_counter: Arc<AtomicUsize>,
    fork_id: ForkId,
}

impl fmt::Debug for Runtime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "({:?}, {:?})", &self.current_rev(), self.fork_id)
    }
}

#[async_trait]
impl System for Runtime {
    async fn query_ref<Q: Query>(&self, query: Q) -> QueryRef<Q::Output> {
        let output = self.query_inner(query).await.output();
        QueryRef(output.unwrap())
    }

    async fn query<Q>(&self, query: Q) -> Q::Output
    where
        Q: Query,
        Q::Output: Clone,
    {
        let output = self.query_inner(query).await.output();
        (*output.unwrap()).clone()
    }

    #[tracing::instrument(skip(f))]
    async fn fork<F, Fut>(&self, f: F) -> Abortable<Fut>
    where
        F: Send + Fn(Self) -> Fut,
        Fut: Future + Send,
    {
        let fork = self.fork_inner();
        let fut = f(fork);
        let (fut, handle) = abortable(fut);
        self.handles.write().await.push(handle);

        fut
    }
}

impl Runtime {
    pub fn current_rev(&self) -> Revision {
        Revision::current(&self.rev_counter)
    }

    pub async fn query_rev<Q>(&self, query: Q) -> (Q::Output, Revision)
    where
        Q: Query,
        Q::Output: Clone,
    {
        let cell = self.query_inner(query).await;
        let rev = cell.rev();
        ((*cell.output().unwrap()).clone(), rev)
    }

    #[tracing::instrument]
    pub async fn set_input<Q: Input + Query>(&self, query: Q, data: <Q as Query>::Output) {
        let type_id = TypeId::of::<Q>();

        {
            let mut handles = self.handles.write().await;
            if !handles.is_empty() {
                tracing::debug!("Canceling ongoing requests");
            }
            for (idx, handle) in handles.drain(..).enumerate() {
                tracing::debug!("{}", idx);
                handle.abort();
            }
        }

        if !self.read_queries().await.contains_key(&type_id) {
            self.write_queries()
                .await
                .insert(type_id, Box::new(QueryStorage::<Q>::default()));
        }

        let output = data;

        let mut guard = self.write_queries().await;
        let storage = guard.get_mut(&type_id).expect("Query storage");
        let storage = storage
            .as_any_mut()
            .downcast_mut::<QueryStorage<Q>>()
            .expect("Couldn't downcast to storage");

        let rev = Revision::new(&self.rev_counter);
        storage.insert_calculated(Arc::new(query), output, rev, Default::default());
    }
}

impl Runtime {
    pub(crate) fn fork_no_inc(&self) -> Self {
        Self {
            queries: self.queries.clone(),
            handles: self.handles.clone(),
            rev_counter: self.rev_counter.clone(),
            fork_counter: self.fork_counter.clone(),
            fork_id: self.fork_id,
        }
    }

    fn fork_inner(&self) -> Self {
        let fork_id = ForkId::new(&self.fork_counter);
        Self {
            queries: self.queries.clone(),
            handles: self.handles.clone(),
            rev_counter: self.rev_counter.clone(),
            fork_counter: self.fork_counter.clone(),
            fork_id,
        }
    }

    #[tracing::instrument]
    async fn write_queries(&self) -> RwLockWriteGuard<'_, QueriesMap> {
        tracing::trace!("WRITE RW LOCK");
        self.queries.write().await
    }

    #[tracing::instrument]
    async fn read_queries(&self) -> RwLockReadGuard<'_, QueriesMap> {
        tracing::trace!("READ RW LOCK");
        self.queries.read().await
    }

    #[tracing::instrument]
    async fn dep_rev(&self, dep: &Dep) -> Revision {
        let guard = self.read_queries().await;
        let storage = guard.get(&dep.query_type()).expect("Dep storage");
        storage.dep_rev(dep)
    }

    async fn reserve_query<Q: Query>(&self, query: Arc<Q>, type_id: TypeId, current_rev: Revision) -> Reservation {
        #![allow(dead_code)]

        let mut global_guard = self.write_queries().await;
        let storage = global_guard.get_mut(&type_id).expect("Query storage");
        let storage = storage
            .as_any_mut()
            .downcast_mut::<QueryStorage<Q>>()
            .expect("Couldn't downcast to storage");

        storage.reserve(query.clone(), self.fork_id, current_rev).await
    }

    #[tracing::instrument(skip(type_id, reservation))]
    async fn recalc_query<Q: Query>(&self, query: Arc<Q>, type_id: TypeId, current_rev: Revision, reservation: Reservation) -> QueryCell<Q> {
        let _local_lock = reservation;

        let tracker = QueryTracker::new(self);

        let output = query.calc(&tracker).await;

        let deps = tracker.into_deps();
        let deps_rev = deps.last_rev();
        let rev = deps_rev.unwrap_or(current_rev);

        {
            let mut guard = self.write_queries().await;
            let storage = guard.get_mut(&type_id).expect("Query storage");
            let storage = storage
                .as_any_mut()
                .downcast_mut::<QueryStorage<Q>>()
                .expect("Couldn't downcast to storage");

            storage.insert_calculated(query, output, rev, deps)
        }
    }

    async fn recalc_outdated_dep(
        &self,
        dep: &Dep,
        caused_by: DepIdx,
        current_rev: Revision,
    ) -> Invalidation {
        let type_id = dep.query_type();
        let query = {
            let guard = self.read_queries().await;
            let storage = guard.get(&type_id).expect("Query dep storage");
            storage.dyn_query(dep)
        };
        let output = query.calc(self).await;

        {
            let mut guard = self.write_queries().await;
            let storage = guard.get_mut(&type_id).expect("Query dep storage");
            storage.update_output_dyn(dep, caused_by, output, current_rev)
        }
    }

    async fn recalc_revisioned_dep(&self, dep: &Dep, caused_by: DepIdx, rev: Revision) {
        let type_id = dep.query_type();

        let mut guard = self.write_queries().await;
        let storage = guard.get_mut(&type_id).expect("Query dep storage");
        storage.update_dep_rev(dep, caused_by, rev)
    }

    async fn recalc_rev(
        &self,
        type_id: TypeId,
        query_idx: usize,
        caused_by: DepIdx,
        rev: Revision,
    ) {
        let mut guard = self.write_queries().await;
        let storage = guard.get_mut(&type_id).expect("Query rev storage");
        storage.update_rev(query_idx, caused_by, rev)
    }

    async fn invalidation(&self, deps: &[Dep]) -> Invalidation {
        let mut invalidation = Invalidation::Fresh;

        for dep in deps {
            let dep_invalidation = self.check_invalidate(dep).await;
            invalidation += dep_invalidation;
        }

        invalidation
    }

    fn check_invalidate<'a>(&'a self, current_dep: &'a Dep) -> BoxFuture<'a, Invalidation> {
        use Invalidation::*;
        async move {
            let mut invalidation = Fresh;

            for dep in current_dep.deps() {
                let dep_invalidate = self.check_invalidate(dep).await;
                tracing::debug!({ ?dep_invalidate }, "Dep {:?}", dep);
                invalidation += dep_invalidate;
            }

            match invalidation {
                Outdated(rev, idx) => self.recalc_outdated_dep(current_dep, idx, rev).await,
                Revisioned(rev, idx) => {
                    self.recalc_revisioned_dep(current_dep, idx, rev).await;
                    Revisioned(rev, idx)
                }
                Fresh => {
                    let current_rev = self.dep_rev(current_dep).await;
                    current_dep.check_outdated(current_rev)
                }
            }
        }
        .instrument(tracing::info_span!("check_invalidate", dep = ?current_dep))
        .boxed()
    }

    #[tracing::instrument]
    async fn query_inner<Q: Query>(&self, query: Q) -> QueryCell<Q> {
        let type_id = TypeId::of::<Q>();
        let query = Arc::new(query);
        let current_rev = self.current_rev();

        let contains_query = {
            let mut guard = self.write_queries().await;
            let storage = guard.entry(type_id)
                .or_insert_with(|| Box::new(QueryStorage::<Q>::default()));

            let storage = storage.as_any_mut()
                    .downcast_mut::<QueryStorage<Q>>()
                    .expect("Query storage");

            if storage.contains_query(&query) {
                None
            }
            else {
                let local_lock = storage.reserve(query.clone(), self.fork_id, current_rev).await;
                Some(local_lock)
            }
        };

        if let Some(local_lock) = contains_query {
            self.recalc_query(query, type_id, current_rev, local_lock).await
        } else {
            let get_cell_fn = || async {
                let guard = self.read_queries().await;
                let storage = guard
                    .get(&type_id)
                    .expect("Query storage")
                    .as_any()
                    .downcast_ref::<QueryStorage<Q>>()
                    .expect("Couldn't downcast to storage");

                storage.get(&query).clone()
            };

            let mut cell = get_cell_fn().await;

            match cell.detect_cycle_or_lock(self.fork_id, self.current_rev()) {
                CycleDetection::Locked(lock) => {
                    lock.await;
                    cell = get_cell_fn().await;
                },
                CycleDetection::CycleDetected => {
                    cell.on_cycle(&query);
                },
                CycleDetection::Canceled => {
                    let local_lock = self.reserve_query(query.clone(), type_id, current_rev).await;
                    return self.recalc_query(query, type_id, current_rev, local_lock).await;
                },
                _ => (),
            }

            tracing::debug!("Load cell: {:?}", cell);

            if cell.deps().is_empty() {
                return cell;
            }

            tracing::debug!("Should I invalidate?");
            let invalidation = self.invalidation(cell.deps()).await;
            tracing::debug!("Invalidation: {:?}", invalidation);

            match invalidation {
                Invalidation::Outdated(_rev, _idx) => {
                    tracing::debug!("Query {:?} outdated. Recalc!", &query);
                    let local_lock = self.reserve_query(query.clone(), type_id, current_rev).await;
                    self.recalc_query(query, type_id, current_rev, local_lock).await
                }
                Invalidation::Revisioned(rev, idx) => {
                    tracing::debug!("Query {:?} outdated. Update revision!", &query);
                    let query_idx = cell.idx();
                    self.recalc_rev(type_id, query_idx, idx, rev).await;
                    cell
                }
                Invalidation::Fresh => cell,
            }
        }
    }
}
