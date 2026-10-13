//! Owned callback bindings, prepared independently of component props and painting.
use crate::{
    application::{action::ActionCall, app::Application},
    expression::context::ExpressionContext,
    ui::presentation::PresentationSettings,
};
use pixui_base::PixuiResult;
use pixui_reflect::DynamicObject;
use std::sync::Arc;

/// An ordered input batch can contain a request-building error after earlier
/// valid requests. Dispatch stops there, preserving prior action mutations.
pub type InputActions = Vec<PixuiResult<ActionCall>>;

/// A published callback. It borrows the emission payload only while building an
/// owned request. No payload or application borrow escapes to dispatch.
pub type EventBinding =
    Box<dyn Fn(&Application, &DynamicObject<'_>) -> PixuiResult<ActionCall> + Send>;
#[derive(Clone)]
pub(crate) enum Callback {
    Rust {
        payload: &'static pixui_reflect::TypeDescriptor,
        factory: RustFactory,
    },
    Activation(crate::ui::activation::ActivationFactory),
    Change(crate::ui::text_input::binding::ChangeFactory),
    Program(Arc<crate::events::program::ActionProgram>),
}
#[derive(Clone)]
pub(crate) struct EventCallback {
    pub name: String,
    pub index: Option<usize>,
    pub callback: Callback,
}

/// Event bindings and props belonging to one successful published preparation.
pub(crate) struct PreparedEvents {
    pub descriptors: &'static [super::interface::EventDescriptor],
    pub bindings: Vec<Option<EventBinding>>,
    pub props: Box<dyn std::any::Any + Send>,
}
impl PreparedEvents {
    pub fn index(&self, name: &str) -> Option<usize> {
        self.descriptors.iter().position(|event| event.name == name)
    }

    pub fn has(&self, name: &str) -> bool {
        self.index(name)
            .is_some_and(|index| self.bindings[index].is_some())
    }
    pub fn call(
        &self,
        name: &str,
        app: &Application,
        payload: &DynamicObject<'_>,
    ) -> PixuiResult<Option<ActionCall>> {
        let Some(index) = self.index(name) else {
            return Ok(None);
        };
        if payload.descriptor().type_id() != self.descriptors[index].payload.type_id() {
            return Err(pixui_base::pixui_error!("incorrect event payload"));
        }
        self.bindings[index]
            .as_ref()
            .map(|binding| {
                let mut call = binding(app, payload)?;
                call.event_name = Some(self.descriptors[index].name.into());
                Ok(call)
            })
            .transpose()
    }
}

pub(crate) type PreparedBindings = (Vec<Option<EventBinding>>, Vec<Option<usize>>);

pub(crate) fn prepare(
    callbacks: &[EventCallback],
    descriptors: &'static [super::interface::EventDescriptor],
    context: &ExpressionContext<'_>,
    settings: &PresentationSettings,
) -> PixuiResult<PreparedBindings> {
    let mut bindings = descriptors.iter().map(|_| None).collect::<Vec<_>>();
    let mut identities = vec![None; descriptors.len()];
    for event in callbacks {
        let index = event
            .index
            .or_else(|| {
                descriptors
                    .iter()
                    .position(|descriptor| descriptor.name == event.name)
            })
            .ok_or_else(|| pixui_base::pixui_error!("unknown component event `{}`", event.name))?;
        if descriptors
            .get(index)
            .is_none_or(|descriptor| descriptor.name != event.name)
        {
            return Err(pixui_base::pixui_error!("invalid event binding index"));
        }
        if bindings[index].is_some() {
            return Err(pixui_base::pixui_error!(
                "duplicate event binding `{}`",
                event.name
            ));
        }
        let (binding, identity): (EventBinding, usize) = match &event.callback {
            Callback::Rust { payload, factory } => {
                if payload.type_id() != descriptors[index].payload.type_id() {
                    return Err(pixui_base::pixui_error!(
                        "event key belongs to another interface"
                    ));
                }
                (
                    factory(context, settings)?,
                    Arc::as_ptr(factory) as *const () as usize,
                )
            }
            Callback::Program(program) => {
                (program.prepare(context)?, Arc::as_ptr(program) as usize)
            }
            Callback::Activation(factory) => {
                let binding = factory(context, settings)?;
                (
                    Box::new(move |app, _payload| binding(app)),
                    *factory as usize,
                )
            }
            Callback::Change(factory) => {
                let binding = factory(context, settings)?;
                (
                    Box::new(move |app, payload| {
                        let content = payload
                            .read_object_named("content")?
                            .downcast_ref::<String>()
                            .cloned()
                            .ok_or_else(|| {
                                pixui_base::pixui_error!("change event requires string content")
                            })?;
                        binding(app, content)
                    }),
                    *factory as usize,
                )
            }
        };
        bindings[index] = Some(binding);
        identities[index] = Some(identity);
    }
    Ok((bindings, identities))
}

/// Generated typed event key. Payload identity prevents using another interface's
/// equally named event by accident. Use the constants in `YourEventsBindings`.
pub struct EventKey<P> {
    pub(crate) name: &'static str,
    marker: std::marker::PhantomData<fn() -> P>,
}
impl<P> Copy for EventKey<P> {}
impl<P> Clone for EventKey<P> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<P> EventKey<P> {
    #[doc(hidden)]
    pub const fn new(name: &'static str) -> Self {
        Self {
            name,
            marker: std::marker::PhantomData,
        }
    }
}
/// Rust callback factory for one generated event payload.
pub type TypedBinding<P> = Box<dyn Fn(&Application, &P) -> PixuiResult<ActionCall> + Send>;
pub type TypedFactory<P> =
    for<'a> fn(&ExpressionContext<'a>, &PresentationSettings) -> PixuiResult<TypedBinding<P>>;
pub(crate) type RustFactory = Arc<
    dyn for<'a> Fn(&ExpressionContext<'a>, &PresentationSettings) -> PixuiResult<EventBinding>
        + Send
        + Sync,
>;
pub(crate) fn typed_callback<P: pixui_reflect::Reflect + Send>(
    key: EventKey<P>,
    factory: TypedFactory<P>,
) -> EventCallback {
    EventCallback {
        name: key.name.into(),
        index: None,
        callback: Callback::Rust {
            payload: P::type_descriptor(),
            factory: Arc::new(move |context, settings| {
                let binding = factory(context, settings)?;
                Ok(Box::new(move |app, payload| {
                    let payload = payload
                        .downcast_ref::<P>()
                        .ok_or_else(|| pixui_base::pixui_error!("incorrect Rust event payload"))?;
                    binding(app, payload)
                }))
            }),
        },
    }
}
