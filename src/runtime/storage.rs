use crate::runtime::dep::{Dep, DepIdx};
use crate::{DynQuery, ForkId, Invalidation, Query, Revision, Runtime, ReservationReader, Reservation};
use async_trait::async_trait;
use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;


#[derive(Clone)]
pub(crate) enum CycleDetection {
    CycleDetected,
    Locked(ReservationReader),
    Canceled,
    Ok
}

#[derive(Debug)]
pub(crate) enum QueryOutput<Q: Query> {
    Calculating(ForkId, Revision, ReservationReader),
    Calculated(Arc<Q::Output>),
}

impl<Q: Query> Clone for QueryOutput<Q> {
    fn clone(&self) -> Self {
        match self {
            Self::Calculated(out) => Self::Calculated(out.clone()),
            Self::Calculating(fork, rev, lock) => Self::Calculating(*fork, *rev, lock.clone()),
        }
    }
}

impl<Q: Query> QueryOutput<Q> {
    pub fn unwrap(self) -> Arc<Q::Output> {
        match self {
            Self::Calculated(out) => out,
            Self::Calculating(fork, rev, _lock) => {
                panic!("Still calculating by {:?} {:?}", fork, rev)
            }
        }
    }

    pub fn unwrap_ref(&self) -> &Arc<Q::Output> {
        match self {
            Self::Calculated(out) => out,
            Self::Calculating(fork, rev, _lock) => {
                panic!("Still calculating by {:?} {:?}", fork, rev)
            }
        }
    }
}

pub(crate) struct QueryCell<Q: Query> {
    output: QueryOutput<Q>, // Arc<Q::Output>,
    idx: usize,
    rev: Revision,
    deps: Vec<Dep>,
}

impl<Q: Query> fmt::Debug for QueryCell<Q> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = format!("QueryCell<{}>", std::any::type_name::<Q>());
        f.debug_struct(&name)
            .field("output", &self.output)
            .field("idx", &self.idx)
            .field("rev", &self.rev)
            .field("deps", &self.deps)
            .finish()
    }
}

impl<Q: Query> QueryCell<Q> {
    fn calculated(output: Arc<Q::Output>, rev: Revision, idx: usize, deps: Vec<Dep>) -> Self {
        Self {
            output: QueryOutput::Calculated(output),
            rev,
            idx,
            deps,
        }
    }

    fn calculating(fork: ForkId, rev: Revision, idx: usize, lock: ReservationReader) -> Self {
        Self {
            output: QueryOutput::Calculating(fork, rev, lock),
            rev,
            idx,
            deps: Default::default(),
        }
    }

    pub fn output(self) -> QueryOutput<Q> {
        self.output
    }

    pub fn rev(&self) -> Revision {
        self.rev
    }

    pub fn idx(&self) -> usize {
        self.idx
    }

    pub fn as_dep(&self) -> Dep {
        Dep {
            idx: DepIdx {
                query_name: std::any::type_name::<Q>(),
                query_type: TypeId::of::<Q>(),
                query_idx: self.idx,
            },
            query_rev: self.rev,
            query_deps: self.deps.clone(),
        }
    }

    pub fn deps(&self) -> &[Dep] {
        &self.deps
    }

    pub fn on_cycle(&mut self, query: &Q) {
        let o = query.on_cycle();
        self.output = QueryOutput::Calculated(Arc::new(o));
    }

    pub fn detect_cycle_or_lock(
        &self,
        current_fork: ForkId,
        current_rev: Revision,
    ) -> CycleDetection {
        let output = &self.output;
        match output {
            QueryOutput::Calculating(fork, rev, lock) => {
                tracing::warn!(
                    "Calculating {:?}, {:?}. Current {:?}, {:?}",
                    fork,
                    rev,
                    current_fork,
                    current_rev
                );
                if *fork == current_fork && *rev == current_rev {
                    return CycleDetection::CycleDetected;
                }
                else if *rev != current_rev {
                    return CycleDetection::Canceled;
                }
                CycleDetection::Locked(lock.clone())
            }
            _ => CycleDetection::Ok,
        }
    }

    fn update_rev(&mut self, caused_by: DepIdx, rev: Revision) {
        self.rev = rev;

        fn rec(dep: &mut Dep, caused_by: DepIdx, rev: Revision) -> bool {
            let changed_children = dep
                .query_deps
                .iter_mut()
                .any(|dep| rec(dep, caused_by, rev));

            if changed_children || dep.idx == caused_by {
                dep.query_rev = rev;
                true
            } else {
                false
            }
        }

        self.deps.iter_mut().for_each(|d| {
            rec(d, caused_by, rev);
        });
    }
}

impl<Q: Query> Clone for QueryCell<Q> {
    fn clone(&self) -> Self {
        Self {
            output: self.output.clone(),
            rev: self.rev,
            idx: self.idx,
            deps: self.deps.clone(),
        }
    }
}

