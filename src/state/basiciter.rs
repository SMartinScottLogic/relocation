use crate::{NewState, OverlayState, State};

#[deprecated]
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

pub struct NewExistingSuccessors {
    existing: std::vec::IntoIter<(NewState, u64)>,
}
impl Iterator for NewExistingSuccessors {
    type Item = (NewState, u64);

    fn next(&mut self) -> Option<Self::Item> {
        self.existing.next()
    }
}
impl From<std::vec::Vec<(NewState, u64)>> for NewExistingSuccessors {
    fn from(existing: std::vec::Vec<(NewState, u64)>) -> Self {
        Self {
            existing: existing.into_iter(),
        }
    }
}

pub struct OverlayExistingSuccessors {
    existing: std::vec::IntoIter<(OverlayState, u64)>,
}
impl Iterator for OverlayExistingSuccessors {
    type Item = (OverlayState, u64);

    fn next(&mut self) -> Option<Self::Item> {
        self.existing.next()
    }
}
impl From<std::vec::Vec<(OverlayState, u64)>> for OverlayExistingSuccessors {
    fn from(existing: std::vec::Vec<(OverlayState, u64)>) -> Self {
        Self {
            existing: existing.into_iter(),
        }
    }
}
