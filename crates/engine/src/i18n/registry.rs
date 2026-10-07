#![doc = include_str!("README.md")]

//! Worker-owned append-only message indices and dense per-language translations.
use super::{
    catalog::{CatalogReport, MessageDeclaration, MessageKey, TranslationCatalog},
    expression::I18nExpression,
    indices::{LanguageIndex, MessageIndex},
    template::{CompiledTemplate, MAX_NESTING},
};
use crate::expression::expression::{Expression, ExpressionKind};
use pixui_base::{PixuiResult, pixui_error};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_OWNER: AtomicU64 = AtomicU64::new(1);
#[derive(Clone)]
struct Message {
    domain: String,
    declaration: MessageDeclaration,
    source: CompiledTemplate,
}
#[derive(Clone)]
struct Language {
    tag: String,
    templates: Vec<Option<CompiledTemplate>>,
    catalogs: BTreeMap<String, BTreeMap<MessageKey, CompiledTemplate>>,
}
/// Cloning preserves identity for transactional registration. Do not use clones
/// as separate applications; construct a fresh registry instead.
#[derive(Clone)]
pub struct TranslationRegistry {
    owner: u64,
    messages: Vec<Option<Message>>,
    indices: BTreeMap<(String, MessageKey), usize>,
    languages: Vec<Language>,
}
impl Default for TranslationRegistry {
    fn default() -> Self {
        let owner = NEXT_OWNER
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .expect("i18n registry identity exhausted");
        Self {
            owner,
            messages: vec![None],
            indices: BTreeMap::new(),
            languages: vec![Language {
                tag: "en".into(),
                templates: vec![None],
                catalogs: BTreeMap::new(),
            }],
        }
    }
}
impl TranslationRegistry {
    /// Select the source tag before registering additional languages.
    pub fn set_source_language(&mut self, tag: &str) -> PixuiResult<()> {
        if self.languages.len() != 1 {
            return Err(pixui_error!(
                "configure source language before translations"
            ));
        }
        self.languages[0].tag = language_tag(tag)?;
        Ok(())
    }
    /// Register or reuse an exact tag (ASCII case insensitive); no regional fallback.
    pub fn register_language(&mut self, tag: &str) -> PixuiResult<LanguageIndex> {
        let tag = language_tag(tag)?;
        if let Some(slot) = self
            .languages
            .iter()
            .position(|language| language.tag == tag)
        {
            return Ok(self.language_index(slot));
        }
        let slot = self.languages.len();
        self.languages.push(Language {
            tag,
            templates: vec![None; self.messages.len()],
            catalogs: BTreeMap::new(),
        });
        Ok(self.language_index(slot))
    }
    pub fn language(&self, tag: &str) -> PixuiResult<LanguageIndex> {
        let tag = language_tag(tag)?;
        self.languages
            .iter()
            .position(|language| language.tag == tag)
            .map(|slot| self.language_index(slot))
            .ok_or_else(|| pixui_error!("unknown translation language `{tag}`"))
    }
    pub fn language_tag(&self, index: LanguageIndex) -> PixuiResult<&str> {
        self.validate_language(index)?;
        Ok(&self.languages[index.slot].tag)
    }
    fn language_index(&self, slot: usize) -> LanguageIndex {
        if slot == 0 {
            LanguageIndex::source()
        } else {
            LanguageIndex {
                slot,
                owner: self.owner,
            }
        }
    }
    pub fn validate_language(&self, index: LanguageIndex) -> PixuiResult<()> {
        if (index.slot == 0 && index.owner == 0)
            || (index.owner == self.owner && index.slot < self.languages.len())
        {
            return Ok(());
        }
        Err(pixui_error!(
            "unknown or foreign translation language index"
        ))
    }

    /// Resolve a standalone declaration atomically. UI registration uses the same
    /// resolver across its full declaration tree in a single transaction.
    pub fn register_expression(
        &mut self,
        domain: &str,
        expression: &Expression,
    ) -> PixuiResult<Expression> {
        validate_domain(domain)?;
        let mut next = self.clone();
        let mut expression = expression.clone();
        next.resolve(domain, &mut expression, 0)?;
        *self = next;
        Ok(expression)
    }
    pub(crate) fn resolve(
        &mut self,
        domain: &str,
        expression: &mut Expression,
        depth: usize,
    ) -> PixuiResult<()> {
        if depth >= MAX_NESTING {
            return Err(pixui_error!("i18n nesting exceeds limit"));
        }
        if let ExpressionKind::I18n(message) = expression.kind_mut() {
            let source = message.validate()?;
            for (_, argument) in &mut message.arguments {
                self.resolve(domain, argument, depth + 1)?;
            }
            message.index = self.intern(domain, message, source)?;
        }
        Ok(())
    }
    fn intern(
        &mut self,
        domain: &str,
        expression: &I18nExpression,
        source: CompiledTemplate,
    ) -> PixuiResult<MessageIndex> {
        let key = (domain.to_owned(), expression.key.clone());
        let slot = if let Some(&slot) = self.indices.get(&key) {
            let message = self.messages[slot].as_mut().expect("registered message");
            if message.declaration.placeholders != *source.names() {
                return Err(pixui_error!("conflicting i18n placeholder schema"));
            }
            if let Some(comment) = &expression.comment {
                message.declaration.comments.insert(comment.clone());
            }
            message
                .declaration
                .locations
                .insert(expression.location.clone());
            slot
        } else {
            let slot = self.messages.len();
            self.indices.insert(key, slot);
            self.messages.push(Some(Message {
                domain: domain.into(),
                declaration: MessageDeclaration {
                    key: expression.key.clone(),
                    placeholders: source.names().clone(),
                    comments: expression.comment.iter().cloned().collect(),
                    locations: BTreeSet::from([expression.location.clone()]),
                },
                source,
            }));
            for language in &mut self.languages {
                language.templates.push(
                    language
                        .catalogs
                        .get(domain)
                        .and_then(|catalog| catalog.get(&expression.key))
                        .cloned(),
                );
            }
            slot
        };
        Ok(MessageIndex {
            slot,
            owner: self.owner,
        })
    }

