use std::{
    any::{Any, TypeId},
    collections::HashMap,
    sync::atomic::{AtomicU64, Ordering},
};

use pixui_base::{Arena, PixuiResult, PixuiString, pixui_error};

use super::action::{ActionDescriptor, ActionIndex};
use super::action_handle::ActionHandle;
use super::collection::Collection;
use super::collection_key::CollectionKey;

/// Process-local slice identity. Never reused, and stable across reordering.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SliceId(u64);

/// A named group of collections and registered actions.
/// Collection names and action names are unique within their own namespaces.
/// Registration is append-only: bindings cannot be invalidated by replacement,
/// removal, or renaming. Multiple collections may contain the same item type.
pub struct ApplicationSlice {
    name: PixuiString,
    id: SliceId,
    collections: Vec<Collection>,
    collection_names: HashMap<String, usize>,
    actions: Vec<&'static ActionDescriptor>,
}

impl ApplicationSlice {
    /// Creates an empty slice. Panics if all process-local slice IDs are exhausted.
    pub fn new(name: impl Into<PixuiString>) -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        let id = NEXT_ID
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .expect("slice IDs exhausted");
        Self {
            name: name.into(),
            id: SliceId(id),
            collections: vec![],
            collection_names: HashMap::new(),
            actions: vec![],
        }
    }

    /// Immutable name used for facade binding.
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn id(&self) -> SliceId {
        self.id
    }
    pub fn collections(&self) -> &[Collection] {
        &self.collections
    }
    pub fn actions(&self) -> &[&'static ActionDescriptor] {
        &self.actions
    }

    /// Adds a collection, rejecting empty or duplicate names without changing the slice.
    pub fn add_collection(&mut self, collection: Collection) -> PixuiResult<()> {
        if collection.name().is_empty() || self.collection_names.contains_key(collection.name()) {
            return Err(pixui_error!(
                "empty or duplicate collection name `{}`",
                collection.name()
            ));
        }
        self.collection_names
            .insert(collection.name().to_owned(), self.collections.len());
        self.collections.push(collection);
        Ok(())
    }

    /// Resolves an exact, case-sensitive name to its append-only collection index.
    pub fn collection_index(&self, name: &str) -> PixuiResult<usize> {
        self.collection_names
            .get(name)
            .copied()
            .ok_or_else(|| pixui_error!("unknown collection `{name}`"))
    }

    /// Resolves a collection name to a stable address in this slice.
    pub fn collection_key(&self, name: &str) -> PixuiResult<CollectionKey> {
        Ok(CollectionKey::new(self.id, self.collection_index(name)?))
    }

    pub fn collection(&self, name: &str) -> PixuiResult<&Collection> {
        Ok(&self.collections[self.collection_index(name)?])
    }

    /// Mutates the typed arena, without permitting collection replacement or renaming.
    pub fn collection_mut<T: Any>(&mut self, name: &str) -> PixuiResult<&mut Arena<T>> {
        let index = self.collection_index(name)?;
        self.collections[index].arena_mut::<T>().ok_or_else(|| {
            pixui_error!(
                "collection `{name}` must contain `{}`",
                std::any::type_name::<T>()
            )
        })
    }

    pub(super) fn collection_by_id_mut(&mut self, id: u16) -> PixuiResult<&mut Collection> {
        self.collections
            .iter_mut()
            .find(|collection| collection.id == id)
            .ok_or_else(|| pixui_error!("unknown collection identity"))
    }

    /// Registers only after every injected collection exists with the expected type.
    /// Missing bindings, wrong types, and duplicate action names leave registration unchanged.
    pub fn register_action(
        &mut self,
        action: &'static ActionDescriptor,
    ) -> PixuiResult<ActionIndex> {
        self.validate_action(action)?;
        let index = ActionIndex(self.actions.len());
        self.actions.push(action);
        Ok(index)
    }

    fn validate_action(&self, action: &ActionDescriptor) -> PixuiResult<()> {
        if action.name().is_empty()
            || self
                .actions
                .iter()
                .any(|other| other.name() == action.name())
        {
            return Err(pixui_error!(
                "empty or duplicate action name `{}`",
                action.name()
            ));
        }
        for binding in action.collections() {
            let collection = self.collection(binding.name)?;
            if collection.item_type_id() != binding.item_type_id {
                return Err(pixui_error!(
                    "collection `{}` must contain `{}`, got `{}`",
                    binding.name,
                    binding.item_type_name,
                    collection.item_type_name()
                ));
            }
        }
        Ok(())
    }

    /// Registers a batch atomically, validating all collection bindings and names first.
    pub fn register_actions(&mut self, actions: &[&'static ActionDescriptor]) -> PixuiResult<()> {
        for (index, action) in actions.iter().enumerate() {
            self.validate_action(action)?;
            if actions[..index]
                .iter()
                .any(|other| other.name() == action.name())
            {
                return Err(pixui_error!("duplicate action name `{}`", action.name()));
            }
        }
        self.actions.extend_from_slice(actions);
        Ok(())
    }

    /// Verifies the exact handler descriptor, rather than trusting a matching action name.
    pub fn action_handle_checked(
        &self,
        expected: &'static ActionDescriptor,
    ) -> PixuiResult<ActionHandle> {
        let handle = self.action_handle_named(expected.name())?;
        if !std::ptr::eq(handle.descriptor(), expected) {
            return Err(pixui_error!(
                "action `{}` has a different descriptor",
                expected.name()
            ));
        }
        Ok(handle)
    }

    /// Resolves an exact, case-sensitive action name.
    pub fn action_index(&self, name: &str) -> PixuiResult<ActionIndex> {
        self.actions
            .iter()
            .position(|action| action.name() == name)
            .map(ActionIndex)
            .ok_or_else(|| pixui_error!("unknown action `{name}`"))
    }

    pub fn action(&self, index: ActionIndex) -> PixuiResult<&'static ActionDescriptor> {
        self.actions
            .get(index.0)
            .copied()
            .ok_or_else(|| pixui_error!("invalid action index {}", index.0))
    }

    pub fn action_named(&self, name: &str) -> PixuiResult<&'static ActionDescriptor> {
        self.action(self.action_index(name)?)
    }

    /// Caches an indexed action's metadata without communicating with a worker.
    /// Handles remain valid when more actions are appended or the slice is moved.
    pub fn action_handle(&self, index: ActionIndex) -> PixuiResult<ActionHandle> {
        Ok(ActionHandle::new(self.id, index, self.action(index)?))
    }

    /// Resolves a name and returns a handle for local request construction.
    pub fn action_handle_named(&self, name: &str) -> PixuiResult<ActionHandle> {
        self.action_handle(self.action_index(name)?)
    }

    pub(super) fn check_type<T: Any>(&self, collection: &str) -> PixuiResult<()> {
        if self.collection(collection)?.item_type_id() != TypeId::of::<T>() {
            return Err(pixui_error!(
                "collection `{collection}` has the wrong item type"
            ));
        }
        Ok(())
    }
}
