use async_trait::async_trait;
use guacamole::test_common::init_log;
use guacamole::{Input, Query, Runtime, System};

#[derive(Hash, PartialEq, Eq, Debug, Clone)]
struct A;
impl Input for A {
    type Data = String;
}

#[derive(Hash, PartialEq, Eq, Debug, Clone)]
struct B;
impl Input for B {
    type Data = String;
}

#[derive(Hash, PartialEq, Eq, Debug)]
pub struct LongQuery<Q>(Q);

#[async_trait]
impl<Q> Query for LongQuery<Q>
where
    Q: Query + Clone,
    Q::Output: Clone,
{
    type Output = Q::Output;

    async fn calc<S: System>(&self, system: &S) -> Self::Output {
        let o = system.query(self.0.clone()).await;
        tokio::time::delay_for(tokio::time::Duration::from_secs(2)).await;
        o
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generic_type_id() {
        let type_id_a = std::any::TypeId::of::<LongQuery<A>>();
        let type_id_b = std::any::TypeId::of::<LongQuery<B>>();
        assert_ne!(type_id_a, type_id_b);
    }
}

#[derive(Hash, PartialEq, Eq, Debug)]
pub struct Add {
    c: usize,
}
#[async_trait]
impl Query for Add {
    type Output = String;

    async fn calc<S: System>(&self, system: &S) -> Self::Output {
        let handle_a = tokio::spawn({
            let system = system.fork();
            async move { system.query_ref(LongQuery(A)).await }
        });
        let handle_b = tokio::spawn({
            let system = system.fork();
            async move { system.query_ref(LongQuery(B)).await }
        });
        let (a, b) = futures::future::try_join(handle_a, handle_b).await.unwrap();
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

#[tokio::test]
async fn parallel() {
    init_log();

    let system = Runtime::default();
    system.set_input(A, "2".into()).await;
    system.set_input(B, "3".into()).await;

    assert_query!(system, "R1", "2", A);
    assert_query!(system, "R2", "3", B);

    // Calc it once
    assert_query!(system, "R2", "2 + 3 + 4", Add { c: 4 });

    // Reuse memoized output
    assert_query!(system, "R2", "2 + 3 + 4", Add { c: 4 });

    // Different parameters means we have to calculate them again
    assert_query!(system, "R2", "2 + 3 + 1", Add { c: 1 });

    // But then still we should be able to read memoized output.
    assert_query!(system, "R2", "2 + 3 + 4", Add { c: 4 });

    system.set_input(A, "X".into()).await;
    assert_query!(system, "R3", "X", A);

    assert_query!(system, "R3", "X + 3 + 4", Add { c: 4 });
}
