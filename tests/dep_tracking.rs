use async_trait::async_trait;
use guacamole::{Input, Query, Runtime, System};
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Hash, PartialEq, Eq, Debug)]
struct A;
impl Input for A {
    type Data = String;
}

#[derive(Hash, PartialEq, Eq, Debug)]
struct B;
impl Input for B {
    type Data = String;
}

static PROCESSED: AtomicUsize = AtomicUsize::new(0);

#[derive(Hash, PartialEq, Eq, Debug)]
pub struct Add {
    c: usize,
}
#[async_trait]
impl Query for Add {
    type Output = String;

    async fn calc<S: System>(&self, system: &S) -> Self::Output {
        let a = system.query_ref(A).await;
        let b = system.query_ref(B).await;
        PROCESSED.fetch_add(1, Ordering::SeqCst);
        format!("{} + {} + {}", *a, *b, self.c)
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
fn dep_tracking() {
    let system = Runtime::default();
    smol::run(async move {
        system.set_input(A, "2".into()).await;
        system.set_input(B, "3".into()).await;

        assert_query!(system, "R1", "2", A);
        assert_query!(system, "R2", "3", B);

        // Calc it once
        assert_query!(system, "R2", "2 + 3 + 4", Add { c: 4 });
        assert_eq!(PROCESSED.load(Ordering::SeqCst), 1, "Processed count");

        // Reuse memoized output
        assert_query!(system, "R2", "2 + 3 + 4", Add { c: 4 });
        assert_eq!(PROCESSED.load(Ordering::SeqCst), 1, "Processed count");

        // Different parameters means we have to calculate them again
        assert_query!(system, "R2", "2 + 3 + 1", Add { c: 1 });
        assert_eq!(PROCESSED.load(Ordering::SeqCst), 2, "Processed count");

        // But then still we should be able to read memoized output.
        assert_query!(system, "R2", "2 + 3 + 4", Add { c: 4 });
        assert_eq!(PROCESSED.load(Ordering::SeqCst), 2, "Processed count");

        system.set_input(A, "X".into()).await;
        assert_query!(system, "R3", "X", A);

        assert_query!(system, "R3", "X + 3 + 4", Add { c: 4 });
        assert_eq!(PROCESSED.load(Ordering::SeqCst), 3, "Processed count");

        assert_query!(system, "R3", "X + 3 + 4", Add { c: 4 });
        assert_eq!(PROCESSED.load(Ordering::SeqCst), 3, "Processed count");
    });
}
