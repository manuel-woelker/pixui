//! Application-wide monotonic timeline; painters receive a sampled integer value.

use std::time::Instant;

pub(crate) struct RenderClock {
    started: Instant,
}

impl Default for RenderClock {
    fn default() -> Self {
        Self {
            started: Instant::now(),
        }
    }
}

impl RenderClock {
    pub fn timestamp_us(&self) -> u64 {
        // Saturate rather than wrapping if the timeline ever outlives u64 micros.
        u64::try_from(self.started.elapsed().as_micros()).unwrap_or(u64::MAX)
    }
}
