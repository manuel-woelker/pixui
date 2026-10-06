//! Cheaply clonable access to an application's internally owned worker thread.

use std::any::Any;

use crossbeam_channel::{TrySendError, bounded};
use pixui_base::erased_value::SendValues;
use pixui_base::{Key, PixuiResult, pixui_error};

use crate::{
    component_registry::component_id::ComponentId, live_model::component::Component,
    painters::painter::Painter,
};

use super::{
    action::{ActionCall, ActionDescriptor, ActionIndex},
    action_handle::ActionHandle,
    app::Application,
    application_slice::{ApplicationSlice, SliceId},
    collection::Collection,
    collection_index::CollectionIndex,
    collection_key::CollectionKey,
    dispatch::{ApplicationCommand, ApplicationReply, CommandSender, PendingAction},
    object_ref::ObjectRef,
};

/// Nonblocking dispatch preserves the call on full or disconnected queues.
pub type TryDispatchResult = Result<PendingAction, TrySendError<ActionCall>>;

/// One cheaply clonable MPSC sender; application state stays on its owner thread.
///
/// Slice registration and inspection wait for a reply. Dispatch waits for queue
/// capacity, then returns a pending result. Drop every clone to close the queue;
/// accepted commands are drained before the worker exits. No join handle or
/// shared application state is retained here. Never call blocking handle methods
/// from this same application's worker, because it cannot process its own queue.
#[derive(Clone)]
pub struct ApplicationHandle {
    sender: CommandSender,
}

impl ApplicationHandle {
    /// Registers component C on the worker without creating physical state.
    pub fn register_component<C: Component>(
        &self,
        name: impl Into<String>,
    ) -> PixuiResult<ComponentId<C>> {
        let name = name.into();
        self.request(move |application| application.register_component::<C>(name))?
            .wait()
    }

    /// Transfers a sendable painter to the worker. Component C must exist first.
    ///
    /// ```compile_fail
    /// use pixui_engine::{application::app::Application, components::label::LabelComponent, painters::button::ButtonPainter};
    /// let application = Application::new();
    /// // ButtonPainter implements Painter<ButtonComponent>, not Painter<LabelComponent>.
    /// application.register_painter::<LabelComponent>(ButtonPainter).unwrap();
    /// ```
    pub fn register_painter<C: Component>(&self, painter: impl Painter<C>) -> PixuiResult<()> {
        self.request(move |application| application.register_painter::<C>(painter))?
            .wait()
    }

    /// Registers standard component types only; choose their painters separately.
    pub fn register_standard_components(
        &self,
    ) -> PixuiResult<crate::painters::standard::StandardComponents> {
        self.request(|application| {
            crate::painters::standard::register_components(&mut application.components)
        })?
        .wait()
    }

    pub fn register_standard_painters(&self) -> PixuiResult<()> {
        self.request(|application| {
            crate::painters::standard::register_painters(
                &mut application.painters,
                &application.components,
            )
        })?
        .wait()
    }

    /// Registers a reusable live-part definition on the worker.
    pub fn register_ui(
        &self,
        definition: crate::ui::definition::UiDefinition,
    ) -> PixuiResult<crate::ui::definition::UiDefinitionId> {
        self.request(move |application| application.register_ui(definition))?
            .wait()
    }

    /// Creates independent worker-side state and a latest-output subscription.
    pub fn create_ui(
        &self,
        definition: crate::ui::definition::UiDefinitionId,
        settings: crate::ui::presentation::PresentationSettings,
    ) -> PixuiResult<(
        crate::ui::instance::UiInstanceId,
        crate::ui::mailbox::OutputReceiver,
    )> {
        self.request(move |application| application.uis.create(definition, settings))?
            .wait()
    }

