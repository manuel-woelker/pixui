use std::{
    collections::HashMap,
    sync::atomic::{AtomicU64, Ordering},
};

use pixui_base::{PixuiResult, PixuiString, pixui_error};

use super::action::{ActionDescriptor, ActionIndex};
use super::action_handle::ActionHandle;
use super::collection_index::CollectionIndex;
use super::collection_key::CollectionKey;

/// Process-local slice identity. Never reused, and stable across reordering.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SliceId(u64);

/// A named group of collection bindings, entity bindings and registered actions.
/// Names are unique within each of those independent namespaces.
/// Registration is append-only: bindings cannot be invalidated by replacement,
/// removal, or renaming. Multiple collections may contain the same item type.
pub struct ApplicationSlice {
    name: PixuiString,
    id: SliceId,
    collections: HashMap<String, CollectionIndex>,
    actions: Vec<&'static ActionDescriptor>,
    entities: HashMap<String, super::erased_object_ref::ErasedObjectRef>,
    pending_entities: Vec<(String, super::named_entities::PendingEntity)>,
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
            collections: HashMap::new(),
            actions: vec![],
            entities: HashMap::new(),
            pending_entities: vec![],
        }
    }

    /// Immutable name used for facade binding.
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn id(&self) -> SliceId {
        self.id
    }
    pub fn collections(&self) -> &HashMap<String, CollectionIndex> {
        &self.collections
    }
    pub fn actions(&self) -> &[&'static ActionDescriptor] {
        &self.actions
    }

    /// Stages a reflected value for insertion into application storage on add_slice.
    /// Names are unique within the entity namespace, independent of collection and
    /// action names. Failed names do not retain the supplied value. No ref exists
    /// until registration; obtain one through Application::entity_ref afterward.
    pub fn bind<T: pixui_reflect::Reflect + Send>(
        &mut self,
        name: impl Into<String>,
        value: T,
    ) -> PixuiResult<()> {
        let name = name.into();
        self.validate_entity_name(&name)?;
        self.pending_entities.push((
            name,
            Box::new(move |application| {
                Ok(super::erased_object_ref::ErasedObjectRef::new(
                    application.create_entity(value)?,
                ))
            }),
        ));
        Ok(())
    }

    /// Binds an existing address without taking ownership of its item. Attachment
    /// validates ownership and liveness before inserting any pending values.
    pub fn bind_entity<T: pixui_reflect::Reflect>(
        &mut self,
        name: impl Into<String>,
        reference: super::object_ref::ObjectRef<T>,
    ) -> PixuiResult<()> {
        let name = name.into();
        self.validate_entity_name(&name)?;
        self.entities.insert(
            name,
            super::erased_object_ref::ErasedObjectRef::new(reference),
        );
        Ok(())
    }

    /// Resolved entity address. Pending values cannot be read before attachment.
    pub fn entity(&self, name: &str) -> PixuiResult<&super::erased_object_ref::ErasedObjectRef> {
        self.entities
            .get(name)
            .ok_or_else(|| pixui_error!("unknown or pending entity `{name}`"))
    }

    /// Resolved addresses only; staged values appear after successful attachment.
    pub fn entities(&self) -> &HashMap<String, super::erased_object_ref::ErasedObjectRef> {
        &self.entities
    }

    pub(super) fn validate_entity_name(&self, name: &str) -> PixuiResult<()> {
        super::named_entities::validate_name(name)?;
        if self.entities.contains_key(name)
            || self.pending_entities.iter().any(|(other, _)| other == name)
        {
            return Err(pixui_error!("duplicate entity binding `{name}`"));
        }
        Ok(())
    }

    pub(super) fn resolve_pending(
        &mut self,
        application: &mut super::app::Application,
    ) -> PixuiResult<()> {
        for (name, create) in std::mem::take(&mut self.pending_entities) {
            self.entities.insert(name, create(application)?);
        }
        Ok(())
    }

    /// Binds a local, case-sensitive name to an existing application collection.
    /// Different names or slices may share a collection. Bindings are append-only.
    /// The application validates index ownership when this slice is added.
    pub fn bind_collection(
        &mut self,
        name: impl Into<String>,
        index: CollectionIndex,
    ) -> PixuiResult<()> {
        let name = name.into();
        if name.is_empty() || self.collections.contains_key(&name) {
            return Err(pixui_error!(
                "empty or duplicate collection binding `{name}`"
            ));
        }
        self.collections.insert(name, index);
        Ok(())
    }

    /// Resolves a local name to its stable application-level collection index.
    pub fn collection_index(&self, name: &str) -> PixuiResult<CollectionIndex> {
        self.collections
            .get(name)
            .copied()
            .ok_or_else(|| pixui_error!("unknown collection `{name}`"))
    }

    pub fn collection_key(&self, name: &str) -> PixuiResult<CollectionKey> {
        self.collection_index(name)
    }

    pub(super) fn append_actions(&mut self, actions: &[&'static ActionDescriptor]) {
        self.actions.extend_from_slice(actions);
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
}
