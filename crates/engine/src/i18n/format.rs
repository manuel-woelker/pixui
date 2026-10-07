//! Catalog formats are explicit adapters, independent of runtime indexing.
use super::catalog::{MessageDeclaration, TranslationCatalog};
use pixui_base::PixuiResult;

pub trait TranslationFormat {
    fn import(&self, input: &str) -> PixuiResult<TranslationCatalog>;
    fn export(&self, messages: &[MessageDeclaration]) -> PixuiResult<String>;
}
