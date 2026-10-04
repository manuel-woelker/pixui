use std::{any::Any, collections::HashMap};

use pixui_base::erased_value::SendValues;
use pixui_base::{Arena, Key, PixuiResult, pixui_error};

use super::{
    action::{ActionCall, ActionResult},
    application_handle::ApplicationHandle,
    application_slice::{ApplicationSlice, SliceId},
    collection::Collection,
    collection_key::CollectionKey,
    object_ref::ObjectRef,
};

/// Worker-owned application state. `new` starts the worker and returns its handle.
/// `Default` creates bare state for adapters and tests that need direct access.
#[derive(Default)]
pub struct Application {
    pub(crate) uis: crate::ui::registry::UiRegistry,
    slices: Vec<ApplicationSlice>,
    slice_names: HashMap<String, usize>,
    slice_ids: HashMap<SliceId, usize>,
}

impl Application {
    /// Starts an owner thread immediately, using a bounded queue of 128 commands.
    /// Panics if the worker thread cannot be started.
    #[allow(
        clippy::new_ret_no_self,
        reason = "the application constructor returns access to its internally owned worker"
    )]
    pub fn new() -> ApplicationHandle {
        Self::with_capacity(128)
    }

    /// Starts an owner thread with the supplied bounded command capacity.
    /// Zero selects rendezvous. Panics if thread creation or queue allocation fails.
    pub fn with_capacity(capacity: usize) -> ApplicationHandle {
        ApplicationHandle::start(capacity)
    }

    /// Slices have unique, nonempty, immutable names.
    pub fn slices(&self) -> &[ApplicationSlice] {
        &self.slices
    }

    /// Adds a slice without changing state on an empty or duplicate name.
    pub fn add_slice(&mut self, slice: ApplicationSlice) -> PixuiResult<SliceId> {
        if slice.name().is_empty() || self.slice_names.contains_key(slice.name()) {
            return Err(pixui_error!(
                "empty or duplicate slice name `{}`",
                slice.name()
            ));
        }
        let id = slice.id();
        self.slice_names
            .insert(slice.name().to_owned(), self.slices.len());
        self.slice_ids.insert(id, self.slices.len());
        self.slices.push(slice);
        Ok(id)
    }

    /// Resolves an exact, case-sensitive slice name.
    pub fn slice_named(&self, name: &str) -> PixuiResult<&ApplicationSlice> {
        Ok(&self.slices[self.slice_index(name)?])
    }

    /// Resolves a slice name to its current position. Positions can change when
    /// slices are removed or reordered; retain SliceId or CollectionKey instead.
    pub fn slice_index(&self, name: &str) -> PixuiResult<usize> {
        self.slice_names
            .get(name)
            .copied()
            .ok_or_else(|| pixui_error!("unknown slice `{name}`"))
    }

    /// Resolves exact slice and collection names into a reusable collection address.
    pub fn collection_key(&self, slice: &str, collection: &str) -> PixuiResult<CollectionKey> {
        self.slice_named(slice)?.collection_key(collection)
    }

    /// Borrows an erased collection, checking its slice identity and index.
    pub fn resolve_collection(&self, key: CollectionKey) -> PixuiResult<&Collection> {
        let slice = self.slice(key.slice)?;
        slice
            .collections()
            .get(key.collection_index)
            .ok_or_else(|| {
                pixui_error!(
                    "invalid collection index {} in slice `{}`",
                    key.collection_index,
                    slice.name()
                )
            })
    }

    /// Removes a slice; existing calls and object references then fail at dispatch.
    pub fn remove_slice(&mut self, id: SliceId) -> PixuiResult<ApplicationSlice> {
        let index = self.slice_position(id)?;
        let slice = self.slices.remove(index);
        self.reindex_slices();
        Ok(slice)
    }

    /// Reorders slices without changing identities or names.
    pub fn swap_slices(&mut self, first: SliceId, second: SliceId) -> PixuiResult<()> {
        let first = self.slice_position(first)?;
        let second = self.slice_position(second)?;
        self.slices.swap(first, second);
        self.reindex_slices();
        Ok(())
    }

    fn slice_position(&self, id: SliceId) -> PixuiResult<usize> {
        self.slice_ids
            .get(&id)
            .copied()
            .ok_or_else(|| pixui_error!("unknown slice"))
    }

    // Vector positions change on removal and reordering; keys retain stable IDs.
    fn reindex_slices(&mut self) {
        self.slice_names.clear();
        self.slice_ids.clear();
        for (index, slice) in self.slices.iter().enumerate() {
            self.slice_names.insert(slice.name().to_owned(), index);
            self.slice_ids.insert(slice.id(), index);
        }
    }

    /// Adds a collection while preserving slice identity and naming invariants.
    pub fn add_collection(
        &mut self,
        slice: SliceId,
        collection: super::collection::Collection,
    ) -> PixuiResult<()> {
        self.slice_mut(slice)?.add_collection(collection)
    }

    pub fn slice(&self, id: SliceId) -> PixuiResult<&ApplicationSlice> {
        Ok(&self.slices[self.slice_position(id)?])
    }

    fn slice_mut(&mut self, id: SliceId) -> PixuiResult<&mut ApplicationSlice> {
        let index = self.slice_position(id)?;
        Ok(&mut self.slices[index])
    }

    /// Constructs an owned request from fields in the action's request schema order.
    pub fn action_call(
        &self,
        slice: SliceId,
        name: &str,
        fields: SendValues,
    ) -> PixuiResult<ActionCall> {
        self.slice(slice)?.action_handle_named(name)?.call(fields)
    }

    /// Dispatches to a registered action, resolving borrows only for the call's duration.
    /// A call targeting a removed slice or an invalid action returns an error.
    pub fn dispatch(&mut self, call: ActionCall) -> ActionResult {
        let action = self.slice(call.slice)?.action(call.action)?;
        let result = action.invoke(self, call.slice, call.request);
        self.uis.invalidate_all();
        result
    }

    /// UI definitions and worker-local instance state, available for inspection.
    pub fn uis(&self) -> &crate::ui::registry::UiRegistry {
        &self.uis
    }

    pub(crate) fn ui_command(&mut self, command: crate::ui::input::UiCommand) -> PixuiResult<()> {
        let mut uis = std::mem::take(&mut self.uis);
        let result = uis.command(command, self);
        self.uis = uis;
        if let Some(call) = result? {
            self.dispatch(call)?;
        }
        Ok(())
    }

    pub(crate) fn render_dirty(&mut self) {
        let mut uis = std::mem::take(&mut self.uis);
        uis.render_dirty(self);
        self.uis = uis;
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
