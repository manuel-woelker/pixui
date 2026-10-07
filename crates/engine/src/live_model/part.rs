use pixui_base::PixuiResult;
use pixui_reflect::DynamicObject;
use std::{marker::PhantomData, sync::Arc};

use super::{component::Component, state::GenericComponentState};
use crate::{
    component_registry::{
        binding::{Binding, ComponentUpdate, ErasedBinding, PropsResolver},
        component_id::{ComponentAddress, ComponentId},
    },
    expression::expression::Expression,
    ui::activation::ActivationFactory,
};

/// Legacy low-level factory for non-GUI tree walks. Registered components use
/// their associated State::default instead, checked against the application.
pub type StateFactory = for<'a> fn(&DynamicObject<'a>) -> PixuiResult<GenericComponentState>;

#[derive(Clone)]
pub enum LivePart {
    Composite(CompositePart),
    Component(ComponentPart),
    ForLoop(ForLoopPart),
    Match(super::match_part::MatchPart),
}

#[derive(Clone)]
pub struct CompositePart {
    pub parts: Vec<LivePart>,
}

#[derive(Clone)]
pub struct ComponentPart {
    pub create_state: StateFactory,
    pub(crate) expressions: Vec<Expression>,
    pub(crate) binding: Option<Arc<dyn ErasedBinding>>,
    pub(crate) activation: Option<ActivationFactory>,
}

impl ComponentPart {
    pub fn new(create_state: StateFactory) -> Self {
        Self {
            create_state,
            expressions: Vec::new(),
            binding: None,
            activation: None,
        }
    }

    /// Binds a registered component to a typed props resolver. Default state is
    /// retained per physical node; changing component identity reinitializes it.
    ///
    /// ```compile_fail
    /// use pixui_engine::{components::button::{ButtonComponent, ButtonProps},
    ///     component_registry::registry::ComponentRegistry, live_model::part::ComponentPart};
    /// let mut registry = ComponentRegistry::default();
    /// let button = registry.register::<ButtonComponent>("button").unwrap();
    /// // A button's resolver must return ButtonProps, not String.
    /// ComponentPart::typed(button, |_, _| Ok(String::new()));
    /// ```
    pub fn typed<C: Component>(id: ComponentId<C>, resolve: PropsResolver<C>) -> Self {
        Self::typed_with_update(id, resolve, |_, _| Ok(()))
    }

    /// Declare expressions in the template so registration can collect messages.
    /// Values are evaluated once per physical component, in declaration order.
    /// Registration resolves private copies; reusing this part is safe.
    pub fn typed_with_expressions<C: Component>(
        id: ComponentId<C>,
        expressions: Vec<Expression>,
        resolve: crate::component_registry::binding::ExpressionPropsResolver<C>,
    ) -> Self {
        Self {
            create_state: |_| Ok(GenericComponentState::new(())),
            expressions,
            binding: Some(Arc::new(
                crate::component_registry::binding::ExpressionBinding::<C> {
                    address: id.address,
                    resolve,
                },
            )),
            activation: None,
        }
    }

    /// An update runs once after resolving props and before painting. It may
    /// change local state but cannot borrow application data or invoke actions.
    /// Errors stop rendering without rolling back previous state updates.
    pub fn typed_with_update<C: Component>(
        id: ComponentId<C>,
        resolve: PropsResolver<C>,
        update: ComponentUpdate<C>,
    ) -> Self {
        Self {
            create_state: |_| Ok(GenericComponentState::new(())),
            expressions: Vec::new(),
            binding: Some(Arc::new(Binding::<C> {
                address: id.address,
                resolve,
                update,
                marker: PhantomData,
            })),
            activation: None,
        }
    }

    /// Resolves an owned activation binding separately from the painter. Bounds
    /// and hit testing remain the renderer's responsibility.
    pub fn with_activation(mut self, activation: ActivationFactory) -> Self {
        self.activation = Some(activation);
        self
    }

    pub(crate) fn component_address(&self) -> Option<ComponentAddress> {
        self.binding.as_ref().map(|binding| binding.address())
    }
}

impl Default for ComponentPart {
    fn default() -> Self {
        Self::new(|_| Ok(GenericComponentState::new(())))
    }
}

#[derive(Clone)]
pub struct ForLoopPart {
    /// Evaluated in the enclosing context; the result must be a sequence.
    pub expression: Expression,
    /// Reused for every element, with that element as the walking context.
    pub body: Box<LivePart>,
}
