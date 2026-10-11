//! Tokenization over whole files or interpolation ranges in the same snapshot.
use super::token::{Token, TokenKind};
use crate::ui_language::{
    diagnostic::report::{Diagnostic, Stage},
    source::snapshot::Source,
};

pub const MAX_SOURCE_BYTES: usize = 1024 * 1024;
pub const MAX_NESTING: usize = 128;
pub const MAX_DIAGNOSTICS: usize = 32;

pub fn lex(source: &Source) -> Result<Vec<Token>, Vec<Diagnostic>> {
    if source.text.len() > MAX_SOURCE_BYTES {
        return Err(vec![Diagnostic::new(
            Stage::Lexer,
            "L001",
            source.span(0, 0).unwrap(),
            "source exceeds 1 MiB limit",
        )]);
    }
    lex_range(source, 0, source.text.len())
}

pub(crate) fn lex_range(
    source: &Source,
    start: usize,
    end: usize,
) -> Result<Vec<Token>, Vec<Diagnostic>> {
    let mut tokens = Vec::new();
    let mut errors = Vec::new();
    let mut cursor = start;
    while cursor < end && errors.len() < MAX_DIAGNOSTICS {
        let tail = &source.text[cursor..end];
        let ch = tail.chars().next().unwrap();
        if ch.is_whitespace() {
            cursor += ch.len_utf8();
            continue;
        }
        if tail.starts_with("//") {
            cursor += tail.find('\n').unwrap_or(tail.len());
            continue;
        }
        let begin = cursor;
        let kind = if ch.is_ascii_alphabetic() || ch == '_' {
            cursor += 1;
            while cursor < end
                && (source.text.as_bytes()[cursor].is_ascii_alphanumeric()
                    || source.text.as_bytes()[cursor] == b'_')
            {
                cursor += 1;
            }
            TokenKind::Name(source.text[begin..cursor].to_owned())
        } else if ch.is_ascii_digit()
            || (ch == '-' && tail.as_bytes().get(1).is_some_and(u8::is_ascii_digit))
        {
            cursor += 1;
            while cursor < end && source.text.as_bytes()[cursor].is_ascii_digit() {
                cursor += 1;
            }
            if cursor + 1 < end
                && source.text.as_bytes()[cursor] == b'.'
                && source.text.as_bytes()[cursor + 1].is_ascii_digit()
            {
                cursor += 1;
                while cursor < end && source.text.as_bytes()[cursor].is_ascii_digit() {
                    cursor += 1;
                }
            }
            TokenKind::Number(source.text[begin..cursor].to_owned())
        } else if ch == '\'' || ch == '"' {
            match quoted_end(&source.text, cursor, end, 0) {
                Some(next) => {
                    cursor = next;
                    TokenKind::String {
                        translated: ch == '"',
                        value: source.text[begin + 1..cursor - 1].to_owned(),
                    }
                }
                None => {
                    errors.push(Diagnostic::new(
                        Stage::Lexer,
                        "L002",
                        source.span(begin, end).unwrap(),
                        "unterminated string or interpolation, or nesting limit exceeded",
                    ));
                    cursor = end;
                    continue;
                }
            }
        } else {
            let pair = if tail.starts_with("=>") {
                Some(TokenKind::Arrow)
            } else if tail.starts_with("==") {
                Some(TokenKind::EqualEqual)
            } else if tail.starts_with("!=") {
                Some(TokenKind::NotEqual)
            } else if tail.starts_with("&&") {
                Some(TokenKind::And)
            } else if tail.starts_with("||") {
                Some(TokenKind::Or)
            } else {
                None
            };
            if let Some(kind) = pair {
                cursor += 2;
                kind
            } else {
                cursor += ch.len_utf8();
                match ch {
                    '<' => TokenKind::Less,
                    '>' => TokenKind::Greater,
                    '/' => TokenKind::Slash,
                    '=' => TokenKind::Equal,
                    '!' => TokenKind::Bang,
                    '.' => TokenKind::Dot,
                    ',' => TokenKind::Comma,
                    ';' => TokenKind::Semicolon,
                    ':' => TokenKind::Colon,
                    '{' => TokenKind::LeftBrace,
                    '}' => TokenKind::RightBrace,
                    '(' => TokenKind::LeftParen,
                    ')' => TokenKind::RightParen,
                    _ => {
                        errors.push(Diagnostic::new(
                            Stage::Lexer,
                            "L003",
                            source.span(begin, cursor).unwrap(),
                            format!("unexpected character `{ch}`"),
                        ));
                        continue;
                    }
                }
            }
        };
        tokens.push(Token {
            kind,
            span: source.span(begin, cursor).unwrap(),
        });
    }
    tokens.push(Token {
        kind: TokenKind::Eof,
        span: source.span(end, end).unwrap(),
    });
    if errors.is_empty() {
        Ok(tokens)
    } else {
        Err(errors)
    }
}

/// Finds the close of a template, skipping quotes inside interpolation.
fn quoted_end(text: &str, start: usize, end: usize, depth: usize) -> Option<usize> {
    if depth >= MAX_NESTING {
        return None;
    }
    let quote = text.as_bytes()[start];
    let mut cursor = start + 1;
    while cursor < end {
        match text.as_bytes()[cursor] {
            b'\\' => {
                cursor += 1;
                cursor += text[cursor..end].chars().next()?.len_utf8();
            }
            ch if ch == quote => return Some(cursor + 1),
            b'{' if text.as_bytes().get(cursor + 1) == Some(&b'{') => cursor += 2,
            b'{' => cursor = interpolation_end(text, cursor + 1, end, depth + 1)? + 1,
            _ => cursor += text[cursor..end].chars().next()?.len_utf8(),
        }
    }
    None
}

pub(crate) fn interpolation_end(
    text: &str,
    start: usize,
    end: usize,
    depth: usize,
) -> Option<usize> {
    if depth >= MAX_NESTING {
        return None;
    }
    let mut cursor = start;
    while cursor < end {
        match text.as_bytes()[cursor] {
            b'\'' | b'"' => cursor = quoted_end(text, cursor, end, depth + 1)?,
            b'}' => return Some(cursor),
            b'{' => cursor = interpolation_end(text, cursor + 1, end, depth + 1)? + 1,
            b'/' if text.as_bytes().get(cursor + 1) == Some(&b'/') => {
                cursor += text[cursor..end].find('\n').unwrap_or(end - cursor)
            }
            _ => cursor += text[cursor..end].chars().next()?.len_utf8(),
        }
    }
    None
}
