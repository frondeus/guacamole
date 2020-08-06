use async_trait::async_trait;
use guacamole::test_common::init_log;
use guacamole::{Input, Query, Runtime, System};
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Hash, PartialEq, Eq, Debug, Clone)]
struct A;
impl Input for A {
    type Data = String;
}

static PROCESSED: AtomicUsize = AtomicUsize::new(0);

#[derive(Hash, PartialEq, Eq, Debug, Clone)]
pub struct Intermediate;

#[async_trait]
impl Query for Intermediate {
    type Output = String;
    async fn calc<S: System>(&self, system: &S) -> Self::Output {
        let input = system.query(A).await;
        tokio::time::delay_for(tokio::time::Duration::from_secs(2)).await;
        PROCESSED.fetch_add(1, Ordering::SeqCst);
        format!("{}2", input)
    }
}

#[derive(Hash, PartialEq, Eq, Debug)]
pub struct LongQuery<Q: Query + Clone>(Q, u64);

#[async_trait]
impl<Q> Query for LongQuery<Q>
where
    Q: Query + Clone,
    Q::Output: Clone,
{
    type Output = Q::Output;

    async fn calc<S: System>(&self, system: &S) -> Self::Output {
        let o = system.query(self.0.clone()).await;
        tokio::time::delay_for(tokio::time::Duration::from_secs(self.1)).await;
        o
    }
}

#[derive(Hash, PartialEq, Eq, Debug)]
pub struct Add;

#[async_trait]
impl Query for Add {
    type Output = String;

    async fn calc<S: System>(&self, system: &S) -> Self::Output {
        let handle_a = tokio::spawn({
            let system = system.fork();
            async move { system.query_ref(LongQuery(Intermediate, 3)).await }
        });
        let handle_b = tokio::spawn({
            let system = system.fork();
            async move { system.query_ref(LongQuery(Intermediate, 2)).await }
        });
        let (a, b) = futures::future::try_join(handle_a, handle_b).await.unwrap();
        format!("{} + {}", *a, *b)
    }
}

macro_rules! assert_query {
    ($system: expr, $rev: expr, $expected: expr, $query: expr) => {
        let (out, rev) = $system.query_rev($query).await;
        assert_eq!(
            format!("{:?}", rev),
            $rev,
            "Revision {}",
            stringify!($query)
        );
        assert_eq!(out, $expected, "Query output {}", stringify!($query));
    };
}

#[tokio::test]
async fn diamond() {
    init_log();

    let system = Runtime::default();
    system.set_input(A, "2".into()).await;

    assert_query!(system, "R1", "2", A);

    assert_query!(system, "R1", "22 + 22", Add);
    assert_eq!(PROCESSED.load(Ordering::SeqCst), 1, "Proccess count");
}
