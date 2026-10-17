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

/// Dynamic subtrees are resolved on the worker and have per-occurrence state.
#[derive(Clone)]
pub enum LivePart {
    Dynamic(DynamicPart),
    Composite(CompositePart),
    Container(crate::layout::container::ContainerPart),
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
    pub(crate) styles: Vec<crate::styles::slots::SlotAssignment>,
    pub(crate) style_resolver: Option<StyleResolver>,
    pub create_state: StateFactory,
    pub(crate) focus: crate::ui::focus::FocusBehavior,
    pub(crate) focus_resolver: Option<crate::ui::focus::FocusResolver>,
    pub layout: crate::layout::style::LayoutStyle,
    pub(crate) expressions: Vec<Expression>,
    pub(crate) binding: Option<Arc<dyn ErasedBinding>>,
    pub(crate) events: Vec<crate::events::binding::EventCallback>,
}

impl ComponentPart {
    pub fn new(create_state: StateFactory) -> Self {
        Self {
            styles: Vec::new(),
            style_resolver: None,
            focus: Default::default(),
            focus_resolver: None,
            create_state,
            layout: Default::default(),
            expressions: Vec::new(),
            binding: None,
            events: vec![],
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
            styles: Vec::new(),
            style_resolver: None,
            focus: Default::default(),
            focus_resolver: None,
            create_state: |_| Ok(GenericComponentState::new(())),
            layout: Default::default(),
            expressions,
            binding: Some(Arc::new(
                crate::component_registry::binding::ExpressionBinding::<C> {
                    address: id.address,
                    resolve,
                },
            )),
            events: vec![],
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
            styles: Vec::new(),
            style_resolver: None,
            focus: Default::default(),
            focus_resolver: None,
            create_state: |_| Ok(GenericComponentState::new(())),
            layout: Default::default(),
            expressions: Vec::new(),
            binding: Some(Arc::new(Binding::<C> {
                address: id.address,
                resolve,
                update,
                marker: PhantomData,
            })),
            events: vec![],
        }
    }

    /// Replace one painter slot for this declaration. Dynamic assignments follow
    /// these constants; duplicate slots are errors rather than order-dependent.
    pub fn with_style(
        mut self,
        slot: impl Into<String>,
        style: crate::styles::family::StyleAssignment,
    ) -> Self {
        self.styles
            .push(crate::styles::slots::SlotAssignment::new(slot, style));
        self
    }
    /// A pure resolver called once during preparation, never during solver probes.
    pub fn with_style_resolver(mut self, resolve: StyleResolver) -> Self {
        self.style_resolver = Some(resolve);
        self
    }
    /// Resolves an owned activation binding separately from the painter. Bounds
    /// and hit testing remain the renderer's responsibility.
    pub fn with_activation(mut self, activation: ActivationFactory) -> Self {
        self.events.retain(|event| event.name != "activate");
        self.events.push(crate::events::binding::EventCallback {
            name: "activate".into(),
            index: None,
            callback: crate::events::binding::Callback::Activation(activation),
        });
        self
    }

    /// Proposes complete owned content through an application action. Without a
    /// binding a TextInputComponent remains selectable but read-only.
    pub fn with_change(mut self, change: crate::ui::text_input::binding::ChangeFactory) -> Self {
        self.events.retain(|event| event.name != "change");
        self.events.push(crate::events::binding::EventCallback {
            name: "change".into(),
            index: None,
            callback: crate::events::binding::Callback::Change(change),
        });
        self
    }

    /// Adds a worker callback by event name. Registration/preparation validates
    /// the name; generated interface metadata provides payload type information.
    pub fn with_event<P: pixui_reflect::Reflect + Send>(
        mut self,
        key: crate::events::binding::EventKey<P>,
        factory: crate::events::binding::TypedFactory<P>,
    ) -> Self {
        self.events
            .push(crate::events::binding::typed_callback(key, factory));
        self
    }

    /// Constant border-box constraints for this physical leaf.
    pub fn with_layout(mut self, layout: crate::layout::style::LayoutStyle) -> Self {
        self.layout = layout;
        self
    }

    /// Sets focus participation independently of activation.
    pub fn with_focus(mut self, behavior: crate::ui::focus::FocusBehavior) -> Self {
        self.focus = behavior;
        self.focus_resolver = None;
        self
    }

    /// Resolves eligibility from application data each render.
    pub fn with_focus_resolver(mut self, resolve: crate::ui::focus::FocusResolver) -> Self {
        self.focus_resolver = Some(resolve);
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
    /// Optional immutable item key, resolved in the loop element's context.
    /// Collection expressions use full arena keys automatically when omitted.
    /// Other sequences retain positional identity unless this resolver is set.
    pub key: Option<super::identity::ItemKeyResolver>,
    /// Evaluated in the enclosing context; the result must be a sequence.
    pub expression: Expression,
    /// Reused for every element, with that element as the walking context.
    pub body: Box<LivePart>,
}

impl ForLoopPart {
    /// A sequence loop. Collection expressions reconcile by arena identity;
    /// other sequences reconcile by index unless `with_key` is used.
    pub fn new(expression: Expression, body: LivePart) -> Self {
        Self {
            expression,
            body: Box::new(body),
            key: None,
        }
    }

    /// Overrides default loop identity with an immutable item key. Duplicate
    /// keys fail traversal before existing loop payloads are moved.
    pub fn with_key(mut self, key: super::identity::ItemKeyResolver) -> Self {
        self.key = Some(key);
        self
    }
}

impl From<ComponentPart> for LivePart {
    fn from(part: ComponentPart) -> Self {
        Self::Component(part)
    }
}

pub type StyleResolver = for<'a> fn(
    &crate::expression::context::ExpressionContext<'a>,
    &crate::ui::presentation::PresentationSettings,
) -> PixuiResult<Vec<crate::styles::slots::SlotAssignment>>;

/// Reference expression and its source location; no props or children yet.
#[derive(Clone)]
pub struct DynamicPart {
    pub component: Expression,
    pub(crate) domain: Option<String>,
    pub origin: Option<crate::ui_language::source::origin::Origin>,
}

impl DynamicPart {
    pub fn new(component: Expression) -> Self {
        Self {
            component,
            domain: None,
            origin: None,
        }
    }
}
