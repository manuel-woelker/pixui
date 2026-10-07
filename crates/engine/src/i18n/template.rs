//! Compiled named interpolation with bounded UTF-8 output and no executable syntax.
use pixui_base::{PixuiResult, pixui_error};
use std::collections::BTreeSet;

pub const MAX_TEMPLATE_BYTES: usize = 64 * 1024;
pub const MAX_OUTPUT_BYTES: usize = 1024 * 1024;
pub const MAX_NESTING: usize = 32;

#[derive(Clone, Debug)]
enum Segment {
    Literal(String),
    Argument(String),
}

#[derive(Clone, Debug)]
pub struct CompiledTemplate {
    segments: Vec<Segment>,
    names: BTreeSet<String>,
}
impl CompiledTemplate {
    /// Parse {name}, with {{ and }} representing literal braces.
    pub fn parse(text: &str) -> PixuiResult<Self> {
        if text.len() > MAX_TEMPLATE_BYTES {
            return Err(pixui_error!("i18n template exceeds byte limit"));
        }
        let mut chars = text.chars().peekable();
        let mut literal = String::new();
        let mut segments = Vec::new();
        let mut names = BTreeSet::new();
        while let Some(c) = chars.next() {
            match c {
                '{' if chars.peek() == Some(&'{') => {
                    chars.next();
                    literal.push('{');
                }
                '}' if chars.peek() == Some(&'}') => {
                    chars.next();
                    literal.push('}');
                }
                '{' => {
                    if !literal.is_empty() {
                        segments.push(Segment::Literal(std::mem::take(&mut literal)));
                    }
                    let mut name = String::new();
                    loop {
                        match chars.next() {
                            Some('}') => break,
                            Some(c) if c.is_ascii_alphanumeric() || c == '_' => name.push(c),
                            _ => return Err(pixui_error!("invalid i18n placeholder")),
                        }
                    }
                    if !name.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_') {
                        return Err(pixui_error!("invalid i18n placeholder name"));
                    }
                    names.insert(name.clone());
                    segments.push(Segment::Argument(name));
                }
                '}' => return Err(pixui_error!("unmatched closing brace in i18n template")),
                c => literal.push(c),
            }
        }
        if !literal.is_empty() {
            segments.push(Segment::Literal(literal));
        }
        Ok(Self { segments, names })
    }

    pub fn names(&self) -> &BTreeSet<String> {
        &self.names
    }

    pub(crate) fn render(&self, arguments: &[(String, String)]) -> PixuiResult<String> {
        let mut output = String::new();
        for segment in &self.segments {
            let text = match segment {
                Segment::Literal(text) => text,
                Segment::Argument(name) => {
                    &arguments
                        .iter()
                        .find(|(key, _)| key == name)
                        .ok_or_else(|| pixui_error!("missing i18n argument `{name}`"))?
                        .1
                }
            };
            if text.len() > MAX_OUTPUT_BYTES - output.len() {
                return Err(pixui_error!("i18n output exceeds byte limit"));
            }
            output.push_str(text);
        }
        Ok(output)
    }
}
