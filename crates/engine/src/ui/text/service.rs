//! Worker cache with per-configuration atomic publication and LRU ownership.

use super::{
    font::{FontConfig, FontKey},
    packing::Packing,
    resource::{Font, FontResource},
};
use pixui_base::{PixuiResult, pixui_error};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// Bounded cache ownership. Retained render outputs may keep evicted resources
/// alive; these limits cannot bound resources intentionally retained by clients.
#[derive(Clone, Copy, Debug)]
pub struct TextLimits {
    pub initial_dimension: u32,
    pub maximum_dimension: u32,
    pub configurations: usize,
    pub coverage_bytes: usize,
    pub characters_per_font: usize,
}
impl Default for TextLimits {
    fn default() -> Self {
        Self {
            initial_dimension: 128,
            maximum_dimension: 2048,
            configurations: 16,
            coverage_bytes: 32 * 1024 * 1024,
            characters_per_font: 4096,
        }
    }
}

struct CachedFont {
    config: FontConfig,
    packing: Packing,
    resource: Font,
}

/// Concrete worker-owned service; no locks or backend plugin machinery. A
/// changed batch produces one resource, unchanged batches clone only its Arc.
pub struct TextService {
    limits: TextLimits,
    // Oldest first. Linear lookup is deliberate: at most sixteen by default.
    cache: Vec<CachedFont>,
    rasterized_glyphs: u64,
}
impl Default for TextService {
    fn default() -> Self {
        Self::new(TextLimits::default()).expect("valid default text limits")
    }
}

impl TextService {
    pub fn new(limits: TextLimits) -> PixuiResult<Self> {
        if !limits.initial_dimension.is_power_of_two()
            || !limits.maximum_dimension.is_power_of_two()
            || limits.initial_dimension < 4
            || limits.initial_dimension > limits.maximum_dimension
            || limits.maximum_dimension > 2048
            || limits.configurations == 0
            || limits.coverage_bytes
                < limits.maximum_dimension as usize * limits.maximum_dimension as usize
            || limits.characters_per_font == 0
            || limits.characters_per_font > 4096
        {
            return Err(pixui_error!("invalid text cache limits"));
        }
        Ok(Self {
            limits,
            cache: Vec::new(),
            rasterized_glyphs: 0,
        })
    }
    /// Cumulative work counter, useful for checking that animated frames reuse
    /// coverage. Failed preparations can count rasterization without publication.
    pub fn rasterized_glyphs(&self) -> u64 {
        self.rasterized_glyphs
    }
    pub fn cached_configurations(&self) -> usize {
        self.cache.len()
    }
    pub fn cached_coverage_bytes(&self) -> usize {
        self.cache
            .iter()
            .map(|entry| entry.resource.atlas().coverage().len())
            .sum()
    }

