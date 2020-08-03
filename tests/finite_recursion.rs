use async_trait::async_trait;
use guacamole::test_common::init_log;
use guacamole::{Input, Query, Runtime, System};
use std::sync::atomic::{AtomicUsize, Ordering};

static PROCESSED: AtomicUsize = AtomicUsize::new(0);
static COUNTED: AtomicUsize = AtomicUsize::new(0);

#[derive(Hash, PartialEq, Eq, Debug, Clone)]
struct File;
impl Input for File {
    type Data = String;
}

#[derive(Hash, PartialEq, Eq, Debug, Clone)]
pub struct Count(usize);
#[async_trait]
impl Query for Count {
    type Output = ();

    async fn calc<S: System>(&self, system: &S) {
        let _file = system.query_ref(File).await; // I do absolutely nothing with it.

        COUNTED.fetch_add(1, Ordering::SeqCst);
        tracing::info!("COUNT: {}", self.0);
        tokio::time::delay_for(tokio::time::Duration::from_secs(1)).await;

        if self.0 > 0 {
            system.query_ref(Count(self.0 - 1)).await;
        }
    }
}

#[derive(Hash, PartialEq, Eq, Debug)]
pub struct ProcessCounted;
#[async_trait]
impl Query for ProcessCounted {
    type Output = ();

    async fn calc<S: System>(&self, system: &S) {
        system.query(Count(1)).await;
        PROCESSED.fetch_add(1, Ordering::SeqCst);
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
async fn finite_recursion() {
    init_log();

    let system = Runtime::default();

    tracing::info!("Set input");
    system.set_input(File, "1".into()).await;
    assert_query!(system, "R1", "1", File);

    tracing::info!("Process once");
    assert_query!(system, "R1", (), ProcessCounted);
    assert_eq!(PROCESSED.load(Ordering::SeqCst), 1, "Processed count");
    assert_eq!(COUNTED.load(Ordering::SeqCst), 2, "Counted count");

    tracing::info!("Load cached");
    assert_query!(system, "R1", (), ProcessCounted);
    assert_eq!(PROCESSED.load(Ordering::SeqCst), 1, "Processed count");
    assert_eq!(COUNTED.load(Ordering::SeqCst), 2, "Counted count");

    tracing::info!("Meaningfull change");
    system.set_input(File, "2".into()).await;
    assert_query!(system, "R2", "2", File);

    tracing::info!("Process second time");
    // Because () == (), we dont have to process Counted again
    assert_query!(system, "R1", (), ProcessCounted);
    assert_eq!(PROCESSED.load(Ordering::SeqCst), 1, "Processed count");
    assert_eq!(COUNTED.load(Ordering::SeqCst), 4, "Counted count");
}
