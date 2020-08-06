mod revision;

mod dyn_query;
mod invalidation;
mod query;
mod query_ref;
mod runtime;
mod system;

pub(crate) use dyn_query::DynQuery;
pub(crate) use invalidation::Invalidation;
pub(crate) use runtime::DepIdx;
pub(crate) use system::ForkId;

pub use query::{Input, Query};
pub use query_ref::QueryRef;
pub use revision::Revision;
pub use runtime::Runtime;
pub use system::System;

pub mod test_common {
    #[cfg(not(any(test, feature = "with_tests")))]
    pub fn init_log() {
        eprintln!("No logging!");
    }

    #[cfg(any(test, feature = "with_tests"))]
    pub fn init_log() {
        use tracing::Level;

        let subscriber = tracing_subscriber::fmt()
            .with_max_level(Level::DEBUG)
            .finish();
        tracing::subscriber::set_global_default(subscriber)
            .expect("no global subscriber has been set");
    }
}
