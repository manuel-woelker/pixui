//! Expected input rejection when the worker has advanced past the displayed frame.

use super::display_list::RenderRevision;

/// Input was not delivered because its displayed geometry or bindings expired.
/// Native hosts may discard obsolete input without reporting an application
/// failure. Never retry it against a newer frame: the target may have changed.
/// Zero or future revisions indicate a host protocol error and remain diagnostic.
#[derive(Debug)]
pub struct StaleInputRevision {
    pub presented: RenderRevision,
    pub current: RenderRevision,
}

impl StaleInputRevision {
    /// Whether this is an ordinary race with a previously published frame.
    pub fn is_obsolete(&self) -> bool {
        self.presented.0 != 0 && self.presented <= self.current
    }
}

impl std::fmt::Display for StaleInputRevision {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "stale UI input revision (presented {}, current {})",
            self.presented.0, self.current.0
        )
    }
}

impl std::error::Error for StaleInputRevision {}
