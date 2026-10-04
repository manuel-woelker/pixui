use pixui_reflect::FieldIndex;

use crate::application::{application_slice::SliceId, collection_key::CollectionKey};

/// An expression evaluated against application state.
pub struct Expression {
    kind: ExpressionKind,
}

impl Expression {
    pub fn new(kind: ExpressionKind) -> Self {
        Self { kind }
    }

    /// References a collection by its slice identity and registration index.
    pub fn collection(slice_id: SliceId, collection_index: usize) -> Self {
        Self::new(ExpressionKind::Collection(CollectionKey::new(
            slice_id,
            collection_index,
        )))
    }

    /// References a previously resolved collection without repeating name lookup.
    pub fn from_collection(key: CollectionKey) -> Self {
        Self::new(ExpressionKind::Collection(key))
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
pub enum ExpressionKind {
    Field(FieldIndex),
    Collection(CollectionKey),
}
