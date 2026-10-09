//! Native text input metadata and ordered clipboard effects.
use crate::ui::{geometry::Rect, instance::UiInstanceId};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EditingSessionId(pub(crate) u64);

/// Latest desired native editing state. Coalescing this metadata is safe;
/// clipboard requests use a separate ordered queue.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NativeTextInput {
    pub session: Option<EditingSessionId>,
    pub editable: bool,
    pub caret: Rect,
    pub capture_pointer: bool,
}
#[derive(Clone, Debug)]
pub enum HostEffect {
    ReadClipboard { request: u64 },
    WriteClipboard { request: u64, text: String },
}
/// An owned result from the native host, including recoverable clipboard errors.
#[derive(Clone, Debug)]
pub struct ClipboardReply {
    pub instance: UiInstanceId,
    pub request: u64,
    pub result: Result<Option<String>, String>,
}