#[async_trait]
pub(crate) trait Storage: Any + Send + Sync {
    fn dyn_query(&self, dep: &Dep) -> Box<dyn DynQuery>;
    fn update_output_dyn(
        &mut self,
        dep: &Dep,
        caused_by: DepIdx,
        dyn_output: Box<dyn Any + Send + Sync>,
        rev: Revision,
    ) -> Invalidation;
    fn update_dep_rev(&mut self, dep: &Dep, caused_by: DepIdx, rev: Revision);
    fn update_rev(&mut self, idx: usize, caused_by: DepIdx, rev: Revision);
    fn dep_rev(&self, dep: &Dep) -> Revision;
    fn as_any(&self) -> &(dyn Any + Send + Sync);
    fn as_any_mut(&mut self) -> &mut (dyn Any + Send + Sync);
}

struct DynQueryWrapper<Q: Query> {
    query: Arc<Q>,
}
#[async_trait]
impl<Q: Query> DynQuery for DynQueryWrapper<Q> {
    async fn calc(&self, system: &Runtime) -> Box<dyn Any + Send + Sync> {
        let out = self.query.calc(system).await;
        Box::new(out)
    }
}

pub(crate) struct QueryStorage<Q: Query> {
    queries: HashMap<Arc<Q>, usize>,
    cells: Vec<QueryCell<Q>>,
}

impl<Q: Query> Default for QueryStorage<Q> {
    fn default() -> Self {
        Self {
            queries: Default::default(),
            cells: Default::default(),
        }
    }
}

#[async_trait]
impl<Q: Query> Storage for QueryStorage<Q> {
    #[tracing::instrument(skip(self))]
    fn dyn_query(&self, dep: &Dep) -> Box<dyn DynQuery> {
        let idx = dep.idx.query_idx;
        let query = self
            .queries
            .iter()
            .find_map(|(key, &val)| if val == idx { Some(key) } else { None })
            .unwrap()
            .clone();
        Box::new(DynQueryWrapper { query })
    }

    #[tracing::instrument(skip(self))]
    fn update_rev(&mut self, idx: usize, caused_by: DepIdx, rev: Revision) {
        let cell = &mut self.cells[idx];
        tracing::debug!("From: {:?}", &cell);

        cell.update_rev(caused_by, rev);

        tracing::debug!("To: {:?}", &cell);
    }

    #[tracing::instrument(skip(self))]
    fn update_dep_rev(&mut self, dep: &Dep, caused_by: DepIdx, rev: Revision) {
        let idx = dep.idx.query_idx;

        self.update_rev(idx, caused_by, rev)
    }

    #[tracing::instrument(skip(self, dyn_output))]
    fn update_output_dyn(
        &mut self,
        dep: &Dep,
        caused_by: DepIdx,
        dyn_output: Box<dyn Any + Send + Sync>,
        rev: Revision,
    ) -> Invalidation {
        let idx = dep.idx.query_idx;
        let dyn_output: Box<dyn Any> = dyn_output;
        let output = dyn_output.downcast::<Q::Output>().unwrap();
        let cell = &mut self.cells[idx];
        tracing::debug!("From: {:?}", &cell);

        cell.update_rev(caused_by, rev);

        if **cell.output.unwrap_ref() != *output {
            cell.output = QueryOutput::Calculated(Arc::from(output));

            tracing::debug!("Into: {:?}", &cell);
            tracing::debug!("Output is different. Outdated!");

            Invalidation::Outdated(rev, caused_by)
        } else {
            tracing::debug!("Into: {:?}", &cell);
            tracing::debug!("Output is same");
            Invalidation::Revisioned(rev, caused_by)
        }
    }

    fn dep_rev(&self, dep: &Dep) -> Revision {
        let idx = dep.idx.query_idx;
        self.cells[idx].rev
    }

    fn as_any(&self) -> &(dyn Any + Send + Sync) {
        self
    }

    fn as_any_mut(&mut self) -> &mut (dyn Any + Send + Sync) {
        self
    }
}

impl<Q: Query> QueryStorage<Q> {
    pub fn get(&self, query: &Q) -> &QueryCell<Q> {
        let idx = self.queries[query];
        &self.cells[idx]
    }

    pub fn contains_query(&self, query: &Q) -> bool {
        self.queries.contains_key(query)
    }

    pub async fn reserve(
        &mut self,
        query: Arc<Q>,
        fork: ForkId,
        current_rev: Revision,
    ) -> Reservation {
        let idx = self.queries.get(&query).copied();

        let (reservation, lock) = Reservation::new().await;

        match idx {
            None => {
                let idx = self.cells.len();

                let cell = QueryCell::calculating(fork, current_rev, idx, lock);
                self.cells.push(cell);
                self.queries.insert(query, idx);
            }
            Some(idx) => {
                let cell = &mut self.cells[idx];
                cell.output = QueryOutput::Calculating(fork, current_rev, lock);
                cell.rev = current_rev;
            }
        }

        reservation
    }

    pub fn insert_calculated(
        &mut self,
        query: Arc<Q>,
        output: Q::Output,
        rev: Revision,
        deps: Vec<Dep>,
    ) -> QueryCell<Q> {
        let idx = self.queries.get(&query).copied();

        match idx {
            None => {
                let idx = self.cells.len();

                let cell = QueryCell::calculated(Arc::new(output), rev, idx, deps);
                self.cells.push(cell.clone());
                self.queries.insert(query, idx);
                cell
            }
            Some(idx) => {
                let cell = &mut self.cells[idx];
                cell.output = QueryOutput::Calculated(Arc::new(output));
                cell.rev = rev;
                cell.deps = deps;

                cell.clone()
            }
        }
    }
}
