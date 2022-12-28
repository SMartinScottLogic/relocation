use crate::State;

pub struct ExistingSuccessors {
    existing: std::vec::IntoIter<(State, u64)>,
}
impl Iterator for ExistingSuccessors {
    type Item = (State, u64);

    fn next(&mut self) -> Option<Self::Item> {
        self.existing.next()
    }
}

impl From<std::vec::Vec<(State, u64)>> for ExistingSuccessors {
    fn from(existing: std::vec::Vec<(State, u64)>) -> Self {
        Self {
            existing: existing.into_iter(),
        }
    }
}
