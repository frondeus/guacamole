use async_trait::async_trait;
use core::hash::Hash;
use std::fmt;
use crate::System;

#[async_trait]
pub trait Query: 'static + Send + Sync + Hash + PartialEq + Eq + fmt::Debug {
    type Output: Send + Sync + fmt::Debug;

    async fn calc<S: System>(&self, system: &S) -> Self::Output;
}

/// Input is a special kind of query that you can set up.
/// Input:LData = Query::Output and it implements Default trait
pub trait Input {
    type Data: Send + Sync + Default + fmt::Debug;
}

#[async_trait]
impl<I> Query for I
where
    I: Input + Send + Sync + Hash + PartialEq + Eq + 'static + fmt::Debug,
{
    type Output = I::Data;

    async fn calc<S: System>(&self, _system: &S) -> Self::Output {
        I::Data::default()
    }
}
