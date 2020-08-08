use async_trait::async_trait;
use guacamole::test_common::init_log;
use guacamole::{Input, Query, Runtime, System};
use smol::Task;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::panic::AssertUnwindSafe;
use futures::FutureExt;

static PROCESSED: AtomicUsize = AtomicUsize::new(0);

#[derive(Hash, PartialEq, Eq, Debug, Clone)]
struct A;
impl Input for A {
    type Data = String;
}

#[derive(Hash, PartialEq, Eq, Debug)]
pub struct LongQuery<Q: Query>(Q);

#[async_trait]
impl<Q> Query for LongQuery<Q>
where
    Q: Query + Clone,
    Q::Output: Clone,
{
    type Output = Q::Output;

    async fn calc<S: System>(&self, system: &S) -> Self::Output {
        let o = system.query(self.0.clone()).await;
        tracing::info!("Started long query");
        tokio::time::delay_for(tokio::time::Duration::from_millis(200)).await;
        tracing::warn!("Calculated long query");
        PROCESSED.fetch_add(1, Ordering::SeqCst);
        o
    }
}

#[derive(Hash, PartialEq, Eq, Debug)]
pub struct Add;
#[async_trait]
impl Query for Add {
    type Output = String;

    async fn calc<S: System>(&self, system: &S) -> Self::Output {
        system
            .fork(|system| Task::spawn(async move { system.query(LongQuery(A)).await }))
            .await
            .await
            .unwrap()
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

#[test]
fn cancel_smol() {
    init_log();

    let system = Runtime::default();
    smol::run(async move {
        system.set_input(A, "2".into()).await;

        assert_query!(system, "R1", "2", A);

        let handle_once =
            system
            .fork(|system| Task::spawn(
                {
                    AssertUnwindSafe(
                        async move { system.query(Add).await }
                    )
                        .catch_unwind()
                }
            ))
            .await;

        tokio::time::delay_for(tokio::time::Duration::from_millis(30)).await;

        system.set_input(A, "3".into()).await;

        let out = handle_once.await;

        tracing::info!("OUTPUT {:?}", out);
        assert!(out.is_err());

        tokio::time::delay_for(tokio::time::Duration::from_millis(200)).await;
        assert_eq!(PROCESSED.load(Ordering::SeqCst), 0, "Processed count");

        let handle_twice =
            system
                .fork(|system| Task::spawn(async move { system.query(Add).await }))
                .await
                .await;
        assert!(handle_twice.is_ok());
        let out = handle_twice.unwrap();
        assert_eq!("3", out);
    });
}