    /// Native callbacks enqueue without blocking. Retain and retry a full-queue
    /// command; a disconnected queue cannot accept further input.
    pub fn try_ui_command(
        &self,
        command: crate::ui::input::UiCommand,
    ) -> Result<ApplicationReply<()>, TrySendError<crate::ui::input::UiCommand>> {
        let (reply, receiver) = bounded(1);
        self.sender
            .try_send(ApplicationCommand::Ui { command, reply })
            .map_err(|error| match error {
                TrySendError::Full(ApplicationCommand::Ui { command, .. }) => {
                    TrySendError::Full(command)
                }
                TrySendError::Disconnected(ApplicationCommand::Ui { command, .. }) => {
                    TrySendError::Disconnected(command)
                }
                _ => unreachable!("UI command sent here"),
            })?;
        Ok(ApplicationReply::new(receiver))
    }

    /// Blocking convenience for setup and headless tests; native callbacks use
    /// `try_ui_command` instead.
    pub fn ui_command(&self, command: crate::ui::input::UiCommand) -> PixuiResult<()> {
        self.request(move |application| application.ui_command(command))?
            .wait()
    }

    pub(super) fn start(capacity: usize) -> Self {
        let (sender, receiver) = bounded(capacity);
        std::thread::Builder::new()
            .name("pixui-application".into())
            .spawn(move || super::dispatch::run(receiver))
            .expect("failed to start application worker");
        Self { sender }
    }

    /// Transfers a configured slice to the worker and waits for registration.
    /// The returned identity is stable and can be retained by every caller.
    pub fn add_slice(&self, slice: ApplicationSlice) -> PixuiResult<SliceId> {
        self.request(move |application| application.add_slice(slice))?
            .wait()
    }

    /// Adds a reflected value to an attached slice, backed by per-type storage.
    pub fn bind<T: pixui_reflect::Reflect + Send>(
        &self,
        slice: SliceId,
        name: impl Into<String>,
        value: T,
    ) -> PixuiResult<()> {
        let name = name.into();
        self.request(move |application| application.bind(slice, name, value))?
            .wait()
    }
    pub fn create_entity<T: pixui_reflect::Reflect + Send>(
        &self,
        value: T,
    ) -> PixuiResult<ObjectRef<T>> {
        self.request(move |application| application.create_entity(value))?
            .wait()
    }
    pub fn bind_entity<T: pixui_reflect::Reflect>(
        &self,
        slice: SliceId,
        name: impl Into<String>,
        reference: ObjectRef<T>,
    ) -> PixuiResult<()> {
        let name = name.into();
        self.request(move |application| application.bind_entity(slice, name, reference))?
            .wait()
    }
    pub fn entity_ref<T: Any>(&self, slice: SliceId, name: &str) -> PixuiResult<ObjectRef<T>> {
        let name = name.to_owned();
        self.request(move |application| application.entity_ref(slice, &name))?
            .wait()
    }

    /// Transfers collection storage to the worker and returns a stable index.
    pub fn register_collection(&self, collection: Collection) -> PixuiResult<CollectionIndex> {
        self.request(move |application| application.register_collection(collection))?
            .wait()
    }

    pub fn add_collection(
        &self,
        slice: SliceId,
        collection: Collection,
    ) -> PixuiResult<CollectionIndex> {
        self.request(move |application| application.add_collection(slice, collection))?
            .wait()
    }

    pub fn bind_collection(
        &self,
        slice: SliceId,
        name: impl Into<String>,
        index: CollectionIndex,
    ) -> PixuiResult<()> {
        let name = name.into();
        self.request(move |application| application.bind_collection(slice, name, index))?
            .wait()
    }

    /// Validates the action against this application's collection store.
    pub fn register_action(
        &self,
        slice: SliceId,
        action: &'static ActionDescriptor,
    ) -> PixuiResult<ActionIndex> {
        self.request(move |application| application.register_action(slice, action))?
            .wait()
    }

