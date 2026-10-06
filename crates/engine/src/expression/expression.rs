use pixui_reflect::FieldIndex;

use crate::application::collection_key::CollectionKey;

/// An expression evaluated against application state.
#[derive(Clone)]
pub struct Expression {
    kind: ExpressionKind,
}

impl Expression {
    pub fn new(kind: ExpressionKind) -> Self {
        Self { kind }
    }

    /// References stable application storage independently of slice lifetime.
    pub fn collection(index: CollectionKey) -> Self {
        Self::new(ExpressionKind::Collection(index))
    }

    /// References a previously resolved collection without repeating name lookup.
    pub fn from_collection(key: CollectionKey) -> Self {
        Self::new(ExpressionKind::Collection(key))
    }

    /// Borrows a checked reflected item directly, without a singleton loop.
    pub fn entity<T: pixui_reflect::Reflect>(
        reference: crate::application::object_ref::ObjectRef<T>,
    ) -> Self {
        Self::new(ExpressionKind::Entity(
            crate::application::erased_object_ref::ErasedObjectRef::new(reference),
        ))
    }

    /// Reads a reflected field on the current root or innermost loop element.
    /// The index belongs to that value's descriptor, not to an application collection.
    pub fn field(index: FieldIndex) -> Self {
        Self::new(ExpressionKind::Field(index))
    }

    pub fn kind(&self) -> &ExpressionKind {
        &self.kind
    }
}

/// Operations supported by the expression evaluator.
#[derive(Clone)]
pub enum ExpressionKind {
    Field(FieldIndex),
    Collection(CollectionKey),
    Entity(crate::application::erased_object_ref::ErasedObjectRef),
}
