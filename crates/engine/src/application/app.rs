use std::{any::Any, collections::HashMap};

use pixui_base::erased_value::SendValues;
use pixui_base::{Arena, Key, PixuiResult, pixui_error};

use crate::{
    component_registry::{component_id::ComponentId, registry::ComponentRegistry},
    live_model::component::Component,
    painters::{painter::Painter, registry::PainterRegistry},
};

use super::{
    action::{ActionCall, ActionDescriptor, ActionIndex, ActionResult},
    application_handle::ApplicationHandle,
    application_slice::{ApplicationSlice, SliceId},
    collection::Collection,
    collection_index::CollectionIndex,
    collection_key::CollectionKey,
    object_ref::ObjectRef,
};

/// Worker-owned application state. `new` starts the worker and returns its handle.
/// `Default` creates bare state for adapters and tests that need direct access.
#[derive(Default)]
pub struct Application {
    pub(crate) components: ComponentRegistry,
    pub(crate) painters: PainterRegistry,
    pub(crate) uis: crate::ui::registry::UiRegistry,
    // Finalization mutates only this cache while renderers borrow application
    // data immutably. RefCell is worker-local; no shared application-state lock.
    pub(crate) text_service: std::cell::RefCell<crate::ui::text::service::TextService>,
    pub(crate) render_clock: crate::ui::render_clock::RenderClock,
    collections: Vec<Collection>,
    slices: Vec<ApplicationSlice>,
    slice_names: HashMap<String, usize>,
    slice_ids: HashMap<SliceId, usize>,
}

impl Application {
    /// Application-local component identities and type metadata.
    pub fn components(&self) -> &ComponentRegistry {
        &self.components
    }

    pub fn painters(&self) -> &PainterRegistry {
        &self.painters
    }

    pub fn register_component<C: Component>(
        &mut self,
        name: impl Into<String>,
    ) -> PixuiResult<ComponentId<C>> {
        self.components.register(name)
    }

    /// Registers appearance independently of component identity and behavior.
    pub fn register_painter<C: Component>(&mut self, painter: impl Painter<C>) -> PixuiResult<()> {
        self.painters.register::<C>(&self.components, painter)
    }

