//! GNU PO/POT adapter. Brace placeholders are validated by the engine, not gettext.
use super::{
    catalog::{MessageDeclaration, MessageKey, TranslationCatalog, TranslationEntry},
    format::TranslationFormat,
};
use ferrocat_po::{Header, MsgStr, PoFile, PoItem, SerializeOptions};
use pixui_base::{PixuiResult, pixui_error};

/// UTF-8, singular-message PO import and deterministic POT export.
/// Fuzzy/obsolete/empty translations are skipped; plural messages are rejected.
pub struct PoFormat;
impl TranslationFormat for PoFormat {
    fn import(&self, input: &str) -> PixuiResult<TranslationCatalog> {
        if input.len() > 16 * 1024 * 1024 {
            return Err(pixui_error!("PO catalog exceeds 16 MiB"));
        }
        validate_quoted_lines(input)?;
        let file = ferrocat_po::parse_po(input)
            .map_err(|error| pixui_error!("parse PO catalog: {error}"))?;
        if file.headers.iter().any(|header| {
            header.key.eq_ignore_ascii_case("Content-Type")
                && !header.value.split(';').any(|parameter| {
                    parameter.split_once('=').is_some_and(|(name, value)| {
                        name.trim().eq_ignore_ascii_case("charset")
                            && value.trim().trim_matches('"').eq_ignore_ascii_case("utf-8")
                    })
                })
        }) {
            return Err(pixui_error!("PO catalog must use UTF-8"));
        }
        let mut catalog = TranslationCatalog::default();
        let mut keys = std::collections::BTreeSet::new();
        for item in file.items {
            if item.obsolete || item.flags.iter().any(|flag| flag == "fuzzy") {
                catalog.diagnostics.push(format!(
                    "skipped obsolete/fuzzy translation: {}",
                    item.msgid
                ));
                continue;
            }
            if item.msgid_plural.is_some() || matches!(item.msgstr, MsgStr::Plural(_)) {
                return Err(pixui_error!(
                    "PO plural messages are not supported: {}",
                    item.msgid
                ));
            }
            let key = MessageKey {
                source: item.msgid,
                context: item.msgctxt,
            };
            if !keys.insert(key.clone()) {
                return Err(pixui_error!("duplicate PO message key"));
            }
            let MsgStr::Singular(translation) = item.msgstr else {
                catalog
                    .diagnostics
                    .push(format!("missing translation: {}", key.source));
                continue;
            };
            if translation.is_empty() {
                catalog
                    .diagnostics
                    .push(format!("empty translation: {}", key.source));
                continue;
            }
            catalog.entries.push(TranslationEntry { key, translation });
        }
        Ok(catalog)
    }
    fn export(&self, messages: &[MessageDeclaration]) -> PixuiResult<String> {
        let mut messages: Vec<_> = messages.iter().collect();
        messages
            .sort_by(|a, b| (&a.key.context, &a.key.source).cmp(&(&b.key.context, &b.key.source)));
        let file = PoFile {
            headers: vec![
                Header {
                    key: "MIME-Version".into(),
                    value: "1.0".into(),
                },
                Header {
                    key: "Content-Type".into(),
                    value: "text/plain; charset=UTF-8".into(),
                },
                Header {
                    key: "Content-Transfer-Encoding".into(),
                    value: "8bit".into(),
                },
            ],
            items: messages
                .into_iter()
                .map(|message| {
                    let mut comments: Vec<_> = message
                        .comments
                        .iter()
                        .flat_map(|comment| comment.lines().map(str::to_owned))
                        .collect();
                    if !message.placeholders.is_empty() {
                        comments.push(format!(
                            "Placeholders: {}",
                            message
                                .placeholders
                                .iter()
                                .cloned()
                                .collect::<Vec<_>>()
                                .join(", ")
                        ));
                    }
                    PoItem {
                        msgid: message.key.source.clone(),
                        msgctxt: message.key.context.clone(),
                        references: message.locations.iter().cloned().collect(),
                        extracted_comments: comments,
                        msgstr: MsgStr::Singular(String::new()),
                        ..Default::default()
                    }
                })
                .collect(),
            ..Default::default()
        };
        Ok(ferrocat_po::stringify_po(
            &file,
            &SerializeOptions::default(),
        ))
    }
}

// The upstream parser accepts some unterminated quoted lines. Validate the
// lexical boundary before parsing so malformed translations never install.
fn validate_quoted_lines(input: &str) -> PixuiResult<()> {
    for (number, line) in input.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let quote = line
            .find('"')
            .ok_or_else(|| pixui_error!("missing PO quote on line {}", number + 1))?;
        let prefix = line[..quote].trim();
        if prefix == "msgid_plural" || prefix.starts_with("msgstr[") {
            return Err(pixui_error!(
                "PO plural messages are not supported on line {}",
                number + 1
            ));
        }
        if !matches!(prefix, "" | "msgid" | "msgctxt" | "msgstr") {
            return Err(pixui_error!("unknown PO directive on line {}", number + 1));
        }
        let mut escaped = false;
        let mut closed = false;
        for (offset, character) in line[quote + 1..].char_indices() {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                closed = line[quote + 2 + offset..].trim().is_empty();
                break;
            }
        }
        if !closed {
            return Err(pixui_error!(
                "invalid quoted PO string on line {}",
                number + 1
            ));
        }
    }
    Ok(())
}
