//! Stable bundle identities and atomic worker-side graph replacement.
use super::{
    bundle::{BundleLimits, InstalledBundle},
    bundle_prepare::{DefinitionKey, PreparedBundle, Signature},
    resolution::environment::Environment,
};
use crate::{
    application::app::Application,
    composition::{component_ref::ComponentRef, registry::Entry},
    live_model::part::{CompositePart, LivePart},
};
use pixui_base::{PixuiResult, pixui_error};
use std::{collections::BTreeMap, sync::Arc};

/// Application-local bundle identity. Reloading does not change its references.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BundleId {
    owner: u64,
    index: usize,
}
#[derive(Clone)]
struct Slot {
    reference: ComponentRef,
    signature: Signature,
}
pub(crate) struct BundleRecord {
    pub entries: Vec<String>,
    pub environment: Environment,
    pub domain: String,
    pub limits: BundleLimits,
    pub revision: u64,
    slots: BTreeMap<DefinitionKey, Slot>,
    installed: InstalledBundle,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ConfigurationStamp {
    templates: u64,
    styles: u64,
    host: u64,
    translations: u64,
}
impl ConfigurationStamp {
    pub fn current(app: &Application) -> Self {
        Self {
            templates: app.templates.revision(),
            styles: app.styles.revision(),
            host: app.ui_configuration_revision,
            translations: app.translations.revision(),
        }
    }
}
pub(crate) struct BundleSnapshot {
    pub environment: Environment,
    pub translations: crate::i18n::registry::TranslationRegistry,
    pub entries: Vec<String>,
    pub domain: String,
    pub limits: BundleLimits,
    pub stamp: ConfigurationStamp,
    pub revision: u64,
}
impl Application {
    /// Find a uniquely installed bundle by one canonical configured entry path.
    pub fn bundle_for_entry(&self, entry: &str) -> PixuiResult<InstalledBundle> {
        let mut matches = self
            .bundles
            .iter()
            .filter(|record| record.entries.iter().any(|name| name == entry));
        let record = matches
            .next()
            .ok_or_else(|| pixui_error!("no UI bundle for entry `{entry}`"))?;
        if matches.next().is_some() {
            return Err(pixui_error!("ambiguous UI bundle entry `{entry}`"));
        }
        Ok(record.installed.clone())
    }

    pub(crate) fn bundle_snapshot(&self, id: BundleId) -> PixuiResult<BundleSnapshot> {
        let record = self.bundle_record(id)?;
        Ok(BundleSnapshot {
            environment: record.environment.clone(),
            translations: self.translations.clone(),
            entries: record.entries.clone(),
            domain: record.domain.clone(),
            limits: record.limits,
            stamp: ConfigurationStamp::current(self),
            revision: record.revision,
        })
    }
    fn bundle_record(&self, id: BundleId) -> PixuiResult<&BundleRecord> {
        if id.owner != self.templates.owner() {
            return Err(pixui_error!("foreign UI bundle"));
        }
        self.bundles
            .get(id.index)
            .ok_or_else(|| pixui_error!("unknown UI bundle"))
    }
    pub(crate) fn accept_bundle(
        &mut self,
        id: BundleId,
        base_revision: u64,
        stamp: ConfigurationStamp,
        prepared: PreparedBundle,
    ) -> PixuiResult<bool> {
        let record = self.bundle_record(id)?;
        if record.revision != base_revision || ConfigurationStamp::current(self) != stamp {
            return Ok(false);
        }
        let entries = record.entries.clone();
        let environment = record.environment.clone();
        let domain = record.domain.clone();
        let limits = record.limits;
        install(
            self,
            prepared,
            entries,
            environment,
            domain,
            limits,
            Some(id),
        )?;
        self.uis.reset_after_source_reload();
        Ok(true)
    }
    /// The current export table. Existing InstalledBundle values retain their
    /// original export snapshots; surviving ComponentRefs continue to resolve live.
    pub fn installed_bundle(&self, id: BundleId) -> PixuiResult<InstalledBundle> {
        Ok(self.bundle_record(id)?.installed.clone())
    }
}

pub(crate) fn install(
    app: &mut Application,
    mut prepared: PreparedBundle,
    entries: Vec<String>,
    environment: Environment,
    domain: String,
    limits: BundleLimits,
    previous: Option<BundleId>,
) -> PixuiResult<InstalledBundle> {
    let id = previous.unwrap_or(BundleId {
        owner: app.templates.owner(),
        index: app.bundles.len(),
    });
    let mut slots = if previous.is_some() {
        app.bundle_record(id)?.slots.clone()
    } else {
        BTreeMap::new()
    };
    if previous.is_some() {
        let old = app.bundle_record(id)?;
        for entry in &entries {
            if old
                .installed
                .modules
                .get(entry)
                .is_some_and(|module| module.entry.is_some())
                && prepared
                    .modules
                    .get(entry)
                    .is_none_or(|module| module.entry.is_none())
            {
                return Err(pixui_error!("configured UI entry `{entry}` was removed"));
            }
        }
    }
    let mut candidate = app.templates.clone();
    let mut links = vec![];
    for definition in &prepared.definitions {
        let reference = if let Some(slot) = slots.get(&definition.key) {
            if slot.signature != definition.signature {
                return Err(pixui_error!(
                    "incompatible public UI signature for {:?}",
                    definition.key
                ));
            }
            slot.reference
        } else {
            let reference = candidate.insert(Entry {
                name: definition.key.0.clone(),
                domain: domain.clone(),
                template: Arc::new(LivePart::Composite(CompositePart { parts: vec![] })),
                unbound: false,
            })?;
            slots.insert(
                definition.key.clone(),
                Slot {
                    reference,
                    signature: definition.signature.clone(),
                },
            );
            reference
        };
        links.push(reference);
    }
    for (key, slot) in &slots {
        if !prepared
            .definitions
            .iter()
            .any(|definition| &definition.key == key)
        {
            candidate.remove(slot.reference);
        }
    }
    for (definition, &reference) in prepared.definitions.iter_mut().zip(&links) {
        super::bundle_link::link(&mut definition.body, &links)?;
        app.components.validate(&definition.body, &app.painters)?;
        app.components.resolve_events(&mut definition.body)?;
        app.painters
            .validate_template_styles(&definition.body, &app.styles)?;
        candidate.replace(
            reference,
            Entry {
                name: definition
                    .key
                    .1
                    .clone()
                    .unwrap_or_else(|| definition.key.0.clone()),
                domain: domain.clone(),
                template: Arc::new(definition.body.clone()),
                unbound: !definition.signature.props.is_empty() || definition.signature.children,
            },
        )?;
    }
    for definition in &prepared.definitions {
        candidate.validate(&definition.body, &domain)?;
    }
    for module in prepared.modules.values_mut() {
        if let Some(reference) = &mut module.entry {
            *reference = links[reference.index];
        }
        for export in module.exports.values_mut() {
            let index = export.reference.expect("prepared symbol").index;
            export.reference = Some(links[index]);
            Arc::make_mut(&mut export.definition).body = prepared.definitions[index].body.clone();
        }
    }
    let installed = InstalledBundle {
        modules: prepared.modules,
        id,
    };
    let revision = previous.map_or(0, |_| app.bundles[id.index].revision + 1);
    let record = BundleRecord {
        entries,
        environment,
        domain,
        limits,
        revision,
        slots,
        installed: installed.clone(),
    };
    app.templates = candidate;
    app.translations = prepared.translations;
    if previous.is_some() {
        app.bundles[id.index] = record;
    } else {
        app.bundles.push(record);
    }
    Ok(installed)
}
