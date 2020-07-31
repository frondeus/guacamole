use std::sync::Arc;

pub struct QueryRef<T>(pub(crate) Arc<T>);

impl<T> std::ops::Deref for QueryRef<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &(*self.0)
    }
}
