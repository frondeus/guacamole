use async_trait::async_trait;
use guacamole::test_common::init_log;
use guacamole::{Input, Query, Runtime, System};

#[derive(Hash, PartialEq, Eq, Debug)]
struct Text;
impl Input for Text {
    type Data = String;
}

// Query doesn't store the output, instead you type `Output = ` and its stored in `System`.
// Now Lines have to implement Hash.
#[derive(Hash, PartialEq, Eq, Debug)]
pub struct Lines;

#[async_trait]
impl Query for Lines {
    type Output = Vec<String>;

    async fn calc<S: System>(&self, system: &S) -> Self::Output {
        system
            .query_ref(Text)
            .await
            .lines()
            .map(ToString::to_string)
            .collect()
    }
}

#[derive(Hash, PartialEq, Eq, Debug)]
pub struct RavenCount;

#[async_trait]
impl Query for RavenCount {
    type Output = usize;

    async fn calc<S: System>(&self, system: &S) -> Self::Output {
        system
            .query_ref(Lines)
            .await
            .iter()
            .flat_map(|line| line.char_indices().map(move |x| (line, x)))
            .filter(|(line, (idx, _))| {
                line[*idx..]
                    .chars()
                    .zip("Raven".chars())
                    .all(|(lhs, rhs)| lhs == rhs)
            })
            .count()
    }
}

// But we can use parameters!
#[derive(Hash, PartialEq, Eq, Debug)]
pub struct Add {
    a: usize,
    b: String,
}

#[async_trait]
impl Query for Add {
    type Output = String;

    async fn calc<S: System>(&self, _system: &S) -> Self::Output {
        format!("{} + {}", self.a, self.b)
    }
}

fn main() {
    init_log();

    let text = "Foo\n Raven\n Foo";

    let system = Runtime::default();
    smol::run(async move {
        system.set_input(Text, text.into()).await;
        let raven_count = system.query(RavenCount).await;
        tracing::info!("raven count: {}", raven_count);

        let raven_count = system.query_ref(RavenCount).await;
        tracing::info!("raven count 2: {}", *raven_count);

        // Calc it once
        let added = system
            .query_ref(Add {
                a: 2,
                b: "3".into(),
            })
            .await;
        tracing::info!("Added: {}", *added);

        // Reuse memoized output
        let added = system
            .query_ref(Add {
                a: 2,
                b: "3".into(),
            })
            .await;
        tracing::info!("Added 2: {}", *added);

        // Different parameters means we have to calculate them again
        let added = system
            .query_ref(Add {
                a: 3,
                b: "2".into(),
            })
            .await;
        tracing::info!("Added 3: {}", *added);

        // But then still we should be able to read memoized output.
        let added = system
            .query_ref(Add {
                a: 2,
                b: "3".into(),
            })
            .await;
        tracing::info!("Added 4: {}", *added);
    });
}
