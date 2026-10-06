//! Finite event-loop retry queue. Adjacent high-frequency updates are coalesced.

use crossbeam_channel::{TryRecvError, TrySendError};
use pixui_base::{PixuiResult, pixui_error};
use pixui_engine::{
    application::{application_handle::ApplicationHandle, dispatch::ApplicationReply},
    ui::input::UiCommand,
};
use std::collections::VecDeque;

const CAPACITY: usize = 256;

#[derive(Default)]
pub(crate) struct PendingInput {
    commands: VecDeque<UiCommand>,
    replies: Vec<ApplicationReply<()>>,
}

impl PendingInput {
    pub fn push(&mut self, command: UiCommand) -> PixuiResult<()> {
        if matches!(command, UiCommand::Close { .. }) {
            self.commands.retain(|queued| {
                !matches!(
                    queued,
                    UiCommand::Redraw { .. } | UiCommand::AnimationFrame { .. }
                ) || queued.instance() != command.instance()
            });
        }
        if self
            .commands
            .back()
            .is_some_and(|earlier| command.replaces(earlier))
        {
            *self.commands.back_mut().expect("existing command") = command;
        } else {
            if self.commands.len() + self.replies.len() >= CAPACITY {
                return Err(pixui_error!(
                    "GUI event queue exhausted; discrete input was not silently dropped"
                ));
            }
            self.commands.push_back(command);
        }
        Ok(())
    }

    pub fn flush(&mut self, application: &ApplicationHandle) -> PixuiResult<()> {
        let mut pending = Vec::new();
        for reply in self.replies.drain(..) {
            match reply.try_recv() {
                Ok(Ok(())) => {}
                Ok(Err(error)) => eprintln!("UI event rejected: {error:?}"),
                Err(TryRecvError::Empty) => pending.push(reply),
                Err(TryRecvError::Disconnected) => {
                    return Err(pixui_error!("application worker stopped"));
                }
            }
        }
        self.replies = pending;
        while let Some(command) = self.commands.pop_front() {
            match application.try_ui_command(command) {
                Ok(reply) => self.replies.push(reply),
                Err(TrySendError::Full(command)) => {
                    self.commands.push_front(command);
                    break;
                }
                Err(TrySendError::Disconnected(_)) => {
                    return Err(pixui_error!("application worker stopped"));
                }
            }
        }
        Ok(())
    }

    pub fn is_empty(&self) -> bool {
        self.commands.is_empty() && self.replies.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pixui_engine::{
        application::app::Application,
        live_model::part::{ComponentPart, LivePart},
        ui::{
            definition::UiDefinition, display_list::RenderRevision, geometry::Point,
            input::UiInput, presentation::PresentationSettings,
        },
    };

    #[test]
    fn adjacent_updates_coalesce_and_discrete_order_survives_overflow() {
        let app = Application::new();
        let definition = app
            .register_ui(UiDefinition::new(
                "test",
                LivePart::Component(ComponentPart::default()),
            ))
            .unwrap();
        let (id, _receiver) = app
            .create_ui(definition, PresentationSettings::default())
            .unwrap();
        let mut queue = PendingInput::default();
        let motion = |x| UiCommand::Input {
            instance: id,
            revision: RenderRevision(1),
            input: UiInput::PointerMoved(Point { x, y: 0.0 }),
        };
        queue.push(motion(1.0)).unwrap();
        queue.push(motion(2.0)).unwrap();
        assert_eq!(queue.commands.len(), 1);
        assert!(matches!(
            queue.commands.front(),
            Some(UiCommand::Input {
                input: UiInput::PointerMoved(Point { x: 2.0, .. }),
                ..
            })
        ));
        queue
            .push(UiCommand::Input {
                instance: id,
                revision: RenderRevision(1),
                input: UiInput::MouseButton {
                    button: pixui_engine::ui::input::MouseButton::Left,
                    state: pixui_engine::ui::input::ButtonState::Released,
                    position: Point::default(),
                    modifiers: Default::default(),
                },
            })
            .unwrap();
        queue.push(motion(3.0)).unwrap();
        assert_eq!(queue.commands.len(), 3);
        for _ in 3..CAPACITY {
            queue.push(UiCommand::Close { instance: id }).unwrap();
        }
        assert!(queue.push(UiCommand::Close { instance: id }).is_err());
        assert_eq!(queue.commands.len(), CAPACITY);
        assert!(matches!(
            queue.commands.get(1),
            Some(UiCommand::Input {
                input: UiInput::MouseButton { .. },
                ..
            })
        ));
    }
}

#[cfg(test)]
mod animation_tests {
    use super::*;
    use pixui_engine::{
        application::app::Application,
        live_model::part::{ComponentPart, LivePart},
        ui::{definition::UiDefinition, presentation::PresentationSettings},
    };

    #[test]
    fn redraws_retry_full_worker_queue_and_close_discards_unsent_ticks() {
        let app = Application::with_capacity(1);
        let definition = app
            .register_ui(UiDefinition::new(
                "retry",
                LivePart::Component(ComponentPart::default()),
            ))
            .unwrap();
        let (id, _outputs) = app
            .create_ui(definition, PresentationSettings::default())
            .unwrap();
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let worker = app.clone();
        let blocker = std::thread::spawn(move || {
            worker
                .inspect(move |_| {
                    started_tx.send(()).unwrap();
                    release_rx.recv().unwrap();
                    Ok(())
                })
                .unwrap()
        });
        started_rx.recv().unwrap();
        let admitted = app
            .try_ui_command(UiCommand::Redraw { instance: id })
            .unwrap();
        let mut pending = PendingInput::default();
        pending.push(UiCommand::Redraw { instance: id }).unwrap();
        pending.flush(&app).unwrap();
        assert_eq!(pending.commands.len(), 1);
        release_tx.send(()).unwrap();
        blocker.join().unwrap();
        admitted.wait().unwrap();
        pending.flush(&app).unwrap();
        assert!(pending.commands.is_empty());
        app.inspect(|_| Ok(())).unwrap();
        pending.flush(&app).unwrap();
        assert!(pending.is_empty());
        pending.push(UiCommand::Redraw { instance: id }).unwrap();
        pending.push(UiCommand::Redraw { instance: id }).unwrap();
        assert_eq!(pending.commands.len(), 1);
        pending
            .push(UiCommand::AnimationFrame {
                instance: id,
                request: 1,
            })
            .unwrap();
        pending.push(UiCommand::Close { instance: id }).unwrap();
        assert_eq!(pending.commands.len(), 1);
        assert!(matches!(
            pending.commands.front(),
            Some(UiCommand::Close { .. })
        ));
    }
}
