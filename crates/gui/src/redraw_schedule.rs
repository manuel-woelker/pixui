//! At most one animation wakeup per window; a new output rearms the schedule.

use std::time::{Duration, Instant};

#[derive(Default)]
pub(crate) struct RedrawSchedule {
    deadline: Option<Instant>,
}
impl RedrawSchedule {
    pub fn received(&mut self, now: Instant, delay: Option<Duration>) {
        self.deadline = delay.and_then(|delay| {
            now.checked_add(delay.clamp(Duration::from_millis(1), Duration::from_secs(86_400)))
        });
    }
    pub fn deadline(&self) -> Option<Instant> {
        self.deadline
    }
    /// Consume the deadline before enqueueing. No extra ticks accumulate while
    /// admission, rendering, or delivery is pending. Rendering failures stop it.
    pub fn take_due(&mut self, now: Instant) -> bool {
        if self.deadline.is_some_and(|deadline| deadline <= now) {
            self.deadline = None;
            true
        } else {
            false
        }
    }
    pub fn cancel(&mut self) {
        self.deadline = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn schedule_coalesces_stops_and_waits_for_new_output() {
        let now = Instant::now();
        let mut schedule = RedrawSchedule::default();
        schedule.received(now, Some(Duration::ZERO));
        assert!(!schedule.take_due(now));
        assert!(schedule.take_due(now + Duration::from_millis(1)));
        assert!(!schedule.take_due(now + Duration::from_secs(1)));
        schedule.received(now, Some(Duration::from_millis(33)));
        schedule.received(now, Some(Duration::from_millis(10)));
        assert_eq!(schedule.deadline(), Some(now + Duration::from_millis(10)));
        schedule.received(now, None);
        assert_eq!(schedule.deadline(), None);
        schedule.received(now, Some(Duration::from_millis(10)));
        schedule.cancel();
        assert!(!schedule.take_due(now + Duration::from_secs(1)));
    }
}
