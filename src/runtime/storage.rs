use crate::Query;
use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

pub(crate) struct Outdated;

#[derive(Clone)]
pub(crate) struct Dep {
    query_type: TypeId,
    query_idx: usize,
    query_rev: usize,
    query_deps: Vec<Dep>,
}
impl Dep {
    pub fn query_type(&self) -> TypeId {
        self.query_type
    }

    pub fn check_outdated(&self, current_rev: usize) -> Result<(), Outdated> {
        if self.query_rev < current_rev {
            return Err(Outdated);
        }
        Ok(())
    }

    pub fn deps(&self) -> &[Dep] {
        &self.query_deps
    }
}

impl fmt::Debug for Dep {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Dep {{ {:?}[{}], rev: {}, deps: {:?} }}",
            self.query_type, self.query_idx, self.query_rev, self.query_deps
        )
    }
}

pub(crate) trait DepsExt {
    fn last_rev(&self) -> Option<usize>;
}

impl DepsExt for Vec<Dep> {
    fn last_rev(&self) -> Option<usize> {
        self.iter().map(|d| d.query_rev).max()
    }
}

pub(crate) struct QueryCell<Q: Query> {
    output: Arc<Q::Output>,
    idx: usize,
    rev: usize,
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
    fn new(output: Arc<Q::Output>, rev: usize, idx: usize, deps: Vec<Dep>) -> Self {
        Self {
            output,
            rev,
            idx,
            deps,
        }
    }

    pub fn output(self) -> Arc<Q::Output> {
        self.output
    }

    pub fn rev(&self) -> usize {
        self.rev
    }

    pub fn as_dep(&self) -> Dep {
        Dep {
            query_type: TypeId::of::<Q>(),
            query_idx: self.idx,
            query_rev: self.rev,
            query_deps: self.deps.clone(),
        }
    }

    pub fn deps(&self) -> &[Dep] {
        &self.deps
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

pub(crate) trait Storage: Any + Send + Sync {
    fn dep_rev(&self, dep: &Dep) -> usize;
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

pub(crate) struct QueryStorage<Q: Query> {
    queries: HashMap<Q, usize>,
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

impl<Q: Query> Storage for QueryStorage<Q> {
    fn dep_rev(&self, dep: &Dep) -> usize {
        let idx = dep.query_idx;
        self.cells[idx].rev
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

impl<Q: Query> QueryStorage<Q> {
    pub fn get(&self, query: &Q) -> &QueryCell<Q> {
        let idx = self.queries[query];
        &self.cells[idx]
    }

    pub fn insert(
        &mut self,
        query: Q,
        output: Q::Output,
        rev: usize,
        deps: Vec<Dep>,
    ) -> QueryCell<Q> {
        let idx = self.queries.get(&query).copied();

        match idx {
            None => {
                let idx = self.cells.len();

                let cell = QueryCell::new(Arc::new(output), rev, idx, deps);
                self.cells.push(cell.clone());
                self.queries.insert(query, idx);
                cell
            }
            Some(idx) => {
                let cell = &mut self.cells[idx];
                cell.output = Arc::new(output);
                cell.rev = rev;
                cell.deps = deps;

                cell.clone()
            }
        }
    }

    pub fn contains_query(&self, query: &Q) -> bool {
        self.queries.contains_key(query)
    }
}
