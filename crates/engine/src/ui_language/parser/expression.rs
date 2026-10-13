//! Pratt operators and located interpolation templates.
use super::parse::{ParseResult, Parser};
use crate::ui_language::{
    ast::expression::{Expression, ExpressionKind, Name, Operator, Segment, SegmentKind},
    diagnostic::report::{Diagnostic, Stage},
    lexer::{
        scan::{interpolation_end, lex_range},
        token::TokenKind,
    },
    source::span::Span,
};
impl Parser<'_> {
    pub(super) fn expression(&mut self, min: u8) -> ParseResult<Expression> {
        self.enter()?;
        let result = self.expression_inner(min);
        self.depth -= 1;
        result
    }
    fn expression_inner(&mut self, min: u8) -> ParseResult<Expression> {
        let token = self.take();
        let start = token.span;
        let mut left = match token.kind {
            TokenKind::Bang => {
                let inner = self.expression(7)?;
                Expression {
                    span: start.join(inner.span),
                    kind: ExpressionKind::Not(Box::new(inner)),
                }
            }
            TokenKind::LeftParen => {
                let mut inner = self.expression(0)?;
                inner.span = start.join(self.expect(TokenKind::RightParen)?.span);
                inner
            }
            TokenKind::Number(value) => Expression {
                span: start,
                kind: ExpressionKind::Number(value),
            },
            TokenKind::String {
                translated,
                value: _,
            } => self.template(start, translated)?,
            TokenKind::Name(value) if value == "true" || value == "false" => Expression {
                span: start,
                kind: ExpressionKind::Bool(value == "true"),
            },
            TokenKind::Name(value) => {
                let mut path = vec![Name { value, span: start }];
                while self.consume(&TokenKind::Dot).is_some() {
                    path.push(self.name()?);
                }
                let mut span = start.join(path.last().unwrap().span);
                let kind = if self.consume(&TokenKind::LeftParen).is_some() {
                    let mut arguments = vec![];
                    if !self.is(&TokenKind::RightParen) {
                        loop {
                            let name = self.name()?;
                            self.expect(TokenKind::Colon)?;
                            let value = self.expression(0)?;
                            arguments.push(crate::ui_language::ast::expression::Argument {
                                span: name.span.join(value.span),
                                name,
                                value,
                            });
                            if self.consume(&TokenKind::Comma).is_none()
                                || self.is(&TokenKind::RightParen)
                            {
                                break;
                            }
                        }
                    }
                    span = span.join(self.expect(TokenKind::RightParen)?.span);
                    ExpressionKind::Call { path, arguments }
                } else {
                    ExpressionKind::Path(path)
                };
                Expression { kind, span }
            }
            _ => {
                return Err(Diagnostic::new(
                    Stage::Parser,
                    "P006",
                    start,
                    "expected expression",
                ));
            }
        };
        loop {
            let (op, precedence) = match self.token().kind {
                TokenKind::Or => (Operator::Or, 1),
                TokenKind::And => (Operator::And, 3),
                TokenKind::EqualEqual => (Operator::Equal, 5),
                TokenKind::NotEqual => (Operator::NotEqual, 5),
                _ => break,
            };
            if precedence < min {
                break;
            }
            self.take();
            let right = self.expression(precedence + 1)?;
            left = Expression {
                span: left.span.join(right.span),
                kind: ExpressionKind::Binary {
                    op,
                    left: Box::new(left),
                    right: Box::new(right),
                },
            };
        }
        Ok(left)
    }
    fn template(&mut self, span: Span, translated: bool) -> ParseResult<Expression> {
        let mut cursor = span.start + 1;
        let end = span.end - 1;
        let mut text = String::new();
        let mut text_start = cursor;
        let mut segments = vec![];
        while cursor < end {
            let tail = &self.source.text[cursor..end];
            let ch = tail.chars().next().unwrap();
            if tail.starts_with("{{") || tail.starts_with("}}") {
                text.push(ch);
                cursor += 2;
                continue;
            }
            if ch == '\\' {
                let begin = cursor;
                cursor += 1;
                let escape = self.source.text[cursor..end]
                    .chars()
                    .next()
                    .ok_or_else(|| self.error("incomplete escape"))?;
                cursor += escape.len_utf8();
                text.push(match escape {
                    'n' => '\n',
                    'r' => '\r',
                    't' => '\t',
                    '\\' => '\\',
                    '\'' => '\'',
                    '"' => '"',
                    _ => {
                        return Err(Diagnostic::new(
                            Stage::Lexer,
                            "L004",
                            self.source.span(begin, cursor).unwrap(),
                            "unsupported string escape",
                        ));
                    }
                });
                continue;
            }
            if ch == '{' {
                if !text.is_empty() {
                    segments.push(Segment {
                        span: self.source.span(text_start, cursor).unwrap(),
                        kind: SegmentKind::Text(std::mem::take(&mut text)),
                    });
                }
                let close = interpolation_end(&self.source.text, cursor + 1, end, 0)
                    .ok_or_else(|| self.error("unclosed interpolation"))?;
                let tokens = lex_range(self.source, cursor + 1, close)
                    .map_err(|errors| errors.into_iter().next().unwrap())?;
                let mut parser = Parser {
                    source: self.source,
                    tokens,
                    cursor: 0,
                    depth: self.depth,
                    errors: vec![],
                };
                let expression = parser.expression(0)?;
                parser.expect(TokenKind::Eof)?;
                segments.push(Segment {
                    span: self.source.span(cursor, close + 1).unwrap(),
                    kind: SegmentKind::Value(expression),
                });
                cursor = close + 1;
                text_start = cursor;
                continue;
            }
            if ch == '}' {
                return Err(Diagnostic::new(
                    Stage::Parser,
                    "P007",
                    self.source.span(cursor, cursor + 1).unwrap(),
                    "literal closing brace must be escaped as `}}`",
                ));
            }
            text.push(ch);
            cursor += ch.len_utf8();
        }
        if !text.is_empty() {
            segments.push(Segment {
                span: self.source.span(text_start, end).unwrap(),
                kind: SegmentKind::Text(text),
            });
        }
        Ok(Expression {
            span,
            kind: ExpressionKind::Template {
                translated,
                segments,
            },
        })
    }
}