    /// Return source declarations for one domain, sorted by stable source/context key.
    pub fn declarations(&self, domain: &str) -> Vec<MessageDeclaration> {
        self.indices
            .iter()
            .filter(|((name, _), _)| name == domain)
            .map(|(_, &slot)| {
                self.messages[slot]
                    .as_ref()
                    .expect("registered message")
                    .declaration
                    .clone()
            })
            .collect()
    }
    pub fn domains(&self) -> Vec<String> {
        self.indices
            .keys()
            .map(|(domain, _)| domain.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    /// Replace exactly one domain/language catalog. Validate even unknown entries
    /// so later definitions can safely use catalogs loaded before registration.
    pub fn install(
        &mut self,
        domain: &str,
        language: LanguageIndex,
        catalog: TranslationCatalog,
    ) -> PixuiResult<CatalogReport> {
        validate_domain(domain)?;
        self.validate_language(language)?;
        if language.slot == 0 {
            return Err(pixui_error!(
                "source language cannot have a translation catalog"
            ));
        }
        let mut compiled = BTreeMap::new();
        let mut report = CatalogReport {
            diagnostics: catalog.diagnostics,
        };
        for entry in catalog.entries {
            if entry.key.source.is_empty() {
                return Err(pixui_error!("empty i18n catalog source"));
            }
            if entry.translation.is_empty() {
                report
                    .diagnostics
                    .push(format!("empty translation: {}", entry.key.source));
                continue;
            }
            let source = CompiledTemplate::parse(&entry.key.source)?;
            let translated = CompiledTemplate::parse(&entry.translation)?;
            if source.names() != translated.names() {
                return Err(pixui_error!(
                    "translation placeholders differ from source: {}",
                    entry.key.source
                ));
            }
            if !self
                .indices
                .contains_key(&(domain.into(), entry.key.clone()))
            {
                report
                    .diagnostics
                    .push(format!("unmatched translation: {}", entry.key.source));
            }
            if compiled.insert(entry.key, translated).is_some() {
                return Err(pixui_error!("duplicate translation catalog key"));
            }
        }
        let target = &mut self.languages[language.slot];
        for (slot, message) in self.messages.iter().enumerate().skip(1) {
            let message = message.as_ref().expect("registered message");
            if message.domain == domain {
                target.templates[slot] = compiled.get(&message.declaration.key).cloned();
            }
        }
        target.catalogs.insert(domain.into(), compiled);
        Ok(report)
    }
    pub(crate) fn template(
        &self,
        index: MessageIndex,
        language: LanguageIndex,
    ) -> PixuiResult<&CompiledTemplate> {
        self.validate_language(language)?;
        if index.slot == 0 || index.owner != self.owner {
            return Err(pixui_error!("unresolved or foreign i18n message index"));
        }
        let message = self
            .messages
            .get(index.slot)
            .and_then(Option::as_ref)
            .ok_or_else(|| pixui_error!("unknown i18n message index"))?;
        Ok(self.languages[language.slot].templates[index.slot]
            .as_ref()
            .unwrap_or(&message.source))
    }
}
pub(crate) fn validate_domain(domain: &str) -> PixuiResult<()> {
    if domain.trim().is_empty() {
        return Err(pixui_error!("translation domain must not be empty"));
    }
    Ok(())
}
fn language_tag(tag: &str) -> PixuiResult<String> {
    if tag.len() > 64
        || tag
            .split('-')
            .any(|part| part.is_empty() || !part.bytes().all(|b| b.is_ascii_alphanumeric()))
        || !tag
            .split('-')
            .next()
            .is_some_and(|part| part.bytes().all(|b| b.is_ascii_alphabetic()))
    {
        return Err(pixui_error!("invalid translation language tag `{tag}`"));
    }
    Ok(tag.to_ascii_lowercase())
}
