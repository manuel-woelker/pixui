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

    /// Read a derived value. The callback must not hide translatable declarations;
    /// declare i18n in the surrounding expression tree so extraction can find it.
    pub fn computed(
        resolve: for<'a> fn(
            &crate::expression::context::ExpressionContext<'a>,
        ) -> pixui_base::PixuiResult<pixui_reflect::DynamicObject<'a>>,
    ) -> Self {
        Self::new(ExpressionKind::Computed(resolve))
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

    /// Declare a translatable template with named argument expressions.
    #[track_caller]
    pub fn i18n(
        source: impl Into<String>,
        arguments: impl IntoIterator<Item = (impl Into<String>, Expression)>,
    ) -> pixui_base::PixuiResult<Self> {
        Ok(Self::new(ExpressionKind::I18n(
            crate::i18n::expression::I18nExpression::new(source, arguments)?,
        )))
    }

    /// Declare a message without placeholders.
    #[track_caller]
    pub fn text(source: impl Into<String>) -> pixui_base::PixuiResult<Self> {
        Self::i18n(source, std::iter::empty::<(String, Expression)>())
    }

    pub fn with_translation_context(mut self, context: impl Into<String>) -> Self {
        if let ExpressionKind::I18n(message) = &mut self.kind {
            *message = message.clone().with_context(context);
        }
        self
    }

    pub fn with_translator_comment(mut self, comment: impl Into<String>) -> Self {
        if let ExpressionKind::I18n(message) = &mut self.kind {
            *message = message.clone().with_comment(comment);
        }
        self
    }

    pub(crate) fn kind_mut(&mut self) -> &mut ExpressionKind {
        &mut self.kind
    }

    pub fn kind(&self) -> &ExpressionKind {
        &self.kind
    }
}

/// Operations supported by the expression evaluator.
#[derive(Clone)]
pub enum ExpressionKind {
    Computed(
        for<'a> fn(
            &crate::expression::context::ExpressionContext<'a>,
        ) -> pixui_base::PixuiResult<pixui_reflect::DynamicObject<'a>>,
    ),
    I18n(crate::i18n::expression::I18nExpression),
    Field(FieldIndex),
    Collection(CollectionKey),
    Entity(crate::application::erased_object_ref::ErasedObjectRef),
}
