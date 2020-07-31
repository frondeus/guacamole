use core::hash::Hash;
use async_trait::async_trait;
// 0.1.36
use crate::System;

#[async_trait]
pub trait Query: 'static + Send + Sync + Hash + PartialEq + Eq {
    type Output: Send + Sync;

    async fn calc(&self, system: &System) -> Self::Output;
}