    /// Registers all actions atomically after validating names, types and aliases.
    pub fn register_actions(
        &self,
        slice: SliceId,
        actions: &[&'static ActionDescriptor],
    ) -> PixuiResult<()> {
        let actions = actions.to_vec();
        self.request(move |application| application.register_actions(slice, &actions))?
            .wait()
    }

    /// Resolves an action through one worker round trip. Cache the returned handle
    /// to construct later calls locally, including from other threads.
    pub fn action(&self, slice: SliceId, name: &str) -> PixuiResult<ActionHandle> {
        let name = name.to_owned();
        self.request(move |application| application.slice(slice)?.action_handle_named(&name))?
            .wait()
    }

    /// Resolves slice and collection names on the worker once. Cache the returned
    /// index for expression construction; removing its slice retains the storage.
    pub fn collection_key(&self, slice: &str, collection: &str) -> PixuiResult<CollectionKey> {
        let slice = slice.to_owned();
        let collection = collection.to_owned();
        self.request(move |application| application.collection_key(&slice, &collection))?
            .wait()
    }

    /// Resolves an action on the worker, then constructs its request locally.
    /// Reuse `action(...)?` and `ActionHandle::call` to avoid repeated lookups.
    pub fn action_call(
        &self,
        slice: SliceId,
        name: &str,
        fields: SendValues,
    ) -> PixuiResult<ActionCall> {
        self.action(slice, name)?.call(fields)
    }

    /// Validates an item address on the worker without retaining any borrow.
    pub fn object_ref<T: Any>(
        &self,
        slice: SliceId,
        collection: &str,
        key: Key<T>,
    ) -> PixuiResult<ObjectRef<T>> {
        let collection = collection.to_owned();
        self.request(move |application| application.object_ref(slice, &collection, key))?
            .wait()
    }

    /// Creates an item address directly from application-level collection storage.
    pub fn object_ref_at<T: Any>(
        &self,
        collection: CollectionIndex,
        key: Key<T>,
    ) -> PixuiResult<ObjectRef<T>> {
        self.request(move |application| application.object_ref_at(collection, key))?
            .wait()
    }

    /// Enqueues an action, blocking when the bounded queue is full.
    /// Failed sends drop the unsent call; `try_dispatch` preserves it for retry.
    pub fn dispatch(&self, call: ActionCall) -> PixuiResult<PendingAction> {
        let (reply, receiver) = bounded(1);
        self.sender
            .send(ApplicationCommand::Dispatch { call, reply })
            .map_err(|_| pixui_error!("application worker disconnected"))?;
        Ok(ApplicationReply::new(receiver))
    }

    /// Enqueues without waiting. Errors retain the call for retry via `into_inner`.
    pub fn try_dispatch(&self, call: ActionCall) -> TryDispatchResult {
        let (reply, receiver) = bounded(1);
        self.sender
            .try_send(ApplicationCommand::Dispatch { call, reply })
            .map_err(|error| match error {
                TrySendError::Full(ApplicationCommand::Dispatch { call, .. }) => {
                    TrySendError::Full(call)
                }
                TrySendError::Disconnected(ApplicationCommand::Dispatch { call, .. }) => {
                    TrySendError::Disconnected(call)
                }
                _ => unreachable!("only dispatch commands are sent here"),
            })?;
        Ok(ApplicationReply::new(receiver))
    }

    /// Inspects state on the worker and returns an owned, sendable snapshot.
    /// Borrowed application data cannot escape. Inspection shares the action queue
    /// and observes commands processed before it, rather than running concurrently.
    pub fn inspect<T: Send + 'static>(
        &self,
        read: impl FnOnce(&Application) -> PixuiResult<T> + Send + 'static,
    ) -> PixuiResult<T> {
        self.request(move |application| read(application))?.wait()
    }

    fn request<T: Send + 'static>(
        &self,
        operation: impl FnOnce(&mut Application) -> PixuiResult<T> + Send + 'static,
    ) -> PixuiResult<ApplicationReply<T>> {
        let (reply, receiver) = bounded(1);
        let command = ApplicationCommand::Task(Box::new(move |application| {
            let _ = reply.send(operation(application));
        }));
        self.sender
            .send(command)
            .map_err(|_| pixui_error!("application worker disconnected"))?;
        Ok(ApplicationReply::new(receiver))
    }
}
