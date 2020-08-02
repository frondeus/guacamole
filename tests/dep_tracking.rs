use async_trait::async_trait;
use guacamole::{Input, Query, Runtime, System};

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
        format!("{} + {} + {}", *a, *b, self.c)
    }
}

macro_rules! assert_query {
    ($system: expr, $rev: expr, $expected: expr, $query: expr) => {
        let (out, rev) = $system.query_rev($query).await;
        assert_eq!(rev, $rev, "Revision {}", stringify!($query));
        assert_eq!(out, $expected, "Query output {}", stringify!($query));
    };
}

#[tokio::test]
async fn test() {
    let system = Runtime::default();
    system.set_input(A, "2".into()).await;
    system.set_input(B, "3".into()).await;

    assert_query!(system, 1, "2", A);
    assert_query!(system, 2, "3", B);

    // Calc it once
    assert_query!(system, 2, "2 + 3 + 4", Add { c: 4 });

    // Reuse memoized output
    assert_query!(system, 2, "2 + 3 + 4", Add { c: 4 });

    // Different parameters means we have to calculate them again
    assert_query!(system, 2, "2 + 3 + 1", Add { c: 1 });

    // But then still we should be able to read memoized output.
    assert_query!(system, 2, "2 + 3 + 4", Add { c: 4 });

    system.set_input(A, "X".into()).await;
    assert_query!(system, 3, "X", A);

    assert_query!(system, 3, "X + 3 + 4", Add { c: 4 });
}
