//! Explicit host exports. No source name implies a slice, Rust type, or callback.
pub use crate::events::value::{ReferenceExport, Snapshot, ValueType};
use crate::ui_language::lowering::binding::LanguageBinding;
use crate::{
    application::{
        action_handle::ActionHandle, app::Application, collection_key::CollectionKey,
        object_ref::ObjectRef,
    },
    component_registry::component_id::ComponentId,
    expression::expression::Expression,
    live_model::component::Component,
};
use pixui_base::{PixuiResult, pixui_error};
use pixui_reflect::{Reflect, TypeDescriptor};
use std::{any::TypeId, collections::HashMap, sync::Arc};

#[derive(Clone)]
pub struct ComponentExport {
    pub props: &'static TypeDescriptor,
    pub events: &'static [crate::events::interface::EventDescriptor],
    pub(crate) build: Arc<
        dyn Fn(Vec<ValueType>) -> Arc<dyn crate::component_registry::binding::ErasedBinding>
            + Send
            + Sync,
    >,
}
#[derive(Clone)]
pub struct DataExport {
    pub name: String,
    pub expression: Expression,
    pub descriptor: &'static TypeDescriptor,
}
/// Environments are application-local. Duplicate exports are rejected atomically.
#[derive(Clone)]
pub struct Environment {
    pub(crate) components: HashMap<String, ComponentExport>,
    pub(crate) collections: HashMap<String, DataExport>,
    pub(crate) entities: HashMap<String, DataExport>,
    pub(crate) actions: HashMap<(String, String), ActionHandle>,
    pub(crate) slices: HashMap<String, crate::application::application_slice::SliceId>,
    pub(crate) types: HashMap<TypeId, ValueType>,
    pub(crate) descriptors: HashMap<TypeId, &'static TypeDescriptor>,
    pub(crate) layouts: HashMap<String, crate::layout::style::LayoutStyle>,
    pub(crate) styles: HashMap<String, Vec<crate::styles::slots::SlotAssignment>>,
    pub(crate) references: HashMap<TypeId, ReferenceExport>,
}
impl Default for Environment {
    fn default() -> Self {
        Self::new()
    }
}
impl Environment {
    pub fn new() -> Self {
        let mut env = Self {
            components: HashMap::new(),
            collections: HashMap::new(),
            entities: HashMap::new(),
            actions: HashMap::new(),
            slices: HashMap::new(),
            types: HashMap::new(),
            descriptors: HashMap::new(),
            references: HashMap::new(),
            layouts: HashMap::new(),
            styles: HashMap::new(),
        };
        macro_rules! types {($($ty:ty),*)=>{$(env.value_type::<$ty>();)*};}
        types!(
            String, bool, i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize, f32, f64
        );
        env
    }
    /// Adds clone/ownership adapters; nested field types must be exported too.
    pub fn value_type<T: Reflect + Clone + Send + Sync>(&mut self) {
        self.reflected_type::<T>();
        self.types.insert(TypeId::of::<T>(), ValueType::of::<T>());
    }
    /// Exports metadata for field chains and sequences without requiring cloning
    /// or Sync. Use value_type as well when the value must become owned props.
    pub fn reflected_type<T: Reflect>(&mut self) {
        self.descriptors
            .insert(TypeId::of::<T>(), T::type_descriptor());
    }
    /// Props need reflection and a sendable constructor; component state does not.
    pub fn component<C: Component>(&mut self, name: &str, id: ComponentId<C>) -> PixuiResult<()>
    where
        C::Props: Reflect,
    {
        if matches!(name, "Row" | "Column" | "Grid") {
            return Err(pixui_error!(
                "component alias is reserved for a layout container"
            ));
        }
        let props = C::Props::type_descriptor();
        if props
            .fields()
            .iter()
            .any(|field| matches!(field.name, "layout" | "styles" | "focus"))
        {
            return Err(pixui_error!(
                "component props collide with reserved language attributes"
            ));
        }
        if !props.is_send_constructible() {
            return Err(pixui_error!(
                "component props need a sendable reflected constructor"
            ));
        }
        let address = id.address;
        insert(
            &mut self.components,
            name,
            ComponentExport {
                props,
                events: <C::Events as crate::events::interface::EventInterface>::descriptors(),
                build: Arc::new(move |types| {
                    Arc::new(LanguageBinding::<C> {
                        address,
                        types,
                        marker: std::marker::PhantomData,
                    })
                }),
            },
        )?;
        for event in <C::Events as crate::events::interface::EventInterface>::descriptors() {
            for descriptor in &event.parameter_types {
                self.descriptors.insert(descriptor.type_id(), descriptor);
            }
        }
        Ok(())
    }
    /// Checks collection ownership/type and reflection immediately.
    pub fn collection<T: Reflect + Send>(
        &mut self,
        name: &str,
        app: &Application,
        key: CollectionKey,
    ) -> PixuiResult<()> {
        let collection = app.resolve_collection(key)?;
        if collection.item_type_id() != TypeId::of::<T>() {
            return Err(pixui_error!("collection export has wrong item type"));
        }
        collection.as_sequence()?;
        insert(
            &mut self.collections,
            name,
            DataExport {
                name: format!("collections.{name}"),
                expression: Expression::collection(key),
                descriptor: Vec::<T>::type_descriptor(),
            },
        )?;
        self.reflected_type::<T>();
        self.reflected_type::<Vec<T>>();
        self.reference::<T>();
        Ok(())
    }
    pub fn entity<T: Reflect>(
        &mut self,
        name: &str,
        app: &Application,
        reference: ObjectRef<T>,
    ) -> PixuiResult<()> {
        app.resolve(reference)?;
        insert(
            &mut self.entities,
            name,
            DataExport {
                name: format!("entities.{name}"),
                expression: Expression::entity(reference),
                descriptor: T::type_descriptor(),
            },
        )?;
        self.reflected_type::<T>();
        self.reflected_type::<Vec<T>>();
        self.reference::<T>();
        Ok(())
    }
    /// Exposes an action under its application slice name. Checks ownership before
    /// inserting; exporting one action grants no access to its unexported peers.
    pub fn action(
        &mut self,
        app: &Application,
        slice: &str,
        name: &str,
        handle: ActionHandle,
    ) -> PixuiResult<()> {
        let owner = app.slice_named(slice)?;
        self.action_alias(slice, owner.id(), name, handle)
    }
    /// Explicit identifier alias for a slice whose application name is unsuitable
    /// for dotted language calls. Registration identities remain checked.
    pub fn action_alias(
        &mut self,
        slice: &str,
        id: crate::application::application_slice::SliceId,
        name: &str,
        handle: ActionHandle,
    ) -> PixuiResult<()> {
        validate_identifier(slice)?;
        validate_identifier(name)?;
        if handle.slice_id() != id || self.slices.get(slice).is_some_and(|old| *old != id) {
            return Err(pixui_error!("action export has incorrect slice ownership"));
        }
        let key = (slice.to_owned(), name.to_owned());
        if self.actions.contains_key(&key) {
            return Err(pixui_error!("duplicate action export"));
        }
        self.slices.insert(slice.to_owned(), id);
        self.actions.insert(key, handle);
        Ok(())
    }
    pub fn layout(
        &mut self,
        name: &str,
        layout: crate::layout::style::LayoutStyle,
    ) -> PixuiResult<()> {
        layout.to_taffy()?;
        insert(&mut self.layouts, name, layout)
    }
    pub fn styles(
        &mut self,
        name: &str,
        styles: Vec<crate::styles::slots::SlotAssignment>,
    ) -> PixuiResult<()> {
        insert(&mut self.styles, name, styles)
    }
    fn reference<T: Reflect>(&mut self) {
        let value = ValueType::of::<ObjectRef<T>>();
        self.types
            .insert(TypeId::of::<ObjectRef<T>>(), value.clone());
        self.references.insert(
            TypeId::of::<ObjectRef<T>>(),
            ReferenceExport {
                entity: |reference| Ok(Arc::new(reference.typed::<T>()?)),
                item: TypeId::of::<T>(),
                value,
                make: |app, collection, position| {
                    let key = app
                        .resolve_collection(collection)?
                        .arena::<T>()
                        .ok_or_else(|| pixui_error!("wrong reference collection type"))?
                        .iter()
                        .nth(position)
                        .map(|(key, _)| key)
                        .ok_or_else(|| pixui_error!("missing loop item"))?;
                    Ok(Arc::new(app.object_ref_at(collection, key)?))
                },
            },
        );
    }
}
fn insert<T>(map: &mut HashMap<String, T>, name: &str, value: T) -> PixuiResult<()> {
    let mut characters = name.chars();
    let valid = characters
        .next()
        .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_')
        && characters.all(|ch| ch.is_ascii_alphanumeric() || ch == '_');
    if !valid || map.contains_key(name) {
        return Err(pixui_error!(
            "invalid or duplicate language export `{name}`"
        ));
    }
    map.insert(name.to_owned(), value);
    Ok(())
}

fn validate_identifier(name: &str) -> PixuiResult<()> {
    if name.is_empty()
        || !name.bytes().enumerate().all(|(index, ch)| {
            ch.is_ascii_alphabetic() || ch == b'_' || (index > 0 && ch.is_ascii_digit())
        })
    {
        return Err(pixui_error!("invalid language identifier `{name}`"));
    }
    Ok(())
}