    /// Checks all component registrations and painters before accepting a template.
    pub fn register_ui(
        &mut self,
        definition: crate::ui::definition::UiDefinition,
    ) -> PixuiResult<crate::ui::definition::UiDefinitionId> {
        self.components
            .validate(&definition.template, &self.painters)?;
        self.uis.register(definition)
    }

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
        for index in slice.collections().values() {
            self.resolve_collection(*index)?;
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

    /// Registers storage without assigning it to a slice. Names are diagnostic
    /// defaults, not globally unique identifiers. Collections live until shutdown.
    pub fn register_collection(&mut self, collection: Collection) -> PixuiResult<CollectionIndex> {
        if collection.name().is_empty() {
            return Err(pixui_error!("empty collection name"));
        }
        let index = CollectionIndex {
            position: self.collections.len(),
            identity: collection.id,
        };
        self.collections.push(collection);
        Ok(index)
    }

    pub fn collections(&self) -> &[Collection] {
        &self.collections
    }

    /// Direct lookup with identity validation; foreign indices never retarget.
    pub fn resolve_collection(&self, index: CollectionIndex) -> PixuiResult<&Collection> {
        self.collections
            .get(index.position)
            .filter(|collection| collection.id == index.identity)
            .ok_or_else(|| pixui_error!("unknown or foreign collection index"))
    }

    pub fn resolve_collection_mut<T: Any>(
        &mut self,
        index: CollectionIndex,
    ) -> PixuiResult<&mut Arena<T>> {
        self.collections
            .get_mut(index.position)
            .filter(|collection| collection.id == index.identity)
            .ok_or_else(|| pixui_error!("unknown or foreign collection index"))?
            .arena_mut::<T>()
            .ok_or_else(|| pixui_error!("wrong collection item type"))
    }

    /// Resolves a collection using a slice's local name.
    pub fn collection(&self, slice: SliceId, name: &str) -> PixuiResult<&Collection> {
        self.resolve_collection(self.slice(slice)?.collection_index(name)?)
    }

    /// Removes a slice's bindings and actions. Collections and item references
    /// remain valid; calls targeting the removed slice fail at dispatch.
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

    /// Registers a new collection and binds its default name atomically.
    pub fn add_collection(
        &mut self,
        slice: SliceId,
        collection: Collection,
    ) -> PixuiResult<CollectionIndex> {
        let name = collection.name().to_owned();
        if name.is_empty() || self.slice(slice)?.collections().contains_key(&name) {
            return Err(pixui_error!(
                "empty or duplicate collection binding `{name}`"
            ));
        }
        let index = self.register_collection(collection)?;
        self.slice_mut(slice)?.bind_collection(name, index)?;
        Ok(index)
    }

    /// Adds an alias to existing storage. Existing bindings cannot be replaced.
    pub fn bind_collection(
        &mut self,
        slice: SliceId,
        name: impl Into<String>,
        index: CollectionIndex,
    ) -> PixuiResult<()> {
        self.resolve_collection(index)?;
        self.slice_mut(slice)?.bind_collection(name, index)
    }

    pub fn register_action(
        &mut self,
        slice: SliceId,
        action: &'static ActionDescriptor,
    ) -> PixuiResult<ActionIndex> {
        let index = ActionIndex(self.slice(slice)?.actions().len());
        self.register_actions(slice, &[action])?;
        Ok(index)
    }

    /// Validates the entire batch before adding any action. Injected collection
    /// names must resolve to existing storage of the correct type. Multiple
    /// mutable parameters may not bind aliases of the same collection.
    pub fn register_actions(
        &mut self,
        slice: SliceId,
        actions: &[&'static ActionDescriptor],
    ) -> PixuiResult<()> {
        let target = self.slice(slice)?;
        for (position, action) in actions.iter().enumerate() {
            if action.name().is_empty()
                || target
                    .actions()
                    .iter()
                    .chain(actions[..position].iter())
                    .any(|other| other.name() == action.name())
            {
                return Err(pixui_error!(
                    "empty or duplicate action name `{}`",
                    action.name()
                ));
            }
            let mut borrowed = std::collections::HashSet::new();
            for binding in action.collections() {
                let index = target.collection_index(binding.name)?;
                let collection = self.resolve_collection(index)?;
                if collection.item_type_id() != binding.item_type_id {
                    return Err(pixui_error!(
                        "collection `{}` must contain `{}`, got `{}`",
                        binding.name,
                        binding.item_type_name,
                        collection.item_type_name()
                    ));
                }
                if !borrowed.insert(index) {
                    return Err(pixui_error!(
                        "action `{}` binds the same mutable collection more than once",
                        action.name()
                    ));
                }
            }
        }
        self.slice_mut(slice)?.append_actions(actions);
        Ok(())
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

    pub(super) fn next_ui_refresh(&self) -> Option<std::time::Instant> {
        self.uis.next_refresh()
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
        let index = self.slice(slice)?.collection_index(collection)?;
        self.object_ref_at(index, key)
    }

    /// Creates an item address directly from its collection index.
    pub fn object_ref_at<T: Any>(
        &self,
        collection: CollectionIndex,
        key: Key<T>,
    ) -> PixuiResult<ObjectRef<T>> {
        let arena = self
            .resolve_collection(collection)?
            .arena::<T>()
            .ok_or_else(|| pixui_error!("wrong collection item type"))?;
        if !arena.contains(key) {
            return Err(pixui_error!("unknown or stale item"));
        }
        Ok(ObjectRef { collection, key })
    }

    /// Resolves an item independently of slice lifetime. Addresses are checked
    /// for application/collection identity, concrete type and arena generation.
    pub fn resolve_mut<T: Any>(&mut self, reference: ObjectRef<T>) -> PixuiResult<&mut T> {
        self.resolve_collection_mut::<T>(reference.collection)?
            .get_mut(reference.key)
            .ok_or_else(|| pixui_error!("unknown or stale item"))
    }

    /// Resolves a named collection in the action's target slice.
    pub fn collection_mut<T: Any>(
        &mut self,
        slice: SliceId,
        name: &str,
    ) -> PixuiResult<&mut Arena<T>> {
        let index = self.slice(slice)?.collection_index(name)?;
        self.resolve_collection_mut(index)
    }
}
