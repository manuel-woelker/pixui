//! Session state shared with the worker; never contains an application sender.
use crate::{resources::path::ResourcePath, ui::image::Image};
use crossbeam_channel::Sender;
use std::{
    collections::{HashMap, HashSet},
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

pub(crate) const MAX_TARGETS: usize = 4096;

pub(crate) enum Hint {
    Event(notify::Event),
    Wake,
}
#[derive(Default)]
pub(crate) struct Images {
    pub requested: HashSet<ResourcePath>,
    pub subscriptions: Vec<ResourcePath>,
    pub snapshots: HashMap<ResourcePath, Image>,
}

pub(crate) struct Shared {
    pub active: AtomicBool,
    pub image_capacity: usize,
    // Serializes stop with accepting a replacement, including catalog installs.
    pub acceptance: Mutex<()>,
    pub images: Mutex<Images>,
    pub hints: Sender<Hint>,
    pub rescan: AtomicBool,
}
impl Shared {
    pub fn new(hints: Sender<Hint>, image_capacity: usize) -> Self {
        Self {
            active: AtomicBool::new(true),
            image_capacity,
            acceptance: Mutex::new(()),
            images: Mutex::new(Images::default()),
            hints,
            rescan: AtomicBool::new(false),
        }
    }

    pub fn active(&self) -> bool {
        self.active.load(Ordering::Acquire)
    }
    pub fn wake(&self) {
        // A full queue already wakes the coordinator; request reconciliation so
        // the dropped hint cannot lose newly subscribed image paths.
        if self.hints.try_send(Hint::Wake).is_err() {
            self.rescan.store(true, Ordering::Release);
        }
    }
    pub fn stop(&self) {
        let _guard = self.acceptance.lock().unwrap();
        self.active.store(false, Ordering::Release);
        self.images.lock().unwrap().snapshots.clear();
        self.wake();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn full_hint_queue_requests_reconciliation_and_stop_releases_pixels() {
        let (sender, _receiver) = crossbeam_channel::bounded(1);
        let shared = Shared::new(sender, 1);
        shared.wake();
        shared.wake();
        assert!(shared.rescan.load(Ordering::Acquire));
        let image = Image::new(1, 1, vec![crate::ui::display_list::Color(1, 2, 3)], None).unwrap();
        let weak = image.downgrade();
        shared
            .images
            .lock()
            .unwrap()
            .snapshots
            .insert(ResourcePath::new("logo.png").unwrap(), image);
        shared.stop();
        assert!(!shared.active());
        assert!(weak.upgrade().is_none());
    }
}
