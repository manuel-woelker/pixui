//! Named recipes resolve against presentation tokens, then one variant and states.
use super::{
    style::{PaintStyle, StatePatch, StylePatch, StyleStates},
    theme::ThemeTokens,
};
use pixui_base::{PixuiResult, pixui_error};
use std::collections::BTreeMap;
/// Pure recipe: evaluated once per occurrence/slot, before solver probes.
pub type StyleRecipe = fn(&ThemeTokens) -> PaintStyle;
#[derive(Clone)]
pub struct StyleFamily {
    pub base: StyleRecipe,
    pub variants: BTreeMap<String, StylePatch>,
    pub active_tokens: Option<fn(&ThemeTokens) -> StatePatch>,
    pub active: StatePatch,
    pub hover: StatePatch,
    pub disabled: StatePatch,
}
impl StyleFamily {
    pub fn new(base: StyleRecipe) -> Self {
        Self {
            base,
            active_tokens: None,
            variants: BTreeMap::new(),
            active: Default::default(),
            hover: Default::default(),
            disabled: Default::default(),
        }
    }
    pub(crate) fn derived(&self, tokens: &ThemeTokens) -> Self {
        let mut family = self.clone();
        if let Some(resolve) = self.active_tokens {
            family.active = resolve(tokens);
        }
        family
    }
    pub(crate) fn resolve(
        &self,
        tokens: &ThemeTokens,
        assignment: &StyleAssignment,
    ) -> PixuiResult<PaintStyle> {
        let mut style = (self.base)(tokens);
        if let Some(variant) = &assignment.variant {
            self.variants
                .get(variant)
                .ok_or_else(|| pixui_error!("unknown style variant `{variant}`"))?
                .apply(&mut style);
        }
        Ok(style)
    }
}
/// Persistent application-local family identity, never a frame's StyleIndex.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StyleFamilyId {
    pub(crate) registry: u64,
    pub(crate) index: usize,
}
#[derive(Clone)]
pub struct StyleAssignment {
    pub family: StyleFamilyId,
    pub variant: Option<String>,
    pub patch: StylePatch,
    pub active: StatePatch,
    pub hover: StatePatch,
    pub disabled: StatePatch,
}
impl StyleAssignment {
    pub fn new(family: StyleFamilyId) -> Self {
        Self {
            family,
            variant: None,
            patch: Default::default(),
            active: Default::default(),
            hover: Default::default(),
            disabled: Default::default(),
        }
    }
    pub fn variant(mut self, name: impl Into<String>) -> Self {
        self.variant = Some(name.into());
        self
    }
    pub fn patch(mut self, patch: StylePatch) -> Self {
        self.patch = patch;
        self
    }
}
/// Base/variant -> family active/hover/disabled -> local patch -> local states.
/// Focus is a separate painter slot. Disabled here only changes appearance.
pub(crate) fn apply_states(
    style: &mut PaintStyle,
    family: &StyleFamily,
    assignment: &StyleAssignment,
    states: StyleStates,
) {
    for (enabled, patch) in [
        (states.active, family.active),
        (states.hovered, family.hover),
        (states.disabled, family.disabled),
    ] {
        if enabled {
            patch.apply(style);
        }
    }
    assignment.patch.apply(style);
    for (enabled, patch) in [
        (states.active, assignment.active),
        (states.hovered, assignment.hover),
        (states.disabled, assignment.disabled),
    ] {
        if enabled {
            patch.apply(style);
        }
    }
}
