//! Explicit host exports. No source name implies a slice, Rust type, or callback.
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
use pixui_base::{PixuiResult, erased_value::SendValue, pixui_error};
use pixui_reflect::{DynamicObject, Reflect, TypeDescriptor};
use std::{
    any::{Any, TypeId},
    collections::HashMap,
    sync::Arc,
};

pub type Snapshot = Arc<dyn Any + Send + Sync>;
#[derive(Clone)]
pub struct ValueType {
    pub descriptor: &'static TypeDescriptor,
    pub(crate) snapshot: fn(&DynamicObject<'_>) -> PixuiResult<Snapshot>,
    pub(crate) send: fn(&dyn Any) -> PixuiResult<SendValue>,
}
impl ValueType {
    pub fn of<T: Reflect + Clone + Send + Sync>() -> Self {
        Self {
            descriptor: T::type_descriptor(),
            snapshot: |value| {
                Ok(Arc::new(
                    value
                        .downcast_ref::<T>()
                        .ok_or_else(|| pixui_error!("expected {}", std::any::type_name::<T>()))?
                        .clone(),
                ))
            },
            send: |value| {
                Ok(Box::new(
                    value
                        .downcast_ref::<T>()
                        .ok_or_else(|| pixui_error!("incorrect snapshot type"))?
                        .clone(),
                ))
            },
        }
    }
}
#[derive(Clone)]
pub struct ComponentExport {
    pub props: &'static TypeDescriptor,
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
#[derive(Clone)]
pub struct ReferenceExport {
    pub item: TypeId,
    pub value: ValueType,
    pub(crate) entity:
        fn(&crate::application::erased_object_ref::ErasedObjectRef) -> PixuiResult<Snapshot>,
    pub(crate) make: fn(&Application, CollectionKey, usize) -> PixuiResult<Snapshot>,
}
/// Environments are application-local. Duplicate exports are rejected atomically.
#[derive(Clone)]
pub struct Environment {
    pub(crate) components: HashMap<String, ComponentExport>,
    pub(crate) collections: HashMap<String, DataExport>,
    pub(crate) entities: HashMap<String, DataExport>,
    pub(crate) actions: HashMap<String, ActionHandle>,
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
            .any(|field| matches!(field.name, "layout" | "styles" | "on_activate"))
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
                build: Arc::new(move |types| {
                    Arc::new(LanguageBinding::<C> {
                        address,
                        types,
                        marker: std::marker::PhantomData,
                    })
                }),
            },
        )
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
    pub fn action(&mut self, name: &str, handle: ActionHandle) -> PixuiResult<()> {
        insert(&mut self.actions, name, handle)
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
