//! Per-window animation handshake and delayed repaint scheduling.
use std::time::{Duration, Instant};

#[derive(Default)]
pub(crate) struct RedrawSchedule {
    deadline: Option<Instant>,
    animating: bool,
    ready: bool,
    next_request: u64,
    outstanding: Option<u64>,
    animation_deadline: Option<Instant>,
    last_request: Option<Instant>,
}
impl RedrawSchedule {
    pub fn received(
        &mut self,
        now: Instant,
        delay: Option<Duration>,
        animating: bool,
        acknowledgement: Option<u64>,
    ) {
        if self.outstanding.is_some() && acknowledgement == self.outstanding {
            self.outstanding = None;
        }
        self.animating = animating;
        self.ready = false;
        self.animation_deadline = None;
        // Continuous animation supersedes a delayed repaint request.
        self.deadline = if animating {
            None
        } else {
            delay.and_then(|delay| {
                now.checked_add(delay.clamp(Duration::from_millis(1), Duration::from_secs(86_400)))
            })
        };
    }
    /// Arm the next native drawing opportunity only after successful presentation.
    /// The interval caps platforms whose redraw events are not vsync-throttled.
    /// It is measured from the previous request so worker time is not added to
    /// every interval; a slow worker schedules one next frame without catching up.
    pub fn presented(&mut self, now: Instant, interval: Duration) -> bool {
        self.ready = self.animating && self.outstanding.is_none();
        self.animation_deadline = self.ready.then(|| {
            self.last_request
                .and_then(|time| time.checked_add(interval))
                .unwrap_or(now)
                .max(now)
        });
        self.ready
    }
    /// Consume a drawing opportunity before enqueueing. Only a completed output
    /// acknowledging this ID releases it; unrelated outputs cannot do so.
    pub fn take_animation_request(&mut self, now: Instant) -> Option<u64> {
        if !self.ready
            || self.outstanding.is_some()
            || self
                .animation_deadline
                .is_some_and(|deadline| deadline > now)
        {
            return None;
        }
        self.next_request = self.next_request.checked_add(1)?;
        self.ready = false;
        self.outstanding = Some(self.next_request);
        self.animation_deadline = None;
        self.last_request = Some(now);
        Some(self.next_request)
    }
    pub fn take_animation_wakeup(&mut self, now: Instant) -> bool {
        if self
            .animation_deadline
            .is_some_and(|deadline| deadline <= now)
        {
            self.animation_deadline = None;
            self.ready
        } else {
            false
        }
    }
    pub fn deadline(&self) -> Option<Instant> {
        [self.deadline, self.animation_deadline]
            .into_iter()
            .flatten()
            .min()
    }
    pub fn take_due(&mut self, now: Instant) -> bool {
        if self.deadline.is_some_and(|deadline| deadline <= now) {
            self.deadline = None;
            true
        } else {
            false
        }
    }
    /// Stops scheduling but retains an in-flight request across suspend/resize.
    pub fn cancel(&mut self) {
        self.deadline = None;
        self.animation_deadline = None;
        self.ready = false;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn refresh_cap_slow_frames_suspension_and_windows_are_independent() {
        let now = Instant::now();
        let interval = Duration::from_millis(10);
        let mut first = RedrawSchedule::default();
        let mut second = RedrawSchedule::default();
        first.received(now, None, true, None);
        second.received(now, None, true, None);
        first.presented(now, interval);
        second.presented(now, interval);
        assert_eq!(first.take_animation_request(now), Some(1));
        assert_eq!(second.take_animation_request(now), Some(1));
        first.received(now + Duration::from_millis(2), None, true, Some(1));
        first.presented(now + Duration::from_millis(3), interval);
        assert_eq!(first.deadline(), Some(now + interval));
        assert_eq!(
            first.take_animation_request(now + Duration::from_millis(9)),
            None
        );
        assert!(!first.take_animation_wakeup(now + Duration::from_millis(9)));
        assert!(first.take_animation_wakeup(now + interval));
        assert!(!first.take_animation_wakeup(now + interval));
        assert_eq!(first.take_animation_request(now + interval), Some(2));
        assert_eq!(second.take_animation_request(now + interval), None);
        let late = now + Duration::from_secs(1);
        first.received(late, None, true, Some(2));
        first.presented(late, interval);
        assert_eq!(first.deadline(), Some(late));
        first.cancel();
        assert_eq!(first.deadline(), None);
        assert_eq!(first.take_animation_request(late), None);
        // Restore uses the retained output and creates exactly one new request.
        first.presented(late, interval);
        assert_eq!(first.take_animation_request(late), Some(3));
        assert_eq!(first.take_animation_request(late), None);
    }
    #[test]
    fn animation_waits_for_presentation_and_its_own_output() {
        let now = Instant::now();
        let mut schedule = RedrawSchedule::default();
        schedule.received(now, Some(Duration::from_millis(33)), true, None);
        assert!(schedule.deadline().is_none());
        assert_eq!(schedule.take_animation_request(now), None);
        assert!(schedule.presented(now, Duration::ZERO));
        assert_eq!(schedule.take_animation_request(now), Some(1));
        assert_eq!(schedule.take_animation_request(now), None);
        schedule.received(now, None, true, None);
        assert!(!schedule.presented(now, Duration::ZERO));
        assert_eq!(schedule.take_animation_request(now), None);
        schedule.cancel();
        schedule.received(now, None, true, Some(1));
        assert!(schedule.presented(now, Duration::ZERO));
        assert_eq!(schedule.take_animation_request(now), Some(2));
        schedule.received(now, None, false, Some(2));
        assert!(!schedule.presented(now, Duration::ZERO));
        assert_eq!(schedule.take_animation_request(now), None);
    }
    #[test]
    fn schedule_coalesces_stops_and_waits_for_new_output() {
        let now = Instant::now();
        let mut schedule = RedrawSchedule::default();
        schedule.received(now, Some(Duration::ZERO), false, None);
        assert!(!schedule.take_due(now));
        assert!(schedule.take_due(now + Duration::from_millis(1)));
        assert!(!schedule.take_due(now + Duration::from_secs(1)));
        schedule.received(now, Some(Duration::from_millis(33)), false, None);
        schedule.received(now, Some(Duration::from_millis(10)), false, None);
        assert_eq!(schedule.deadline(), Some(now + Duration::from_millis(10)));
        schedule.received(now, None, false, None);
        assert_eq!(schedule.deadline(), None);
        schedule.received(now, Some(Duration::from_millis(10)), false, None);
        schedule.cancel();
        assert!(!schedule.take_due(now + Duration::from_secs(1)));
    }
}
