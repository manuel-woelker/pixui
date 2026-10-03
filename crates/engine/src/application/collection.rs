use std::any::{Any, TypeId, type_name};

use pixui_base::erased_value::SendValue;
use pixui_base::{Arena, PixuiString};

/// A named, homogeneous collection with an erased typed arena.
///
/// `new::<T>` chooses the collection's item type permanently. The collection
/// owns an `Arena<T>` behind `Any`, so collections of different item types can
/// coexist in a slice. Values are stored directly, without per-item wrappers
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
        }
    }

    /// Immutable binding name. Names must be unique within a slice.
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
