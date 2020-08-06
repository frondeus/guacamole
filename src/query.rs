use crate::System;
use async_trait::async_trait;
use core::hash::Hash;
use std::fmt;

#[async_trait]
pub trait Query: 'static + Send + Sync + Hash + PartialEq + Eq + fmt::Debug {
    type Output: Send + Sync + fmt::Debug + Eq;

    async fn calc<S: System>(&self, system: &S) -> Self::Output;

    fn on_cycle(&self) -> Self::Output {
        panic!("Cycle detected")
    }
}

/// Input is a special kind of query that you can set up.
/// Input:LData = Query::Output and it implements Default trait
pub trait Input {
    type Data: Send + Sync + Default + fmt::Debug + Eq;
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
