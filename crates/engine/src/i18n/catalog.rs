//! Format-independent source keys and translator-facing catalog data.
use std::collections::BTreeSet;

/// Domain is supplied by the catalog installation/export operation, not individual messages.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MessageKey {
    pub source: String,
    pub context: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MessageDeclaration {
    pub key: MessageKey,
    pub placeholders: BTreeSet<String>,
    pub comments: BTreeSet<String>,
    pub locations: BTreeSet<String>,
}
#[derive(Clone, Debug)]
pub struct TranslationEntry {
    pub key: MessageKey,
    pub translation: String,
}
/// One domain/language's translations. Missing entries intentionally fall back.
#[derive(Clone, Debug, Default)]
pub struct TranslationCatalog {
    pub entries: Vec<TranslationEntry>,
    pub diagnostics: Vec<String>,
}
/// Nonfatal skipped/unknown entries; the caller decides how to surface them.
#[derive(Clone, Debug, Default)]
pub struct CatalogReport {
    pub diagnostics: Vec<String>,
}
