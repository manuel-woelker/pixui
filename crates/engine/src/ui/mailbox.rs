//! A bounded latest-output mailbox. Publication never waits for presentation.

use super::display_list::RenderOutput;
use crossbeam_channel::{Receiver, Sender, TryRecvError, TrySendError, bounded};
use std::sync::{Arc, Weak};

pub(crate) struct OutputSender {
    sender: Sender<RenderOutput>,
    drain: Receiver<RenderOutput>,
    subscriber: Weak<()>,
}

/// Single GUI consumer. Dropping it disconnects the logical subscription.
/// At most one pending complete output is retained per instance.
pub struct OutputReceiver {
    receiver: Receiver<RenderOutput>,
    _subscription: Arc<()>,
}

pub(crate) fn mailbox() -> (OutputSender, OutputReceiver) {
    let (sender, receiver) = bounded(1);
    let subscription = Arc::new(());
    (
        OutputSender {
            sender,
            drain: receiver.clone(),
            subscriber: Arc::downgrade(&subscription),
        },
        OutputReceiver {
            receiver,
            _subscription: subscription,
        },
    )
}

impl OutputSender {
    pub fn connected(&self) -> bool {
        self.subscriber.strong_count() > 0
    }

    pub fn publish(&self, mut output: RenderOutput) {
        if !self.connected() {
            return;
        }
        loop {
            match self.sender.try_send(output) {
                Ok(()) | Err(TrySendError::Disconnected(_)) => return,
                Err(TrySendError::Full(value)) => {
                    output = value;
                    let _ = self.drain.try_recv();
                }
            }
        }
    }
}

impl OutputReceiver {
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
