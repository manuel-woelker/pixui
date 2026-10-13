#![doc = include_str!("README.md")]
use pixui_base::{PixuiResult, pixui_error};
use pixui_reflect::{DynamicObject, Reflect, TypeDescriptor};
use std::cell::RefCell;

/// Immutable metadata for one method of an event interface.
pub struct EventDescriptor {
    pub name: &'static str,
    pub payload: &'static TypeDescriptor,
    pub parameter_types: Vec<&'static TypeDescriptor>,
}
impl EventDescriptor {
    pub fn of<P: Reflect + Send + 'static>(name: &'static str) -> Self {
        Self {
            name,
            payload: P::type_descriptor(),
            parameter_types: vec![],
        }
    }
    /// Generated metadata allows language access to custom reflected parameters.
    pub fn with_parameter_type<T: Reflect>(mut self) -> Self {
        self.parameter_types.push(T::type_descriptor());
        self
    }
}
/// Implemented for `dyn YourEvents` by `#[component_events]`.
/// The proxy is borrowed only for the input turn; it cannot escape into state.
pub trait EventInterface {
    fn descriptors() -> &'static [EventDescriptor];
    fn with_proxy<R>(sink: &EventSink, use_proxy: impl FnOnce(&Self) -> R) -> R;
}
/// Event interface for components without behavior.
pub struct NoEvents;
impl EventInterface for NoEvents {
    fn descriptors() -> &'static [EventDescriptor] {
        &[]
    }
    fn with_proxy<R>(_sink: &EventSink, use_proxy: impl FnOnce(&Self) -> R) -> R {
        use_proxy(&Self)
    }
}
/// Local emission queue. A short interior borrow supports shared proxy methods.
/// Owned payloads are processed in order after the component hook returns.
#[derive(Clone)]
pub struct EventSink {
    descriptors: &'static [EventDescriptor],
    bound: Vec<bool>,
    pending: std::rc::Rc<RefCell<Vec<PendingEvent>>>,
}
pub struct PendingEvent {
    pub index: usize,
    pub payload: DynamicObject<'static>,
}
impl EventSink {
    pub fn new(descriptors: &'static [EventDescriptor], bound: Vec<bool>) -> PixuiResult<Self> {
        if bound.len() != descriptors.len() {
            return Err(pixui_error!("incorrect event binding count"));
        }
        Ok(Self {
            descriptors,
            bound,
            pending: std::rc::Rc::new(RefCell::new(vec![])),
        })
    }
    pub fn emit(&self, index: usize, payload: DynamicObject<'static>) -> PixuiResult<()> {
        let event = self
            .descriptors
            .get(index)
            .ok_or_else(|| pixui_error!("invalid event index"))?;
        if !payload.is_owned() {
            return Err(pixui_error!("event payload must be owned"));
        }
        if payload.descriptor().type_id() != event.payload.type_id() {
            return Err(pixui_error!("incorrect payload for event `{}`", event.name));
        }
        if self.bound[index] {
            self.pending
                .try_borrow_mut()
                .map_err(|_| pixui_error!("event queue is already borrowed"))?
                .push(PendingEvent { index, payload });
        }
        Ok(())
    }
    pub fn into_pending(self) -> Vec<PendingEvent> {
        std::mem::take(&mut *self.pending.borrow_mut())
    }
}
/// Semantic activation or a targeted low-level input from the source window.
/// Pointer coordinates in Raw input are logical window coordinates.
pub enum ComponentInput {
    Activate,
    /// A press and release over the same target, validated by the worker.
    Click {
        position: crate::ui::geometry::Point,
        button: crate::ui::input::MouseButton,
        modifiers: crate::ui::input::Modifiers,
    },
    Raw(crate::ui::input::UiInput),
}

/// Declares which targeted inputs call a component hook. Restricting interests
/// avoids traversing retained state for irrelevant pointer movement/key events.
#[derive(Clone, Copy)]
pub struct InputInterest {
    pub pointer: bool,
    pub keyboard: bool,
    pub activation: bool,
    pub click: bool,
}
impl InputInterest {
    pub const NONE: Self = Self {
        pointer: false,
        keyboard: false,
        activation: false,
        click: false,
    };
    pub const ALL: Self = Self {
        pointer: true,
        keyboard: true,
        activation: true,
        click: true,
    };
    pub const ACTIVATION: Self = Self {
        activation: true,
        ..Self::NONE
    };
    pub const CLICK: Self = Self {
        click: true,
        ..Self::NONE
    };
    pub(crate) fn accepts(self, input: &ComponentInput) -> bool {
        match input {
            ComponentInput::Activate => self.activation,
            ComponentInput::Click { .. } => self.click,
            ComponentInput::Raw(
                crate::ui::input::UiInput::Keyboard(_)
                | crate::ui::input::UiInput::ModifiersChanged(_)
                | crate::ui::input::UiInput::ImeCommit(_)
                | crate::ui::input::UiInput::ImePreedit { .. },
            ) => self.keyboard,
            ComponentInput::Raw(_) => self.pointer,
        }
    }
}
