use crate::ui_description::ast::ui_element::UiElement;
use crate::ui_description::ast::ui_property::UiProperty;
use crate::ui_description::diagnostic::ui_description_error;
use crate::ui_description::lexer::lex_source_file;
use crate::ui_description::token::Token;
use crate::ui_description::token_kind::TokenKind;
use pixui_base::result::PixuiResult;
use pixui_base::shared_string::SharedString;
use pixui_base::source_file::SourceFile;
use pixui_base::span::Span;

pub fn parse_ui_description(source: &str) -> PixuiResult<UiElement> {
    parse_ui_description_source_file(&SourceFile::new("<ui_description>", source))
}

pub fn parse_ui_description_source_file(source_file: &SourceFile) -> PixuiResult<UiElement> {
    let tokens = lex_source_file(source_file)?;
    Parser::new(source_file, tokens).parse()
}

/// Parser for the JSX-like UI description language.
pub struct Parser<'a> {
    source_file: &'a SourceFile,
    tokens: Vec<Token>,
    position: usize,
}

impl<'a> Parser<'a> {
    pub fn new(source_file: &'a SourceFile, tokens: Vec<Token>) -> Self {
        Self {
            source_file,
            tokens,
            position: 0,
        }
    }

    pub fn parse(mut self) -> PixuiResult<UiElement> {
        let element = self.parse_element()?;

        if let Some(token) = self.peek() {
            return Err(self.error(
                token.span.clone(),
                "Unexpected trailing token",
                "remove this token or wrap it in a parent element",
            ));
        }

        Ok(element)
    }

    fn parse_element(&mut self) -> PixuiResult<UiElement> {
        let start_span = self.expect_punctuation(
            |kind| matches!(kind, TokenKind::LessThan),
            "Expected `<` to start an element",
        )?;
        let tag_name = self.expect_identifier("Expected a tag name after `<`")?;
        let mut properties = Vec::new();

        while self.peek_identifier().is_some() {
            properties.push(self.parse_property()?);
        }

        if let Some(token) = self.peek()
            && matches!(token.kind, TokenKind::SlashGreaterThan)
        {
            let end_span = self.advance().expect("peeked token should exist").span;
            return Ok(UiElement {
                tag_name,
                properties,
                children: Vec::new(),
                span: Span::new(start_span.start(), end_span.end()),
            });
        }

        self.expect_punctuation(
            |kind| matches!(kind, TokenKind::GreaterThan),
            "Expected `>` or `/>` after an opening tag",
        )?;

        let mut children = Vec::new();
        loop {
            match self.peek().map(|token| &token.kind) {
                Some(TokenKind::LessThan) => children.push(self.parse_element()?),
                Some(TokenKind::LessThanSlash) => break,
                Some(_) => {
                    let token = self.peek().expect("peek matched some token");
                    return Err(self.error(
                        token.span.clone(),
                        format!("Unexpected token inside `<{}>`", tag_name),
                        "only child elements or a closing tag are allowed here",
                    ));
                }
                None => {
                    return Err(self.error(
                        Span::new(start_span.start(), start_span.end()),
                        format!("Missing closing tag for `<{}>`", tag_name),
                        "this element is opened here but never closed",
                    ));
                }
            }
        }

        self.expect_punctuation(
            |kind| matches!(kind, TokenKind::LessThanSlash),
            "Expected `</` to close the element",
        )?;
        let closing_tag_name = self.expect_identifier("Expected a tag name after `</`")?;

        if closing_tag_name != tag_name {
            let closing_span = self.previous_span();
            return Err(self.error(
                closing_span,
                format!(
                    "Mismatched closing tag: expected `</{}>` but found `</{}>`",
                    tag_name, closing_tag_name
                ),
                "this closing tag does not match the currently open element",
            ));
        }

        let end_span = self.expect_punctuation(
            |kind| matches!(kind, TokenKind::GreaterThan),
            "Expected `>` after a closing tag",
        )?;

        Ok(UiElement {
            tag_name,
            properties,
            children,
            span: Span::new(start_span.start(), end_span.end()),
        })
    }

    fn parse_property(&mut self) -> PixuiResult<UiProperty> {
        let name_token = self.expect_identifier_token("Expected a property name")?;
        self.expect_punctuation(
            |kind| matches!(kind, TokenKind::Equals),
            "Expected `=` after a property name",
        )?;
        let value_token =
            self.expect_string_literal_token("Expected a string literal after `=`")?;

        let TokenKind::Identifier(name) = name_token.kind else {
            unreachable!("property name token must be an identifier");
        };
        let TokenKind::StringLiteral(value) = value_token.kind else {
            unreachable!("property value token must be a string literal");
        };

        Ok(UiProperty {
            name,
            value,
            span: Span::new(name_token.span.start(), value_token.span.end()),
        })
    }

    fn expect_identifier(&mut self, message: &str) -> PixuiResult<SharedString> {
        let token = self.expect_identifier_token(message)?;
        let TokenKind::Identifier(identifier) = token.kind else {
            unreachable!("identifier token must contain an identifier");
        };
        Ok(identifier)
    }

    fn expect_identifier_token(&mut self, message: &str) -> PixuiResult<Token> {
        match self.advance() {
            Some(token) if matches!(token.kind, TokenKind::Identifier(_)) => Ok(token),
            Some(token) => Err(self.error(token.span.clone(), message, "unexpected token here")),
            None => Err(self.eof_error(message)),
        }
    }

    fn expect_string_literal_token(&mut self, message: &str) -> PixuiResult<Token> {
        match self.advance() {
            Some(token) if matches!(token.kind, TokenKind::StringLiteral(_)) => Ok(token),
            Some(token) => Err(self.error(
                token.span.clone(),
                message,
                "expected a quoted string value",
            )),
            None => Err(self.eof_error(message)),
        }
    }

