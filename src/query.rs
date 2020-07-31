use core::hash::Hash;
use async_trait::async_trait;
// 0.1.36
use crate::System;

#[async_trait]
pub trait Query: 'static + Send + Sync + Hash + PartialEq + Eq {
    type Output: Send + Sync;

    async fn calc(&self, system: &System) -> Self::Output;
}

/// Input is a special kind of query that you can set up.
/// Input:LData = Query::Output and it implements Default trait
pub trait Input {
    type Data: Send + Sync + Default;
}

#[async_trait]
impl<I> Query for I where
    I: Input + Send + Sync + Hash + PartialEq + Eq + 'static
{
    type Output = I::Data;

    async fn calc(&self, _system: &System) -> Self::Output {
        I::Data::default()
    }
}

