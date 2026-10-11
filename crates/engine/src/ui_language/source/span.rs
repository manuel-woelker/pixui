//! Locations use half-open UTF-8 byte ranges in an immutable source snapshot.
use super::snapshot::SourceId;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub source: SourceId,
    pub start: usize,
    pub end: usize,
}
impl Span {
    pub fn join(self, other: Self) -> Self {
        assert_eq!(self.source, other.source, "cannot join different snapshots");
        Self {
            source: self.source,
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }
}