    fn expect_punctuation(
        &mut self,
        predicate: impl FnOnce(&TokenKind) -> bool,
        message: &str,
    ) -> PixuiResult<Span> {
        match self.advance() {
            Some(token) if predicate(&token.kind) => Ok(token.span),
            Some(token) => Err(self.error(token.span.clone(), message, "unexpected token here")),
            None => Err(self.eof_error(message)),
        }
    }

    fn peek_identifier(&self) -> Option<&SharedString> {
        match self.peek() {
            Some(Token {
                kind: TokenKind::Identifier(identifier),
                ..
            }) => Some(identifier),
            _ => None,
        }
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.position)
    }

    fn advance(&mut self) -> Option<Token> {
        let token = self.tokens.get(self.position).cloned();
        if token.is_some() {
            self.position += 1;
        }
        token
    }

    fn previous_span(&self) -> Span {
        self.tokens
            .get(self.position.saturating_sub(1))
            .map(|token| token.span.clone())
            .unwrap_or_default()
    }

    fn eof_error(&self, message: &str) -> pixui_base::error::PixuiError {
        let span = self
            .tokens
            .last()
            .map(|token| Span::new(token.span.end(), token.span.end()))
            .unwrap_or_default();
        self.error(span, message, "the parser reached the end of the file here")
    }

    fn error(
        &self,
        span: Span,
        summary: impl Into<String>,
        annotation: impl Into<String>,
    ) -> pixui_base::error::PixuiError {
        ui_description_error(self.source_file, span, summary, annotation)
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_ui_description, parse_ui_description_source_file};
    use crate::ui_description::ast::ui_element::UiElement;
    use crate::ui_description::ast::ui_property::UiProperty;
    use pixui_base::cli::format_cli_error;
    use pixui_base::shared_string::SharedString;
    use pixui_base::source_file::SourceFile;
    use pixui_base::span::Span;

    #[test]
    fn parses_a_self_closing_component_with_a_string_property() {
        let element = parse_ui_description(r#"<Button label="Save" />"#).unwrap();

        assert_eq!(
            element,
            UiElement {
                tag_name: SharedString::from("Button"),
                properties: vec![UiProperty {
                    name: SharedString::from("label"),
                    value: SharedString::from("Save"),
                    span: Span::new(8, 20),
                }],
                children: Vec::new(),
                span: Span::new(0, 23),
            }
        );
    }

    #[test]
    fn parses_a_parent_with_multiple_child_elements() {
        let element = parse_ui_description(
            r#"<Stack><Button label="Save" /><Button label="Cancel" /></Stack>"#,
        )
        .unwrap();

        assert_eq!(element.tag_name, "Stack");
        assert_eq!(element.children.len(), 2);
        assert_eq!(element.children[0].tag_name, "Button");
        assert_eq!(element.children[0].properties[0].value, "Save");
        assert_eq!(element.children[1].properties[0].value, "Cancel");
    }

    #[test]
    fn parses_multiple_string_properties() {
        let element = parse_ui_description(r#"<Button label="Save" variant="primary" />"#).unwrap();

        assert_eq!(element.properties.len(), 2);
        assert_eq!(element.properties[0].name, "label");
        assert_eq!(element.properties[0].value, "Save");
        assert_eq!(element.properties[1].name, "variant");
        assert_eq!(element.properties[1].value, "primary");
    }

    #[test]
    fn rejects_mismatched_closing_tags() {
        let source_file = SourceFile::new("examples/ui.pixui", r#"<Stack></Button>"#);
        let error = parse_ui_description_source_file(&source_file).unwrap_err();
        let rendered = pixui_base::unansi(&format_cli_error("parsing failed", &error));

        assert!(
            rendered.contains(
                "error: Mismatched closing tag: expected `</Stack>` but found `</Button>`"
            )
        );
        assert!(rendered.contains("examples/ui.pixui:1:10"));
        assert!(rendered.contains("does not match the currently open element"));
    }

    #[test]
    fn rejects_missing_closing_tags_at_end_of_file() {
        let source_file = SourceFile::new("examples/ui.pixui", r#"<Stack><Button />"#);
        let error = parse_ui_description_source_file(&source_file).unwrap_err();
        let rendered = pixui_base::unansi(&format_cli_error("parsing failed", &error));

        assert!(rendered.contains("error: Missing closing tag for `<Stack>`"));
        assert!(rendered.contains("examples/ui.pixui:1:1"));
        assert!(rendered.contains("opened here but never closed"));
    }

    #[test]
    fn rejects_missing_equals_in_property_syntax() {
        let source_file = SourceFile::new("examples/ui.pixui", r#"<Button label "Save" />"#);
        let error = parse_ui_description_source_file(&source_file).unwrap_err();
        let rendered = pixui_base::unansi(&format_cli_error("parsing failed", &error));

        assert!(rendered.contains("error: Expected `=` after a property name"));
        assert!(rendered.contains("examples/ui.pixui:1:15"));
        assert!(rendered.contains("unexpected token here"));
    }

    #[test]
    fn rejects_malformed_string_literals_from_the_parse_api() {
        let source_file = SourceFile::new("examples/ui.pixui", r#"<Button label="Save />"#);
        let error = parse_ui_description_source_file(&source_file).unwrap_err();
        let rendered = pixui_base::unansi(&format_cli_error("parsing failed", &error));

        assert!(rendered.contains("error: Unterminated string literal"));
        assert!(rendered.contains("examples/ui.pixui:1:15"));
        assert!(rendered.contains("missing its closing quote"));
    }
}
