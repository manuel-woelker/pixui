#![doc = include_str!("Actions.md")]

use std::any::{Any, TypeId, type_name};

use pixui_base::erased_value::SendValue;
use pixui_base::{PixuiResult, pixui_error};
use pixui_reflect::{Reflect, TypeDescriptor};

use super::{app::Application, application_slice::SliceId};

pub use pixui_reflect_macros::{action, slice_actions};

/// An injected collection selected by the handler parameter's name.
/// Registration checks both the name and concrete item type.
pub struct CollectionBinding {
    pub name: &'static str,
    pub item_type_id: TypeId,
    pub item_type_name: &'static str,
}

impl CollectionBinding {
    pub fn new<T: Any>(name: &'static str) -> Self {
        Self {
            name,
            item_type_id: TypeId::of::<T>(),
            item_type_name: type_name::<T>(),
        }
    }
}

/// Owned request payload safe to send to the application thread.
pub type ActionRequest = SendValue;
/// Owned action output safe to return to the calling thread.
pub type ActionOutput = SendValue;
/// Dispatch outcome, including handler or validation errors.
pub type ActionResult = PixuiResult<ActionOutput>;

pub type ActionHandler = fn(&mut Application, SliceId, ActionRequest) -> ActionResult;

/// Immutable metadata and a function adapter, with no captured application state.
///
/// `#[action]` generates the request struct, reflection, and adapter. Value
/// parameters become request fields, mutable item references become opaque
/// `ObjectRef<T>` fields, and mutable arenas are injected by parameter name.
/// Initially at most one mutable parameter is supported. Outputs are owned
/// `Any` values. Errors propagate without rollback; panics are not caught.
/// Descriptors are shared from static storage without reference counting.
pub struct ActionDescriptor {
    name: &'static str,
    description: &'static str,
    arguments: &'static TypeDescriptor,
    collections: Vec<CollectionBinding>,
    handler: ActionHandler,
}

impl ActionDescriptor {
    /// Builds a manual adapter. Application borrows must not escape the callback.
    pub fn new<Args: Reflect + Send>(
        name: &'static str,
        description: &'static str,
        collections: Vec<CollectionBinding>,
        handler: ActionHandler,
    ) -> Self {
        Self {
            name,
            description,
            arguments: Args::type_descriptor(),
            collections,
            handler,
        }
    }

    pub fn name(&self) -> &'static str {
        self.name
    }
    pub fn description(&self) -> &'static str {
        self.description
    }
    /// Request schema: injected collections are absent; item borrows are handles.
    pub fn arguments(&self) -> &'static TypeDescriptor {
        self.arguments
    }
    pub fn collections(&self) -> &[CollectionBinding] {
        &self.collections
    }

    pub(super) fn invoke(
        &self,
        application: &mut Application,
        slice: SliceId,
        request: ActionRequest,
    ) -> ActionResult {
        if request.as_ref().type_id() != self.arguments.type_id() {
            return Err(pixui_error!(
                "action `{}` requires owned `{}`",
                self.name,
                self.arguments.type_name()
            ));
        }
        (self.handler)(application, slice, request)
    }
}

/// A registration position valid only for the slice where it was resolved.
/// Registration is append-only, so indices remain stable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActionIndex(pub usize);

/// An owned invocation that can be prepared or queued without retaining borrows.
/// The request payload is constructed using the registered action's descriptor.
/// No serialization, scheduling, or concurrency is implied.
pub struct ActionCall {
    pub slice: SliceId,
    pub action: ActionIndex,
    pub request: ActionRequest,
}
