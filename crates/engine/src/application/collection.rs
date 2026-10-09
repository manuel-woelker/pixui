use std::any::{Any, TypeId, type_name};

use pixui_base::erased_value::SendValue;
use pixui_base::{Arena, PixuiResult, PixuiString, pixui_error};
use pixui_reflect::{DynamicObject, Reflect};

type SequenceView = for<'a> fn(&'a dyn Any) -> PixuiResult<DynamicObject<'a>>;

/// Application-owned homogeneous storage with an erased typed arena.
///
/// `new::<T>` chooses the collection's item type permanently. The collection
/// owns an `Arena<T>` behind `Any`, so collections of different item types can
/// coexist in the application. Values are stored directly, without per-item wrappers
/// or reflection requirements. `T` must be `'static + Send` so the application
/// can move to its owner thread. `Sync` is not required.
///
/// Typed accessors return `None` for the wrong item type. Arena keys remain
/// typed and retain the arena's checks for stale and foreign handles.
pub struct Collection {
    name: PixuiString,
    pub(super) id: u16,
    arena: SendValue,
    item_type_id: TypeId,
    item_type_name: &'static str,
    sequence_view: Option<SequenceView>,
    sequence_keys: Option<fn(&dyn Any) -> Vec<u64>>,
}

impl Collection {
    /// Creates an empty collection of `T`. Uses the arena's normal ID allocation.
    pub fn new<T: Any + Send>(name: impl Into<PixuiString>) -> Self {
        let arena = Arena::<T>::new();
        Self {
            name: name.into(),
            id: arena.arena_id(),
            arena: Box::new(arena),
            item_type_id: TypeId::of::<T>(),
            item_type_name: type_name::<T>(),
            sequence_view: None,
            sequence_keys: None,
        }
    }

    /// Creates a collection with read-only reflected sequence access for expressions.
    /// Items remain stored directly in the arena, without per-item dynamic wrappers.
    pub fn new_reflected<T: Reflect + Send>(name: impl Into<PixuiString>) -> Self {
        let mut collection = Self::new::<T>(name);
        collection.sequence_view = Some(|value| {
            let arena = value
                .downcast_ref::<Arena<T>>()
                .ok_or_else(|| pixui_error!("collection arena has an unexpected type"))?;
            Ok(DynamicObject::from_arena(arena))
        });
        collection.sequence_keys = Some(|value| {
            value
                .downcast_ref::<Arena<T>>()
                .expect("collection arena type")
                .iter()
                .map(|(key, _)| key.bits())
                .collect()
        });
        collection
    }

    /// Keys in exactly the same order as the reflected sequence view.
    pub(crate) fn sequence_keys(&self) -> PixuiResult<Vec<u64>> {
        self.sequence_keys
            .map(|keys| keys(self.arena.as_ref()))
            .ok_or_else(|| pixui_error!("collection has no reflected sequence access"))
    }

    /// Borrows live items as a shared sequence, skipping vacant arena slots.
    /// Returns an error for collections created without reflected access.
    pub fn as_sequence(&self) -> PixuiResult<DynamicObject<'_>> {
        let view = self.sequence_view.ok_or_else(|| {
            pixui_error!(
                "collection `{}` has no reflected sequence access; use Collection::new_reflected",
                self.name
            )
        })?;
        view(self.arena.as_ref())
    }

    /// Diagnostic name and default for `Application::add_collection`. Slice-local
    /// aliases may differ, and different collections may have the same name.
    /// Internally allocated ad hoc collections have an empty name.
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn item_type_id(&self) -> TypeId {
        self.item_type_id
    }

    /// Diagnostic Rust type name, not a persistent identifier.
    pub fn item_type_name(&self) -> &'static str {
        self.item_type_name
    }

    /// Borrows the arena if `T` matches the collection's item type.
    pub fn arena<T: Any>(&self) -> Option<&Arena<T>> {
        self.arena.downcast_ref()
    }

    /// Exclusively borrows the arena if `T` matches the collection's item type.
    pub fn arena_mut<T: Any>(&mut self) -> Option<&mut Arena<T>> {
        self.arena.downcast_mut()
    }
}
