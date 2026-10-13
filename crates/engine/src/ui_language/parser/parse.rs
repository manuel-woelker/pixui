//! Recursive-descent markup with Pratt expressions and bounded child recovery.
use crate::ui_language::{
    ast::{
        definition::Definition,
        expression::Name,
        part::{Arm, Attribute, Part, PartKind},
    },
    diagnostic::report::{Diagnostic, Stage},
    lexer::{
        scan::{MAX_DIAGNOSTICS, MAX_NESTING, lex},
        token::{Token, TokenKind},
    },
    source::snapshot::Source,
};

pub(super) type ParseResult<T> = Result<T, Diagnostic>;
pub fn parse(source: &Source) -> Result<Definition, Vec<Diagnostic>> {
    let tokens = lex(source)?;
    let mut parser = Parser {
        source,
        tokens,
        cursor: 0,
        depth: 0,
        errors: vec![],
    };
    match parser.definition() {
        Ok(definition) if parser.errors.is_empty() => Ok(definition),
        Ok(_) => Err(parser.errors),
        Err(error) => {
            if parser.errors.len() < MAX_DIAGNOSTICS {
                parser.errors.push(error);
            }
            Err(parser.errors)
        }
    }
}
pub(super) struct Parser<'a> {
    pub(super) source: &'a Source,
    pub(super) tokens: Vec<Token>,
    pub(super) cursor: usize,
    pub(super) depth: usize,
    pub(super) errors: Vec<Diagnostic>,
}
impl Parser<'_> {
    pub(super) fn token(&self) -> &Token {
        &self.tokens[self.cursor]
    }
    pub(super) fn take(&mut self) -> Token {
        let token = self.token().clone();
        if token.kind != TokenKind::Eof {
            self.cursor += 1;
        }
        token
    }
    pub(super) fn is(&self, kind: &TokenKind) -> bool {
        std::mem::discriminant(&self.token().kind) == std::mem::discriminant(kind)
    }
    pub(super) fn consume(&mut self, kind: &TokenKind) -> Option<Token> {
        self.is(kind).then(|| self.take())
    }
    pub(super) fn error(&self, message: impl Into<String>) -> Diagnostic {
        Diagnostic::new(Stage::Parser, "P001", self.token().span, message)
    }
    pub(super) fn expect(&mut self, kind: TokenKind) -> ParseResult<Token> {
        self.consume(&kind)
            .ok_or_else(|| self.error(format!("expected {kind:?}")))
    }
    pub(super) fn name(&mut self) -> ParseResult<Name> {
        match self.token().kind.clone() {
            TokenKind::Name(value) => Ok(Name {
                value,
                span: self.take().span,
            }),
            _ => Err(self.error("expected identifier")),
        }
    }
    fn keyword(&mut self, word: &str) -> ParseResult<Name> {
        let name = self.name()?;
        if name.value == word {
            Ok(name)
        } else {
            Err(Diagnostic::new(
                Stage::Parser,
                "P002",
                name.span,
                format!("expected `{word}`"),
            ))
        }
    }
    pub(super) fn enter(&mut self) -> ParseResult<()> {
        if self.depth >= MAX_NESTING {
            Err(self.error("nesting exceeds limit"))
        } else {
            self.depth += 1;
            Ok(())
        }
    }
    fn definition(&mut self) -> ParseResult<Definition> {
        let start = self.keyword("ui")?.span;
        let name = self.name()?;
        let domain = if matches!(&self.token().kind,TokenKind::Name(name) if name=="domain") {
            self.take();
            let token = self.take();
            match token.kind {
                TokenKind::String {
                    translated: false,
                    value,
                } if !value.contains(['{', '}', '\\']) => Some(Name {
                    value,
                    span: token.span,
                }),
                _ => {
                    return Err(Diagnostic::new(
                        Stage::Parser,
                        "P003",
                        token.span,
                        "domain must be a single-quoted literal without escapes or interpolation",
                    ));
                }
            }
        } else {
            None
        };
        self.expect(TokenKind::LeftBrace)?;
        let root = self.part()?;
        let end = self.expect(TokenKind::RightBrace)?.span;
        self.expect(TokenKind::Eof)?;
        Ok(Definition {
            name,
            domain,
            root,
            span: start.join(end),
        })
    }
    fn part(&mut self) -> ParseResult<Part> {
        self.enter()?;
        let result = if self.is(&TokenKind::Less) {
            self.element()
        } else {
            self.control()
        };
        self.depth -= 1;
        result
    }
    fn element(&mut self) -> ParseResult<Part> {
        let start = self.expect(TokenKind::Less)?.span;
        let name = if self.is(&TokenKind::Greater) {
            None
        } else {
            Some(self.name()?)
        };
        let mut attributes = vec![];
        let mut previous = name.as_ref().map_or(start.end, |name| name.span.end);
        while !self.is(&TokenKind::Greater) && !self.is(&TokenKind::Slash) {
            if self.token().span.start == previous {
                return Err(self.error("attributes require separating whitespace or a comment"));
            }
            let at = self.consume(&TokenKind::At);
            let mut name = self.name()?;
            if let Some(at) = at {
                name.value.insert(0, '@');
                name.span = at.span.join(name.span);
            }
            self.expect(TokenKind::Equal)?;
            let value = self.expression(0)?;
            previous = value.span.end;
            let span = name.span.join(value.span);
            attributes.push(Attribute { name, value, span });
        }
        if self.consume(&TokenKind::Slash).is_some() {
            let end = self.expect(TokenKind::Greater)?.span;
            return Ok(Part {
                kind: PartKind::Element {
                    name,
                    attributes,
                    children: vec![],
                },
                span: start.join(end),
            });
        }
        self.expect(TokenKind::Greater)?;
        let mut children = vec![];
        while !(self.is(&TokenKind::Less)
            && self
                .tokens
                .get(self.cursor + 1)
                .is_some_and(|token| token.kind == TokenKind::Slash))
        {
            if self.is(&TokenKind::Eof) {
                return Err(self.error("expected closing tag"));
            }
            let before = self.cursor;
            match self.part() {
                Ok(part) => children.push(part),
                Err(error) => {
                    self.errors.push(error);
                    if self.errors.len() >= MAX_DIAGNOSTICS {
                        return Err(self.error("too many errors"));
                    }
                    if self.cursor == before {
                        self.take();
                    }
                    while !matches!(
                        self.token().kind,
                        TokenKind::Less | TokenKind::LeftBrace | TokenKind::Eof
                    ) {
                        self.take();
                    }
                }
            }
        }
        self.expect(TokenKind::Less)?;
        self.expect(TokenKind::Slash)?;
        if let Some(open) = &name {
            let close = self.name()?;
            if close.value != open.value {
                return Err(Diagnostic::new(
                    Stage::Parser,
                    "P004",
                    close.span,
                    "closing tag does not match opening tag",
                )
                .secondary(open.span, "opened here"));
            }
        }
        let end = self.expect(TokenKind::Greater)?.span;
        Ok(Part {
            kind: PartKind::Element {
                name,
                attributes,
                children,
            },
            span: start.join(end),
        })
    }
    fn control(&mut self) -> ParseResult<Part> {
        let start = self.expect(TokenKind::LeftBrace)?.span;
        let keyword = self.name()?;
        let kind = match keyword.value.as_str() {
            "for" => {
                let binding = self.name()?;
                self.keyword("in")?;
                let sequence = self.expression(0)?;
                self.expect(TokenKind::Arrow)?;
                let body = Box::new(self.part()?);
                PartKind::For {
                    binding,
                    sequence,
                    body,
                }
            }
            "match" => {
                let value = self.expression(0)?;
                self.expect(TokenKind::Semicolon)?;
                let mut arms = vec![];
                while !self.is(&TokenKind::RightBrace) {
                    let start = self.token().span;
                    let pattern = if matches!(&self.token().kind,TokenKind::Name(name) if name=="_")
                    {
                        self.take();
                        None
                    } else {
                        Some(self.expression(0)?)
                    };
                    self.expect(TokenKind::Arrow)?;
                    let body = self.part()?;
                    let end = self.expect(TokenKind::Comma)?.span;
                    arms.push(Arm {
                        pattern,
                        body,
                        span: start.join(end),
                    });
                }
                PartKind::Match { value, arms }
            }
            _ => {
                return Err(Diagnostic::new(
                    Stage::Parser,
                    "P005",
                    keyword.span,
                    "expected `for` or `match`",
                ));
            }
        };
        let end = self.expect(TokenKind::RightBrace)?.span;
        Ok(Part {
            kind,
            span: start.join(end),
        })
    }
}
