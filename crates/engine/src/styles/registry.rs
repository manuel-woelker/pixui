#![doc = include_str!("README.md")]
//! Validated application-local families and replaceable Light/Dark token sets.
use super::{
    family::{StyleAssignment, StyleFamily, StyleFamilyId, apply_states},
    style::{PaintStyle, StyleStates},
    theme::ThemeTokens,
};
use crate::ui::presentation::Theme;
use pixui_base::{PixuiResult, pixui_error};
use std::{
    collections::HashMap,
    sync::atomic::{AtomicU64, Ordering},
};
#[derive(Clone)]
pub struct StyleRegistry {
    id: u64,
    names: HashMap<String, usize>,
    families: Vec<StyleFamily>,
    themes: [ThemeTokens; 2],
    revision: u64,
}
impl Default for StyleRegistry {
    fn default() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self {
            id: NEXT.fetch_add(1, Ordering::Relaxed),
            names: HashMap::new(),
            families: Vec::new(),
            themes: [
                ThemeTokens::for_theme(Theme::Light),
                ThemeTokens::for_theme(Theme::Dark),
            ],
            revision: 0,
        }
    }
}
impl StyleRegistry {
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn tokens(&self, theme: Theme) -> &ThemeTokens {
        &self.themes[usize::from(theme == Theme::Dark)]
    }
    pub fn family(&self, name: &str) -> PixuiResult<StyleFamilyId> {
        Ok(StyleFamilyId {
            registry: self.id,
            index: *self
                .names
                .get(name)
                .ok_or_else(|| pixui_error!("unknown style family `{name}`"))?,
        })
    }
    pub fn register(
        &mut self,
        name: impl Into<String>,
        family: StyleFamily,
    ) -> PixuiResult<StyleFamilyId> {
        let name = name.into();
        if name.is_empty() || self.names.contains_key(&name) {
            return Err(pixui_error!("empty or duplicate style family `{name}`"));
        }
        self.validate_family(&family)?;
        let id = StyleFamilyId {
            registry: self.id,
            index: self.families.len(),
        };
        self.names.insert(name, id.index);
        self.families.push(family);
        self.revision += 1;
        Ok(id)
    }
    pub fn replace(&mut self, id: StyleFamilyId, family: StyleFamily) -> PixuiResult<()> {
        let previous = self.get(id)?;
        if previous
            .variants
            .keys()
            .any(|name| !family.variants.contains_key(name))
        {
            return Err(pixui_error!(
                "replacement must retain existing style variants"
            ));
        }
        self.validate_family(&family)?;
        self.families[id.index] = family;
        self.revision += 1;
        Ok(())
    }
    pub fn set_tokens(&mut self, theme: Theme, tokens: ThemeTokens) -> PixuiResult<()> {
        tokens.validate()?;
        let mut candidate = self.clone();
        candidate.themes[usize::from(theme == Theme::Dark)] = tokens;
        for family in &candidate.families {
            candidate.validate_family(family)?;
        }
        candidate.revision += 1;
        *self = candidate;
        Ok(())
    }
    pub fn resolve(
        &self,
        assignment: &StyleAssignment,
        theme: Theme,
        scale: f32,
        states: StyleStates,
    ) -> PixuiResult<PaintStyle> {
        let family = self.get(assignment.family)?.derived(self.tokens(theme));
        let mut style = family.resolve(self.tokens(theme), assignment)?;
        apply_states(&mut style, &family, assignment, states);
        style.validate(scale)?;
        Ok(style)
    }
    fn get(&self, id: StyleFamilyId) -> PixuiResult<&StyleFamily> {
        if id.registry != self.id {
            return Err(pixui_error!("style family belongs to another application"));
        }
        self.families
            .get(id.index)
            .ok_or_else(|| pixui_error!("invalid style family index"))
    }
    fn validate_family(&self, family: &StyleFamily) -> PixuiResult<()> {
        for tokens in &self.themes {
            let base = (family.base)(tokens);
            base.validate(1.0)?;
            for (name, patch) in &family.variants {
                if name.is_empty() {
                    return Err(pixui_error!("empty style variant"));
                }
                let mut style = base.clone();
                patch.apply(&mut style);
                style.validate(1.0)?;
            }
        }
        Ok(())
    }
}

/// Holds the evaluated recipe so state refresh after layout never reruns it.
pub(crate) struct PreparedStyle {
    base: PaintStyle,
    family: StyleFamily,
    assignment: StyleAssignment,
}
impl PreparedStyle {
    pub(crate) fn resolve(&self, states: StyleStates, scale: f32) -> PixuiResult<PaintStyle> {
        let mut style = self.base.clone();
        apply_states(&mut style, &self.family, &self.assignment, states);
        style.validate(scale)?;
        Ok(style)
    }
}
impl StyleRegistry {
    pub(crate) fn prepare(
        &self,
        assignment: &StyleAssignment,
        theme: Theme,
    ) -> PixuiResult<PreparedStyle> {
        let family = self.get(assignment.family)?.derived(self.tokens(theme));
        Ok(PreparedStyle {
            base: family.resolve(self.tokens(theme), assignment)?,
            family,
            assignment: assignment.clone(),
        })
    }
}
