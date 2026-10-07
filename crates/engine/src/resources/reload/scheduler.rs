//! Per-target debounce and bounded retry policy, independent of native events.
use std::time::{Duration, Instant};

pub(crate) struct Schedule {
    pub revision: std::sync::Arc<std::sync::atomic::AtomicU64>,
    pub due: Option<Instant>,
    failures: u8,
}
impl Schedule {
    pub fn new(now: Instant, quiet: Duration) -> Self {
        Self {
            revision: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(1)),
            due: Some(now + quiet),
            failures: 0,
        }
    }
    pub fn dirty(&mut self, now: Instant, quiet: Duration) {
        self.revision
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        self.due = Some(now + quiet);
        self.failures = 0;
    }
    pub fn failed(&mut self, now: Instant, quiet: Duration) -> bool {
        self.failures += 1;
        self.due = (self.failures < 3).then_some(now + quiet.max(Duration::from_millis(100)));
        self.due.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn debounce_and_retry_are_per_target_and_bounded() {
        let now = Instant::now();
        let quiet = Duration::from_millis(200);
        let mut first = Schedule::new(now, quiet);
        let second = Schedule::new(now, quiet);
        first.dirty(now + quiet, quiet);
        assert_eq!(first.due, Some(now + quiet * 2));
        assert_eq!(second.due, Some(now + quiet));
        assert_eq!(first.revision.load(std::sync::atomic::Ordering::Acquire), 2);
        assert!(!first.failed(now, quiet));
        assert!(!first.failed(now, quiet));
        assert!(first.failed(now, quiet));
        first.dirty(now, quiet);
        assert!(!first.failed(now, quiet));
    }
}
