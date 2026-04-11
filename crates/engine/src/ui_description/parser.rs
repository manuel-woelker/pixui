use crate::ui_description::ast::ui_element::UiElement;
use crate::ui_description::ast::ui_property::UiProperty;
use crate::ui_description::lexer::lex;
use crate::ui_description::token::Token;
use crate::ui_description::token_kind::TokenKind;
use pixui_base::bail;
use pixui_base::result::PixuiResult;
use pixui_base::shared_string::SharedString;
use pixui_base::span::Span;

pub fn parse_ui_description(source: &str) -> PixuiResult<UiElement> {
    let tokens = lex(source)?;
    Parser::new(tokens).parse()
}

/// Parser for the JSX-like UI description language.
pub struct Parser {
    tokens: Vec<Token>,
    position: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            position: 0,
        }
    }

    pub fn parse(mut self) -> PixuiResult<UiElement> {
        let element = self.parse_element()?;

        if let Some(token) = self.peek() {
            bail!("Unexpected trailing token at byte {}", token.span.start());
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
                Some(_) => bail!(
                    "Unexpected token inside `<{}>` at byte {}",
                    tag_name,
                    self.peek()
                        .map(|token| token.span.start())
                        .unwrap_or_default()
                ),
                None => bail!("Missing closing tag for `<{}>`", tag_name),
            }
        }

        self.expect_punctuation(
            |kind| matches!(kind, TokenKind::LessThanSlash),
            "Expected `</` to close the element",
        )?;
        let closing_tag_name = self.expect_identifier("Expected a tag name after `</`")?;

        if closing_tag_name != tag_name {
            bail!(
                "Mismatched closing tag: expected `</{}>` but found `</{}>`",
                tag_name,
                closing_tag_name
            );
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
            Some(token) => bail!("{} at byte {}", message, token.span.start()),
            None => bail!("{message}"),
        }
    }

    fn expect_string_literal_token(&mut self, message: &str) -> PixuiResult<Token> {
        match self.advance() {
            Some(token) if matches!(token.kind, TokenKind::StringLiteral(_)) => Ok(token),
            Some(token) => bail!("{} at byte {}", message, token.span.start()),
            None => bail!("{message}"),
        }
    }

    fn expect_punctuation(
        &mut self,
        predicate: impl FnOnce(&TokenKind) -> bool,
        message: &str,
    ) -> PixuiResult<Span> {
        match self.advance() {
            Some(token) if predicate(&token.kind) => Ok(token.span),
            Some(token) => bail!("{} at byte {}", message, token.span.start()),
            None => bail!("{message}"),
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
}

#[cfg(test)]
mod tests {
    use super::parse_ui_description;
    use crate::ui_description::ast::ui_element::UiElement;
    use crate::ui_description::ast::ui_property::UiProperty;
    use pixui_base::shared_string::SharedString;
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
        let error = parse_ui_description(r#"<Stack></Button>"#).unwrap_err();

        assert!(
            error
                .to_test_string()
                .contains("Mismatched closing tag: expected `</Stack>` but found `</Button>`")
        );
    }

    #[test]
    fn rejects_missing_closing_tags_at_end_of_file() {
        let error = parse_ui_description(r#"<Stack><Button />"#).unwrap_err();

        assert!(
            error
                .to_test_string()
                .contains("Missing closing tag for `<Stack>`")
        );
    }

    #[test]
    fn rejects_missing_equals_in_property_syntax() {
        let error = parse_ui_description(r#"<Button label "Save" />"#).unwrap_err();

        assert!(
            error
                .to_test_string()
                .contains("Expected `=` after a property name")
        );
    }

    #[test]
    fn rejects_malformed_string_literals_from_the_parse_api() {
        let error = parse_ui_description(r#"<Button label="Save />"#).unwrap_err();

        assert!(
            error
                .to_test_string()
                .contains("Unterminated string literal")
        );
    }
}
