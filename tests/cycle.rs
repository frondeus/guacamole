use async_trait::async_trait;
use guacamole::test_common::init_log;
use guacamole::{Input, Query, Runtime, System};

#[derive(Hash, PartialEq, Eq, Debug, Clone)]
struct File;
impl Input for File {
    type Data = String;
}

#[derive(Hash, PartialEq, Eq, Debug, Clone)]
pub struct Count;
#[async_trait]
impl Query for Count {
    type Output = ();

    async fn calc<S: System>(&self, system: &S) {
        let _file = system.query_ref(File).await; // I do absolutely nothing with it.

        system.query_ref(Count).await;
    }
}

#[derive(Hash, PartialEq, Eq, Debug)]
pub struct ProcessCounted;
#[async_trait]
impl Query for ProcessCounted {
    type Output = ();

    async fn calc<S: System>(&self, system: &S) {
        system.query(Count).await;
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
async fn cycle() {
    init_log();

    let system = Runtime::default();

    tracing::info!("Set input");
    system.set_input(File, "1".into()).await;
    assert_query!(system, "R1", "1", File);

    tracing::info!("Process once");
    assert_query!(system, "R1", (), ProcessCounted);

    tracing::info!("Load cached");
    assert_query!(system, "R1", (), ProcessCounted);

    tracing::info!("Meaningfull change");
    system.set_input(File, "2".into()).await;
    assert_query!(system, "R2", "2", File);

    tracing::info!("Process second time");
    // Because () == (), we dont have to process Counted again
    assert_query!(system, "R1", (), ProcessCounted);
}
