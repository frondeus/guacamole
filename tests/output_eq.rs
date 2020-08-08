use async_trait::async_trait;
use guacamole::test_common::init_log;
use guacamole::{Input, Query, Runtime, System};
use itertools::Itertools;
use std::sync::atomic::{AtomicUsize, Ordering};

static PROCESSED: AtomicUsize = AtomicUsize::new(0);
static PARSED: AtomicUsize = AtomicUsize::new(0);

#[derive(Hash, PartialEq, Eq, Debug, Clone)]
struct File;
impl Input for File {
    type Data = String;
}

#[derive(Hash, PartialEq, Eq, Debug, Clone)]
pub struct Parse;
#[async_trait]
impl Query for Parse {
    type Output = String;

    async fn calc<S: System>(&self, system: &S) -> Self::Output {
        PARSED.fetch_add(1, Ordering::SeqCst);

        system
            .query_ref(File)
            .await
            .chars()
            .filter(|c| !c.is_whitespace())
            .join("")
    }
}

#[derive(Hash, PartialEq, Eq, Debug)]
pub struct Indirect<Q>(Q);

#[async_trait]
impl<Q> Query for Indirect<Q>
where
    Q: Query + Clone,
    Q::Output: Clone,
{
    type Output = Q::Output;

    async fn calc<S: System>(&self, system: &S) -> Self::Output {
        system.query(self.0.clone()).await
    }
}

#[derive(Hash, PartialEq, Eq, Debug)]
pub struct ProcessParsed;
#[async_trait]
impl Query for ProcessParsed {
    type Output = String;

    async fn calc<S: System>(&self, system: &S) -> Self::Output {
        let out = system.query(Indirect(Parse)).await;
        PROCESSED.fetch_add(1, Ordering::SeqCst);
        out
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
fn output_eq() {
    init_log();

    let system = Runtime::default();

    smol::run(async move {
        tracing::info!("Set input");
        system.set_input(File, "2+3".into()).await;
        assert_query!(system, "R1", "2+3", File);

        tracing::info!("Process once");
        assert_query!(system, "R1", "2+3", ProcessParsed);
        assert_eq!(PROCESSED.load(Ordering::SeqCst), 1, "Processed count");
        assert_eq!(PARSED.load(Ordering::SeqCst), 1, "Parsed count");

        tracing::info!("Meaningless change");
        system.set_input(File, "2 + 3".into()).await;
        assert_query!(system, "R2", "2 + 3", File);

        tracing::info!("Load cached");
        assert_query!(system, "R1", "2+3", ProcessParsed);
        assert_eq!(PROCESSED.load(Ordering::SeqCst), 1, "Processed count");
        assert_eq!(PARSED.load(Ordering::SeqCst), 2, "Parsed count");

        tracing::info!("Load cached 2");
        assert_query!(system, "R2", "2+3", ProcessParsed);
        assert_eq!(PROCESSED.load(Ordering::SeqCst), 1, "Processed count");
        assert_eq!(PARSED.load(Ordering::SeqCst), 2, "Parsed count");

        tracing::info!("Meaningfull change");
        system.set_input(File, "2 + 4".into()).await;
        assert_query!(system, "R3", "2 + 4", File);

        tracing::info!("Process second time");
        assert_query!(system, "R3", "2+4", ProcessParsed);
        assert_eq!(PROCESSED.load(Ordering::SeqCst), 2, "Processed count");
        assert_eq!(PARSED.load(Ordering::SeqCst), 3, "Parsed count");
    });
}
