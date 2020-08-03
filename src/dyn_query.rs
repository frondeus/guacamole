use crate::Runtime;
use async_trait::async_trait;
use std::any::Any;

#[async_trait]
pub(crate) trait DynQuery: Send + Sync {
    async fn calc(&self, system: &Runtime) -> Box<dyn Any + Send + Sync>;
}