    /// Finalizes one render's union of characters. Each font commits atomically;
    /// successful earlier fonts remain cached if a later font fails.
    pub fn prepare(
        &mut self,
        config: &FontConfig,
        requested: &BTreeSet<char>,
    ) -> PixuiResult<Font> {
        if requested.iter().any(|character| character.is_control()) {
            return Err(pixui_error!("normalize controls before preparing glyphs"));
        }
        let key: FontKey = config.key();
        let index = self
            .cache
            .iter()
            .position(|entry| entry.config.key() == key);
        if let Some(index) = index
            && requested
                .iter()
                .all(|character| self.cache[index].resource.glyph(*character).is_some())
        {
            let entry = self.cache.remove(index);
            let result = entry.resource.clone();
            self.cache.push(entry);
            return Ok(result);
        }
        let old = index.map(|index| &self.cache[index]);
        let mut characters: BTreeSet<char> = old.map_or_else(BTreeSet::new, |entry| {
            entry.resource.characters().keys().copied().collect()
        });
        characters.extend(requested);
        if characters.len() > self.limits.characters_per_font {
            return Err(pixui_error!("font character alias limit exceeded"));
        }
        let fresh = Packing::new(self.limits.initial_dimension);
        let packing = old.map_or(&fresh, |entry| &entry.packing);
        let canonical: BTreeSet<_> = characters
            .iter()
            .map(|&character| config.face.0.canonical(character))
            .collect();
        // Reject an impossible batch before allocating temporary glyph bitmaps.
        // Padded area is a lower bound; fragmentation is checked by the packer.
        let maximum = self.limits.maximum_dimension;
        let mut area = 0_u64;
        for &character in &canonical {
            let (width, height) = if let Some(info) = packing.glyphs.get(&character) {
                info.atlas_rect
                    .map_or((0, 0), |rect| (rect.width, rect.height))
            } else {
                config
                    .face
                    .0
                    .dimensions(character, config.size, config.scale)?
            };
            if width != 0 && height != 0 {
                area += u64::from(width + 2) * u64::from(height + 2);
                if width + 2 > maximum
                    || height + 2 > maximum
                    || area > u64::from(maximum) * u64::from(maximum)
                {
                    return Err(pixui_error!(
                        "font glyphs exceed single-atlas limit {maximum}"
                    ));
                }
            }
        }
        let mut rasterized = BTreeMap::new();
        for character in canonical {
            if !packing.glyphs.contains_key(&character) {
                rasterized.insert(
                    character,
                    config
                        .face
                        .0
                        .rasterize(character, config.size, config.scale)?,
                );
                self.rasterized_glyphs = self.rasterized_glyphs.saturating_add(1);
            }
        }
        // Alias-only changes share pixels; drawable changes copy exactly once.
        let (packing, atlas) = if rasterized.is_empty()
            && let Some(entry) = old
        {
            (entry.packing.clone(), entry.resource.atlas().clone())
        } else {
            let (packing, atlas) = packing.prepare(
                old.map(|entry| &**entry.resource.atlas()),
                &rasterized,
                self.limits.maximum_dimension,
            )?;
            (packing, atlas)
        };
        let map: HashMap<_, _> = characters
            .into_iter()
            .map(|character| {
                (
                    character,
                    packing.glyphs[&config.face.0.canonical(character)],
                )
            })
            .collect();
        let resource = Font::from_value(FontResource::new(
            atlas,
            map,
            config.metrics(),
            config.scale,
        )?);
        if let Some(index) = index {
            self.cache.remove(index);
        }
        self.cache.push(CachedFont {
            config: config.clone(),
            packing,
            resource: resource.clone(),
        });
        while self.cache.len() > self.limits.configurations
            || self.cached_coverage_bytes() > self.limits.coverage_bytes
        {
            self.cache.remove(0);
        }
        Ok(resource)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        application::app::Application,
        components::label::{LabelComponent, LabelProps},
        live_model::part::{ComponentPart, LivePart},
        painters::label::LabelPainter,
        ui::{definition::UiDefinition, input::UiCommand, presentation::PresentationSettings},
    };

    #[test]
    fn failed_finalization_preserves_last_good_revision_geometry_and_snapshot() {
        let mut app = Application::default();
        *app.text_service.get_mut() = TextService::new(TextLimits {
            initial_dimension: 32,
            maximum_dimension: 32,
            ..Default::default()
        })
        .unwrap();
        let label = app.register_component::<LabelComponent>("label").unwrap();
        app.register_painter::<LabelComponent>(LabelPainter)
            .unwrap();
        let part = ComponentPart::typed(label, |_, settings| {
            Ok(LabelProps {
                text: settings.locale.clone(),
            })
        });
        let definition = app
            .register_ui(UiDefinition::new("text", LivePart::Component(part)))
            .unwrap();
        let (instance, outputs) = app
            .uis
            .create(
                definition,
                PresentationSettings {
                    locale: "A".into(),
                    ..Default::default()
                },
            )
            .unwrap();
        app.render_dirty();
        let good = outputs.try_recv().unwrap();
        let geometry = app
            .uis
            .instance(instance)
            .unwrap()
            .layout()
            .component_bounds
            .clone();
        app.ui_command(UiCommand::Present {
            instance,
            settings: PresentationSettings {
                locale: (' '..='~').collect(),
                ..Default::default()
            },
        })
        .unwrap();
        app.render_dirty();
        assert!(outputs.try_recv().is_err());
        let state = app.uis.instance(instance).unwrap();
        assert_eq!(state.revision(), good.revision);
        assert_eq!(state.layout().component_bounds, geometry);
        assert!(state.last_error().unwrap().contains("single-atlas limit"));
        app.ui_command(UiCommand::Present {
            instance,
            settings: PresentationSettings {
                locale: "A".into(),
                ..Default::default()
            },
        })
        .unwrap();
        app.render_dirty();
        let recovered = outputs.try_recv().unwrap();
        assert!(recovered.revision > good.revision);
        assert!(Font::ptr_eq(
            &good.display_list.fonts[0],
            &recovered.display_list.fonts[0]
        ));
    }
}
