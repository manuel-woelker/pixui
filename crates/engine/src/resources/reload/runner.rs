//! One bounded preparation/delivery pipeline for all resource kinds.
use super::{
    builder::ResourceReloadBuilder,
    scheduler::Schedule,
    shared::{Hint, MAX_TARGETS, Shared},
    target::{Prepared, Source, Target},
};
use crate::{application::dispatch::ApplicationReply, resources::image_loader::ImageLoader};
use crossbeam_channel::{Receiver, Sender, TryRecvError, TrySendError};
use pixui_base::{PixuiResult, pixui_error};
use std::{
    sync::{Arc, atomic::Ordering},
    time::{Duration, Instant},
};

struct Delivery {
    target: usize,
    hash: blake3::Hash,
    update: Box<Prepared>,
}
struct Flight {
    target: usize,
    hash: blake3::Hash,
    reply: ApplicationReply<bool>,
    version: u64,
}
pub(crate) struct Coordinator {
    builder: ResourceReloadBuilder,
    shared: Arc<Shared>,
    loader: Option<ImageLoader>,
    hints: Receiver<Hint>,
    ready: Option<Sender<PixuiResult<()>>>,
    initial_error: Option<String>,
    targets: Vec<Target>,
    pending: Option<Delivery>,
    flight: Option<Flight>,
}
impl Coordinator {
    pub fn new(
        builder: ResourceReloadBuilder,
        shared: Arc<Shared>,
        loader: Option<ImageLoader>,
        hints: Receiver<Hint>,
        ready: Sender<PixuiResult<()>>,
    ) -> Self {
        let targets = builder
            .catalogs
            .iter()
            .map(|catalog| Target {
                source: Source::Catalog(catalog.clone()),
                path: catalog.path.clone(),
                schedule: Schedule::new(Instant::now(), builder.quiet),
                accepted: None,
                initial: true,
                last_error: None,
            })
            .collect();
        let mut coordinator = Self {
            builder,
            shared,
            loader,
            hints,
            ready: Some(ready),
            initial_error: None,
            targets,
            pending: None,
            flight: None,
        };
        coordinator.subscribe_images(true);
        coordinator
    }
    pub fn run(mut self) {
        while self.shared.active() {
            self.drain();
            if !self.shared.active() {
                break;
            }
            self.deliver();
            if !self.shared.active() {
                break;
            }
            if self.pending.is_none() && self.flight.is_none() {
                let now = Instant::now();
                let next = self
                    .targets
                    .iter()
                    .enumerate()
                    .filter_map(|(i, t)| t.schedule.due.map(|due| (i, due)))
                    .min_by_key(|(_, due)| *due);
                if let Some((index, _due)) = next.filter(|(_, due)| *due <= now) {
                    self.prepare(index);
                }
            }
            if self.ready.is_some() && self.targets.iter().all(|target| !target.initial) {
                let result = self.initial_error.take().map_or(Ok(()), |e| {
                    Err(pixui_error!("initial resource loading: {e}"))
                });
                let _ = self.ready.take().unwrap().send(result);
            }
            let wait = if self.pending.is_some() || self.flight.is_some() {
                Duration::from_millis(10)
            } else {
                self.targets
                    .iter()
                    .filter_map(|t| t.schedule.due)
                    .min()
                    .map_or(Duration::from_secs(3600), |due| {
                        due.saturating_duration_since(Instant::now())
                    })
            };
            if let Ok(hint) = self.hints.recv_timeout(wait) {
                self.hint(hint);
            }
        }
    }
    fn subscribe_images(&mut self, initial: bool) {
        let Some(loader) = &self.loader else { return };
        let paths = std::mem::take(&mut self.shared.images.lock().unwrap().subscriptions);
        for path in paths {
            // Image subscriptions reserve catalog capacity during attachment.
            assert!(self.targets.len() < MAX_TARGETS);
            self.targets.push(Target {
                source: Source::Image(loader.clone()),
                path,
                schedule: Schedule::new(Instant::now(), self.builder.quiet),
                accepted: None,
                initial,
                last_error: None,
            });
        }
    }
    fn hint(&mut self, hint: Hint) {
        self.subscribe_images(false);
        let Hint::Event(event) = hint else { return };
        let all = event.need_rescan() || event.paths.is_empty();
        let _guard = self.shared.acceptance.lock().unwrap();
        for target in &mut self.targets {
            if all || target.affected(&event.paths) {
                target.schedule.dirty(Instant::now(), self.builder.quiet);
            }
        }
    }
    fn drain(&mut self) {
        self.subscribe_images(false);
        for _ in 0..256 {
            let Ok(hint) = self.hints.try_recv() else {
                break;
            };
            self.hint(hint);
        }
        if self.shared.rescan.swap(false, Ordering::AcqRel) {
            let _guard = self.shared.acceptance.lock().unwrap();
            for target in &mut self.targets {
                target.schedule.dirty(Instant::now(), self.builder.quiet);
            }
        }
    }
    fn prepare(&mut self, index: usize) {
        self.targets[index].schedule.due = None;
        let target = &self.targets[index];
        let version = target.schedule.revision.load(Ordering::Acquire);
        let before = target.stamp();
        let result = target.read().and_then(|bytes| {
            let hash = blake3::hash(&bytes);
            if target.accepted == Some(hash) {
                return Ok((hash, None));
            }
            Ok((hash, Some(target.source.decode(&bytes)?)))
        });
        // File events during read/decode invalidate the result before delivery.
        self.drain();
        let target = &mut self.targets[index];
        if target.stamp() != before && target.schedule.revision.load(Ordering::Acquire) == version {
            let _guard = self.shared.acceptance.lock().unwrap();
            target.schedule.dirty(Instant::now(), self.builder.quiet);
        }
        if target.schedule.revision.load(Ordering::Acquire) != version {
            return;
        }
        match result {
            Ok((hash, Some(payload))) => {
                self.pending = Some(Delivery {
                    target: index,
                    hash,
                    update: Box::new(Prepared {
                        shared: self.shared.clone(),
                        revision: target.schedule.revision.clone(),
                        version,
                        source: target.source.clone(),
                        path: target.path.clone(),
                        payload,
                    }),
                });
            }
            Ok((_, None)) => {
                target.initial = false;
                target.last_error = None;
            }
            Err(error) => self.failed(index, format!("{error:?}")),
        }
    }
    fn deliver(&mut self) {
        if let Some(flight) = self.flight.take() {
            match flight.reply.try_recv() {
                Ok(Ok(installed)) => {
                    let target = &mut self.targets[flight.target];
                    if installed {
                        target.accepted = Some(flight.hash);
                        target.last_error = None;
                    }
                    if installed || target.schedule.due.is_none() {
                        target.initial = false;
                    }
                }
                Ok(Err(error)) => {
                    if self.targets[flight.target]
                        .schedule
                        .revision
                        .load(Ordering::Acquire)
                        == flight.version
                    {
                        self.failed(flight.target, format!("{error:?}"));
                    }
                }
                Err(TryRecvError::Empty) => {
                    self.flight = Some(flight);
                }
                Err(TryRecvError::Disconnected) => {
                    self.shared.stop();
                    return;
                }
            }
        }
        if let Some(delivery) = self.pending.take() {
            if delivery.update.revision.load(Ordering::Acquire) != delivery.update.version {
                return;
            }
            let version = delivery.update.version;
            match self
                .builder
                .application
                .try_resource_update(delivery.update)
            {
                Ok(reply) => {
                    self.flight = Some(Flight {
                        target: delivery.target,
                        hash: delivery.hash,
                        reply,
                        version,
                    })
                }
                Err(TrySendError::Full(update)) => {
                    self.pending = Some(Delivery { update, ..delivery });
                }
                Err(TrySendError::Disconnected(_)) => self.shared.stop(),
            }
        }
    }
    fn failed(&mut self, index: usize, message: String) {
        let target = &mut self.targets[index];
        if target.last_error.as_ref() != Some(&message) {
            tracing::warn!(
                path = target.path.as_str(),
                error = message,
                "resource reload failed; keeping last good value"
            );
            target.last_error = Some(message.clone());
        }
        if target.schedule.failed(Instant::now(), self.builder.quiet) && target.initial {
            target.initial = false;
            self.initial_error.get_or_insert(message);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        application::app::Application,
        i18n::po::PoFormat,
        resources::{
            filesystem::{ResourceFilesystem, ResourceReader},
            path::ResourcePath,
        },
    };
    struct Source;
    impl ResourceFilesystem for Source {
        fn open(&self, _: &ResourcePath) -> PixuiResult<Option<ResourceReader>> {
            Ok(None)
        }
        fn watch_roots(&self) -> Vec<std::path::PathBuf> {
            vec![std::path::PathBuf::from("/assets")]
        }
    }
    #[test]
    fn directory_hints_are_scoped_and_overflow_reconciles_all_targets() {
        let app = Application::new();
        let german = app.register_language("de").unwrap();
        let builder = ResourceReloadBuilder::new(app)
            .catalog(
                Arc::new(Source),
                ResourcePath::new("one/de.po").unwrap(),
                "one",
                german,
                Arc::new(PoFormat),
            )
            .unwrap()
            .catalog(
                Arc::new(Source),
                ResourcePath::new("two/de.po").unwrap(),
                "two",
                german,
                Arc::new(PoFormat),
            )
            .unwrap();
        let (sender, receiver) = crossbeam_channel::bounded(1);
        let shared = Arc::new(Shared::new(sender, 0));
        let (ready, _) = crossbeam_channel::bounded(1);
        let mut coordinator = Coordinator::new(builder, shared.clone(), None, receiver, ready);
        coordinator.hint(Hint::Event(
            notify::Event::new(notify::EventKind::Any).add_path("/assets/one".into()),
        ));
        assert_eq!(
            coordinator.targets[0]
                .schedule
                .revision
                .load(Ordering::Acquire),
            2
        );
        assert_eq!(
            coordinator.targets[1]
                .schedule
                .revision
                .load(Ordering::Acquire),
            1
        );
        shared.rescan.store(true, Ordering::Release);
        coordinator.drain();
        assert_eq!(
            coordinator.targets[0]
                .schedule
                .revision
                .load(Ordering::Acquire),
            3
        );
        assert_eq!(
            coordinator.targets[1]
                .schedule
                .revision
                .load(Ordering::Acquire),
            2
        );
    }
}
