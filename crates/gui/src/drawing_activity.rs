//! Wayland fallback: a withheld redraw opportunity pauses worker rendering.
//! This is not an exact visibility query. Compositors normally withhold frame
//! callbacks for invisible surfaces; a stalled compositor can behave the same way.
//! Native pointer/keyboard interaction also resumes drawing without requiring focus.
use std::time::{Duration, Instant};

const PAUSE_DELAY: Duration = Duration::from_millis(500);

#[derive(Default)]
pub(crate) struct DrawingActivity {
    requested: Option<Instant>,
    pub paused: bool,
}
impl DrawingActivity {
    pub fn request(&mut self, now: Instant) {
        // Repeated requests must not postpone detecting a withheld callback.
        self.requested.get_or_insert(now);
    }
    pub fn received(&mut self) -> bool {
        self.requested = None;
        std::mem::take(&mut self.paused)
    }
    pub fn check(&mut self, now: Instant) {
        if self
            .requested
            .is_some_and(|requested| now.duration_since(requested) >= PAUSE_DELAY)
        {
            self.paused = true;
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_withheld_requests_pause_and_one_callback_resumes() {
        let now = Instant::now();
        let mut activity = DrawingActivity::default();
        activity.check(now + Duration::from_secs(10));
        assert!(!activity.paused);
        activity.request(now);
        activity.request(now + Duration::from_millis(400));
        activity.check(now + Duration::from_millis(499));
        assert!(!activity.paused);
        activity.check(now + PAUSE_DELAY);
        assert!(activity.paused);
        assert!(activity.received());
        assert!(!activity.paused);
        assert!(!activity.received());
        activity.check(now + Duration::from_secs(10));
        assert!(!activity.paused);
    }
}
