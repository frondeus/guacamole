use std::sync::Arc;
use core::any::Any;
use core::any::TypeId;
use std::collections::HashMap;
use tokio::sync::RwLock;
use crate::{Query, QueryRef};

pub struct QueryStorage<Q: Query>(HashMap<Q, Arc<Q::Output>>);

impl<Q: Query> Default for QueryStorage<Q> {
    fn default() -> Self { Self(Default::default()) }
}

pub struct System {
    pub text: String, //TODO. Replace with input query

    queries: RwLock<HashMap<TypeId, Box<dyn Any + Send + Sync>>>,
}

impl System {
    pub fn new(text: impl ToString) -> Self {
        Self {
            text: text.to_string(),
            queries: Default::default()
        }
    }

    async fn query_inner<Q: Query>(&self, query: Q) -> Arc<Q::Output> {
        let type_id = TypeId::of::<Q>();

        let contains_query = if !self.queries.read().await.contains_key(&type_id) {
            self.queries.write().await.insert(type_id, Box::new( QueryStorage::<Q>::default() ));
            false
        }
        else {
            self.queries.read().await.get(&type_id).expect("Query Storage").downcast_ref::<QueryStorage<Q>>()
                .expect("Query storage")
                .0.contains_key(&query)
        };

        if !contains_query {
            let output = Arc::new(query.calc(&self).await);

            let mut guard = self.queries.write().await;
            let storage = guard.get_mut(&type_id).expect("Query storage");
            let storage = storage.downcast_mut::<QueryStorage<Q>>().expect("Couldn't downcast to storage");
            storage.0.insert(query, output.clone());
            output
        }
        else {
            let guard = self.queries.read().await;
            let storage = guard.get(&type_id).expect("Query storage").downcast_ref::<QueryStorage<Q>>()
                .expect("Couldn't downcast to storage");

            let any = storage.0.get(&query).expect("Query");
            any.clone()
        }
    }

    pub async fn query_ref<Q: Query>(&self, query: Q) -> QueryRef<Q::Output> {
        let output = self.query_inner(query).await;
        QueryRef(output)
    }

    pub async fn query<Q>(&self, query: Q) -> Q::Output
        where Q: Query,
              Q::Output: Clone {
        let output = self.query_inner(query).await;
        (*output).clone()
    }

}
