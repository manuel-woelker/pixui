//! A bounded latest-output mailbox. Publication never waits for presentation.

use super::display_list::RenderOutput;
use crossbeam_channel::{Receiver, Sender, TryRecvError, TrySendError, bounded};
use std::sync::{Arc, Mutex, Weak};

type WakeCallback = Arc<dyn Fn() + Send + Sync>;
#[derive(Default)]
struct OutputWake(Mutex<Option<WakeCallback>>);
impl OutputWake {
    fn notify(&self) {
        let callback = self.0.lock().expect("output wake lock").clone();
        if let Some(callback) = callback {
            callback();
        }
    }
}

pub(crate) struct OutputSender {
    effects: Sender<super::text_input::protocol::HostEffect>,
    pub(crate) window_commands: super::window_mailbox::WindowCommandSender,
    sender: Option<Sender<RenderOutput>>,
    drain: Receiver<RenderOutput>,
    subscriber: Weak<()>,
    wake: Arc<OutputWake>,
}

/// Single GUI consumer. Dropping it disconnects the logical subscription.
/// At most one pending complete output is retained per instance.
pub struct OutputReceiver {
    effects: Receiver<super::text_input::protocol::HostEffect>,
    window_commands: super::window_mailbox::WindowCommandReceiver,
    receiver: Receiver<RenderOutput>,
    _subscription: Arc<()>,
    wake: Arc<OutputWake>,
}

pub(crate) fn mailbox() -> (OutputSender, OutputReceiver) {
    let (effects, effect_receiver) = bounded(16);
    let (window_sender, window_receiver) = super::window_mailbox::mailbox();
    let (sender, receiver) = bounded(1);
    let subscription = Arc::new(());
    let wake = Arc::new(OutputWake::default());
    (
        OutputSender {
            effects,
            window_commands: window_sender,
            sender: Some(sender),
            drain: receiver.clone(),
            subscriber: Arc::downgrade(&subscription),
            wake: wake.clone(),
        },
        OutputReceiver {
            effects: effect_receiver,
            window_commands: window_receiver,
            receiver,
            _subscription: subscription,
            wake,
        },
    )
}

impl OutputSender {
    pub(crate) fn effect(
        &self,
        effect: super::text_input::protocol::HostEffect,
    ) -> pixui_base::PixuiResult<()> {
        if !self.connected() {
            return Err(pixui_base::pixui_error!("native host disconnected"));
        }
        self.effects
            .try_send(effect)
            .map_err(|_| pixui_base::pixui_error!("native effect queue is full or disconnected"))?;
        self.wake.notify();
        Ok(())
    }

    pub fn connected(&self) -> bool {
        self.subscriber.strong_count() > 0
    }

    pub fn publish(&self, mut output: RenderOutput) {
        if !self.connected() {
            return;
        }
        loop {
            match self
                .sender
                .as_ref()
                .expect("live output sender")
                .try_send(output)
            {
                Ok(()) => {
                    self.wake.notify();
                    return;
                }
                Err(TrySendError::Disconnected(_)) => return,
                Err(TrySendError::Full(value)) => {
                    output = value;
                    let _ = self.drain.try_recv();
                }
            }
        }
    }
}

impl Drop for OutputSender {
    fn drop(&mut self) {
        // Disconnect before waking so the consumer can observe termination.
        drop(self.sender.take());
        self.wake.notify();
    }
}

impl OutputReceiver {
    /// Ordered bounded native operations. Headless tests can implement these
    /// after declaring clipboard support with UiCommand::HostAttached.
    pub fn effects(&self) -> &Receiver<super::text_input::protocol::HostEffect> {
        &self.effects
    }

    /// Installs a lightweight notification after publication and disconnection.
    /// Called on the publishing thread, outside the callback lock: do not block
    /// or panic. Notifications may be coalesced by the consumer.
    /// Installation wakes for both mailboxes, covering earlier publication.
    /// Only one callback is retained; installing another replaces it.
    pub fn set_waker(&self, callback: impl Fn() + Send + Sync + 'static) {
        let callback: WakeCallback = Arc::new(callback);
        let window_callback = callback.clone();
        self.window_commands.set_waker(move || window_callback());
        *self.wake.0.lock().expect("output wake lock") = Some(callback);
        self.wake.notify();
    }

    /// Separate coalescing metadata mailbox. Installing an output waker also
    /// attaches it here; metadata can arrive without a new rendered frame.
    pub fn window_commands(&self) -> &super::window_mailbox::WindowCommandReceiver {
        &self.window_commands
    }

    /// Blocking convenience for headless consumers with a finite wait deadline.
    pub fn recv_timeout(
        &self,
        timeout: std::time::Duration,
    ) -> Result<RenderOutput, crossbeam_channel::RecvTimeoutError> {
        self.receiver.recv_timeout(timeout)
    }
    /// Returns the pending output without blocking. Disconnection means that the
    /// instance or application worker was removed; the last displayed output may
    /// still be retained by the host.
    pub fn try_recv(&self) -> Result<RenderOutput, TryRecvError> {
        self.receiver.try_recv()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::{display_list::RenderRevision, instance::UiInstanceId};
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn output(revision: u64) -> RenderOutput {
        RenderOutput {
            instance_id: UiInstanceId(1),
            revision: RenderRevision(revision),
            paint_revision: RenderRevision(revision),
            display_list: Default::default(),
            redraw_after: None,
            timings: Default::default(),
            animating: true,
            animation_request: Some(revision),
        }
    }
    #[test]
    fn wake_attachment_publication_replacement_and_disconnect_are_observable() {
        let (sender, receiver) = mailbox();
        sender.publish(output(1));
        let calls = Arc::new(AtomicUsize::new(0));
        let callback_calls = calls.clone();
        receiver.set_waker(move || {
            callback_calls.fetch_add(1, Ordering::SeqCst);
        });
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        sender.publish(output(2));
        sender.publish(output(3));
        assert_eq!(calls.load(Ordering::SeqCst), 4);
        let latest = receiver.try_recv().unwrap();
        assert_eq!(latest.revision, RenderRevision(3));
        assert_eq!(latest.animation_request, Some(3));
        assert!(matches!(receiver.try_recv(), Err(TryRecvError::Empty)));
        drop(sender);
        assert_eq!(calls.load(Ordering::SeqCst), 6);
        assert!(matches!(
            receiver.try_recv(),
            Err(TryRecvError::Disconnected)
        ));
    }
    #[test]
    fn wake_callback_is_reentrant_and_replaces_the_previous_callback() {
        let (sender, receiver) = mailbox();
        let receiver = Arc::new(receiver);
        let weak = Arc::downgrade(&receiver);
        // This would deadlock if callbacks ran while holding the registration lock.
        receiver.set_waker(move || {
            if let Some(receiver) = weak.upgrade() {
                receiver.set_waker(|| {});
            }
        });
        sender.publish(output(1));
        assert_eq!(receiver.try_recv().unwrap().revision, RenderRevision(1));
    }
}
