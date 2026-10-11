//! Reflection discovers slots; construction binds indices without field setters.
use super::{family::StyleAssignment, index::StyleIndex};
use pixui_base::{PixuiResult, pixui_error};
use pixui_reflect::{Reflect, TypeDescriptor};
use std::{any::TypeId, collections::HashMap};
#[derive(Clone)]
pub struct SlotAssignment {
    pub slot: String,
    pub style: StyleAssignment,
}
impl SlotAssignment {
    pub fn new(slot: impl Into<String>, style: StyleAssignment) -> Self {
        Self {
            slot: slot.into(),
            style,
        }
    }
}
/// All fields in a styled painter must be StyleIndex. Configurable non-slot
/// painters use the original raw registration path; no cloning/Sync required.
pub struct PainterSlots {
    descriptor: &'static TypeDescriptor,
    names: HashMap<&'static str, usize>,
}
impl PainterSlots {
    pub fn discover<P: Reflect>() -> PixuiResult<Self> {
        let descriptor = P::type_descriptor();
        if !descriptor.is_send_constructible() {
            return Err(pixui_error!(
                "styled painter requires a sendable constructor"
            ));
        }
        let mut names = HashMap::new();
        for (index, field) in descriptor.fields().iter().enumerate() {
            if field.type_id() != Some(TypeId::of::<StyleIndex>()) {
                return Err(pixui_error!(
                    "styled painter field `{}` must be StyleIndex",
                    field.name
                ));
            }
            names.insert(field.name, index);
        }
        if names.is_empty() {
            return Err(pixui_error!("styled painter has no slots"));
        }
        Ok(Self { descriptor, names })
    }
    pub fn index(&self, name: &str) -> PixuiResult<usize> {
        self.names
            .get(name)
            .copied()
            .ok_or_else(|| pixui_error!("unknown painter slot `{name}`"))
    }
    pub fn names(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.descriptor.fields().iter().map(|field| field.name)
    }
    pub fn assignments(&self, values: Vec<SlotAssignment>) -> PixuiResult<Vec<StyleAssignment>> {
        let mut ordered = vec![None; self.names.len()];
        for value in values {
            let index = *self
                .names
                .get(value.slot.as_str())
                .ok_or_else(|| pixui_error!("unknown painter slot `{}`", value.slot))?;
            if ordered[index].replace(value.style).is_some() {
                return Err(pixui_error!("duplicate painter slot `{}`", value.slot));
            }
        }
        ordered
            .into_iter()
            .enumerate()
            .map(|(index, value)| {
                value.ok_or_else(|| {
                    pixui_error!(
                        "missing painter slot `{}`",
                        self.descriptor.fields()[index].name
                    )
                })
            })
            .collect()
    }
    pub fn construct<P: Reflect + Send>(&self, indices: &[StyleIndex]) -> PixuiResult<P> {
        self.descriptor
            .construct_send(
                indices
                    .iter()
                    .map(|index| Box::new(*index) as pixui_base::erased_value::SendValue)
                    .collect(),
            )?
            .downcast::<P>()
            .map(|value| *value)
            .map_err(|_| pixui_error!("styled painter constructor type mismatch"))
    }
}
