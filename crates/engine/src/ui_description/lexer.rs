use crate::ui_description::token::Token;
use crate::ui_description::token_kind::TokenKind;
use pixui_base::bail;
use pixui_base::result::PixuiResult;
use pixui_base::shared_string::SharedString;
use pixui_base::span::Span;

pub fn lex(source: &str) -> PixuiResult<Vec<Token>> {
    Lexer::new(source).lex()
}

/// Lexer for the JSX-like UI description language.
pub struct Lexer<'a> {
    source: &'a str,
    position: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(source: &'a str) -> Self {
        Self {
            source,
            position: 0,
        }
    }

    pub fn lex(mut self) -> PixuiResult<Vec<Token>> {
        let mut tokens = Vec::new();

        while !self.is_eof() {
            self.skip_whitespace();

            if self.is_eof() {
                break;
            }

            tokens.push(self.lex_token()?);
        }

        Ok(tokens)
    }

    fn lex_token(&mut self) -> PixuiResult<Token> {
        let start = self.position;

        match self.current_char() {
            Some('<') => {
                self.advance_char();
                if self.current_char() == Some('/') {
                    self.advance_char();
                    Ok(Token {
                        kind: TokenKind::LessThanSlash,
                        span: Span::new(start, self.position),
                    })
                } else {
                    Ok(Token {
                        kind: TokenKind::LessThan,
                        span: Span::new(start, self.position),
                    })
                }
            }
            Some('>') => {
                self.advance_char();
                Ok(Token {
                    kind: TokenKind::GreaterThan,
                    span: Span::new(start, self.position),
                })
            }
            Some('/') => {
                self.advance_char();
                if self.current_char() != Some('>') {
                    bail!("Unexpected character `/` at byte {}", start);
                }

                self.advance_char();
                Ok(Token {
                    kind: TokenKind::SlashGreaterThan,
                    span: Span::new(start, self.position),
                })
            }
            Some('=') => {
                self.advance_char();
                Ok(Token {
                    kind: TokenKind::Equals,
                    span: Span::new(start, self.position),
                })
            }
            Some('"') => self.lex_string_literal(),
            Some(character) if is_identifier_start(character) => self.lex_identifier(),
            Some(character) => bail!("Unexpected character `{character}` at byte {}", start),
            None => bail!("Unexpected end of input"),
        }
    }

    fn lex_identifier(&mut self) -> PixuiResult<Token> {
        let start = self.position;
        self.advance_char();

        while let Some(character) = self.current_char() {
            if !is_identifier_continue(character) {
                break;
            }

            self.advance_char();
        }

        Ok(Token {
            kind: TokenKind::Identifier(self.source[start..self.position].into()),
            span: Span::new(start, self.position),
        })
    }

    fn lex_string_literal(&mut self) -> PixuiResult<Token> {
        let start = self.position;
        self.advance_char();

        let mut value = SharedString::empty();

        while let Some(character) = self.current_char() {
            match character {
                '"' => {
                    self.advance_char();
                    return Ok(Token {
                        kind: TokenKind::StringLiteral(value),
                        span: Span::new(start, self.position),
                    });
                }
                '\\' => {
                    self.advance_char();
                    let escaped = match self.current_char() {
                        Some('"') => "\"",
                        Some('\\') => "\\",
                        Some('n') => "\n",
                        Some('r') => "\r",
                        Some('t') => "\t",
                        Some(other) => bail!(
                            "Unsupported escape sequence `\\{other}` at byte {}",
                            self.position.saturating_sub(1)
                        ),
                        None => bail!("Unterminated string literal starting at byte {}", start),
                    };
                    value.push_str(escaped);
                    self.advance_char();
                }
                '\n' | '\r' => bail!("Unterminated string literal starting at byte {}", start),
                _ => {
                    let end = self.position + character.len_utf8();
                    value.push_str(&self.source[self.position..end]);
                    self.position = end;
                }
            }
        }

        bail!("Unterminated string literal starting at byte {}", start)
    }

    fn skip_whitespace(&mut self) {
        while let Some(character) = self.current_char() {
            if !character.is_whitespace() {
                break;
            }

            self.advance_char();
        }
    }

    fn current_char(&self) -> Option<char> {
        self.source[self.position..].chars().next()
    }

    fn advance_char(&mut self) {
        if let Some(character) = self.current_char() {
            self.position += character.len_utf8();
        }
    }

    fn is_eof(&self) -> bool {
        self.position >= self.source.len()
    }
}

fn is_identifier_start(character: char) -> bool {
    character.is_ascii_alphabetic() || character == '_'
}

fn is_identifier_continue(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '_' || character == '-'
}

#[cfg(test)]
mod tests {
    use super::lex;
    use crate::ui_description::token_kind::TokenKind;
    use pixui_base::shared_string::SharedString;
    use pixui_base::span::Span;

    #[test]
    fn lexes_simple_self_closing_tag() {
        let tokens = lex(r#"<Button />"#).unwrap();

        assert_eq!(
            tokens.iter().map(|token| &token.kind).collect::<Vec<_>>(),
            vec![
                &TokenKind::LessThan,
                &TokenKind::Identifier(SharedString::from("Button")),
                &TokenKind::SlashGreaterThan,
            ]
        );
        assert_eq!(tokens[0].span, Span::new(0, 1));
        assert_eq!(tokens[1].span, Span::new(1, 7));
        assert_eq!(tokens[2].span, Span::new(8, 10));
    }

    #[test]
    fn lexes_nested_tags_and_string_properties() {
        let tokens = lex(r#"<Stack><Button label="Save" /></Stack>"#).unwrap();

        assert_eq!(
            tokens.iter().map(|token| &token.kind).collect::<Vec<_>>(),
            vec![
                &TokenKind::LessThan,
                &TokenKind::Identifier(SharedString::from("Stack")),
                &TokenKind::GreaterThan,
                &TokenKind::LessThan,
                &TokenKind::Identifier(SharedString::from("Button")),
                &TokenKind::Identifier(SharedString::from("label")),
                &TokenKind::Equals,
                &TokenKind::StringLiteral(SharedString::from("Save")),
                &TokenKind::SlashGreaterThan,
                &TokenKind::LessThanSlash,
                &TokenKind::Identifier(SharedString::from("Stack")),
                &TokenKind::GreaterThan,
            ]
        );
    }

    #[test]
    fn rejects_unterminated_string_literals() {
        let error = lex(r#"<Button label="Save />"#).unwrap_err();

        assert!(
            error
                .to_test_string()
                .contains("Unterminated string literal")
        );
    }

    #[test]
    fn rejects_unexpected_characters() {
        let error = lex("{").unwrap_err();

        assert!(
            error
                .to_test_string()
                .contains("Unexpected character `{` at byte 0")
        );
    }
}
