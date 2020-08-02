use std::fmt;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Revision(pub(crate) usize);

impl Revision {
    pub fn inc(self) -> Self {
        Self(self.0 + 1)
    }
}

impl fmt::Debug for Revision {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "R{:?}", &self.0)
    }
}
