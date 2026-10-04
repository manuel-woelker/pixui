#![doc = include_str!("README.md")]

use super::component_id::{ComponentAddress, ComponentId};
use crate::{
    live_model::{component::Component, part::LivePart, state::GenericComponentState},
    painters::registry::PainterRegistry,
};
use pixui_base::{PixuiResult, pixui_error};
use std::{
    any::{TypeId, type_name},
    collections::HashMap,
    marker::PhantomData,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_REGISTRY: AtomicU64 = AtomicU64::new(1);

/// Immutable metadata. The initializer creates a fresh default for each
/// physical node; descriptors never contain per-instance state.
pub struct ComponentDescriptor {
    pub name: String,
    pub component_type: TypeId,
    pub props_type: TypeId,
    pub state_type: TypeId,
    pub props_type_name: &'static str,
    pub state_type_name: &'static str,
    pub(crate) initialize: fn() -> GenericComponentState,
}

pub struct ComponentRegistry {
    identity: u64,
    descriptors: Vec<ComponentDescriptor>,
    types: HashMap<TypeId, usize>,
    names: HashMap<String, usize>,
}

impl Default for ComponentRegistry {
    fn default() -> Self {
        let identity = NEXT_REGISTRY
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .expect("component registry identity exhausted");
        Self {
            identity,
            descriptors: Vec::new(),
            types: HashMap::new(),
            names: HashMap::new(),
        }
    }
}

impl ComponentRegistry {
    /// Rejects duplicate names and component types without modifying registration.
    pub fn register<C: Component>(
        &mut self,
        name: impl Into<String>,
    ) -> PixuiResult<ComponentId<C>> {
        let name = name.into();
        if name.is_empty()
            || self.names.contains_key(&name)
            || self.types.contains_key(&TypeId::of::<C>())
        {
            return Err(pixui_error!(
                "empty or duplicate component registration `{name}`"
            ));
        }
        let index = self.descriptors.len();
        self.names.insert(name.clone(), index);
        self.types.insert(TypeId::of::<C>(), index);
        self.descriptors.push(ComponentDescriptor {
            name,
            component_type: TypeId::of::<C>(),
            props_type: TypeId::of::<C::Props>(),
            state_type: TypeId::of::<C::State>(),
            props_type_name: type_name::<C::Props>(),
            state_type_name: type_name::<C::State>(),
            initialize: || GenericComponentState::new(C::State::default()),
        });
        self.component::<C>()
    }

    pub fn component<C: Component>(&self) -> PixuiResult<ComponentId<C>> {
        let index = *self
            .types
            .get(&TypeId::of::<C>())
            .ok_or_else(|| pixui_error!("unregistered component `{}`", type_name::<C>()))?;
        Ok(ComponentId {
            address: ComponentAddress {
                registry: self.identity,
                index,
                component_type: TypeId::of::<C>(),
            },
            marker: PhantomData,
        })
    }

    pub fn descriptor<C: Component>(
        &self,
        id: ComponentId<C>,
    ) -> PixuiResult<&ComponentDescriptor> {
        self.resolve(id.address)
    }

    pub(crate) fn resolve(&self, address: ComponentAddress) -> PixuiResult<&ComponentDescriptor> {
        if address.registry != self.identity {
            return Err(pixui_error!("component belongs to a different registry"));
        }
        self.descriptors
            .get(address.index)
            .filter(|entry| entry.component_type == address.component_type)
            .ok_or_else(|| pixui_error!("invalid component registration index"))
    }

    /// Validates the entire template, including bodies of currently empty loops.
    pub fn validate(&self, template: &LivePart, painters: &PainterRegistry) -> PixuiResult<()> {
        let mut pending = vec![template];
        while let Some(part) = pending.pop() {
            match part {
                LivePart::Composite(composite) => pending.extend(&composite.parts),
                LivePart::ForLoop(part) => pending.push(&part.body),
                LivePart::Component(part) => {
                    if let Some(address) = part.component_address() {
                        let descriptor = self.resolve(address)?;
                        painters.require(address, descriptor)?;
                    }
                }
            }
        }
        Ok(())
    }

    pub(crate) fn initialize(
        &self,
        address: ComponentAddress,
    ) -> PixuiResult<GenericComponentState> {
        Ok((self.resolve(address)?.initialize)().with_component(address))
    }
}
