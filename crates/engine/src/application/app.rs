use std::any::Any;

use pixui_base::erased_value::SendValues;
use pixui_base::{Arena, Key, PixuiResult, pixui_error};

use super::{
    action::{ActionCall, ActionResult},
    application_slice::{ApplicationSlice, SliceId},
    object_ref::ObjectRef,
};

/// An application's ordered slices of data.
#[derive(Default)]
pub struct Application {
    pub slices: Vec<ApplicationSlice>,
}

impl Application {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn slice(&self, id: SliceId) -> PixuiResult<&ApplicationSlice> {
        self.slices
            .iter()
            .find(|slice| slice.id() == id)
            .ok_or_else(|| pixui_error!("unknown slice"))
    }

    pub fn slice_mut(&mut self, id: SliceId) -> PixuiResult<&mut ApplicationSlice> {
        self.slices
            .iter_mut()
            .find(|slice| slice.id() == id)
            .ok_or_else(|| pixui_error!("unknown slice"))
    }

    /// Constructs an owned request from fields in the action's request schema order.
    pub fn action_call(
        &self,
        slice: SliceId,
        name: &str,
        fields: SendValues,
    ) -> PixuiResult<ActionCall> {
        let target = self.slice(slice)?;
        let action = target.action_index(name)?;
        let request = target.action(action)?.arguments().construct_send(fields)?;
        Ok(ActionCall {
            slice,
            action,
            request,
        })
    }

    /// Dispatches to a registered action, resolving borrows only for the call's duration.
    /// A call targeting a removed slice or an invalid action returns an error.
    pub fn dispatch(&mut self, call: ActionCall) -> ActionResult {
        let action = self.slice(call.slice)?.action(call.action)?;
        action.invoke(self, call.slice, call.request)
    }

    /// Creates a checked opaque reference without retaining an application borrow.
    pub fn object_ref<T: Any>(
        &self,
        slice: SliceId,
        collection: &str,
        key: Key<T>,
    ) -> PixuiResult<ObjectRef<T>> {
        let target = self.slice(slice)?;
        target.check_type::<T>(collection)?;
        let collection = target.collection(collection)?;
        if !collection.arena::<T>().expect("type checked").contains(key) {
            return Err(pixui_error!("unknown or stale item"));
        }
        Ok(ObjectRef {
            slice,
            collection: collection.id,
            key,
        })
    }

    /// Resolves a handle, rechecking its slice, collection, type, and generation.
    /// Handles may refer to any slice in this application, including another slice
    /// than the action's target. They are addresses, not authorization tokens.
    pub fn resolve_mut<T: Any>(&mut self, reference: ObjectRef<T>) -> PixuiResult<&mut T> {
        self.slice_mut(reference.slice)?
            .collection_by_id_mut(reference.collection)?
            .arena_mut::<T>()
            .ok_or_else(|| pixui_error!("wrong collection item type"))?
            .get_mut(reference.key)
            .ok_or_else(|| pixui_error!("unknown or stale item"))
    }

    /// Resolves a named collection in the action's target slice.
    pub fn collection_mut<T: Any>(
        &mut self,
        slice: SliceId,
        name: &str,
    ) -> PixuiResult<&mut Arena<T>> {
        self.slice_mut(slice)?.collection_mut::<T>(name)
    }
}
