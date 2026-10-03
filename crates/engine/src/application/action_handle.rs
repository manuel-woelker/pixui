//! Cached action metadata for constructing requests on the calling thread.

use pixui_base::{PixuiResult, erased_value::SendValues};

use super::{
    action::{ActionCall, ActionDescriptor, ActionIndex},
    application_slice::SliceId,
};

/// A copyable action address and its static request descriptor.
///
/// Obtain it from a configured slice before adding the slice to the application,
/// or resolve it once through `ApplicationHandle::action`. `call` constructs
/// requests locally, without channels, locks, or access to application state.
/// Append-only registration preserves its index. It does not keep the worker
/// alive or guarantee the target still exists; dispatch validates that separately.
#[derive(Clone, Copy)]
pub struct ActionHandle {
    slice: SliceId,
    index: ActionIndex,
    descriptor: &'static ActionDescriptor,
}

impl ActionHandle {
    pub(super) fn new(
        slice: SliceId,
        index: ActionIndex,
        descriptor: &'static ActionDescriptor,
    ) -> Self {
        Self {
            slice,
            index,
            descriptor,
        }
    }

    pub fn slice_id(&self) -> SliceId {
        self.slice
    }
    pub fn index(&self) -> ActionIndex {
        self.index
    }
    pub fn descriptor(&self) -> &'static ActionDescriptor {
        self.descriptor
    }

    /// Consumes owned sendable fields in request order and creates a call locally.
    /// Wrong counts and types return constructor errors before anything is queued.
    pub fn call(&self, fields: SendValues) -> PixuiResult<ActionCall> {
        Ok(ActionCall {
            slice: self.slice,
            action: self.index,
            request: self.descriptor.arguments().construct_send(fields)?,
        })
    }
}
