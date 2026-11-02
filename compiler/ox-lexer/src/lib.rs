use ox_diagnostics::Diagnostic;
use std::ops::Range;

pub const CANONICAL_KEYWORDS: &[&str] = &[
    "fx", "val", "mut", "ret", "if", "else", "loop", "stop", "skip", "api", "use", "GET", "POST",
    "PUT", "PATCH", "DELETE", "true", "false", "shape", "choice", "pick", "case", "other", "try",
    "catch", "fail", "fails", "test", "pub", "as",
];

pub const RESERVED_KEYWORDS: &[&str] = &[
    "none", "defer", "task", "wait", "spawn", "race", "chan", "recv", "trait", "impl", "self",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TokenKind {
    Keyword(String),
    Identifier(String),
    Integer(i64),
    String(String),
    Symbol(String),
    Eof,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Range<usize>,
}

pub fn lex(source: &str) -> Result<Vec<Token>, Diagnostic> {
    let chars: Vec<(usize, char)> = source.char_indices().collect();
    let mut tokens = Vec::new();
    let mut cursor = 0;

    while cursor < chars.len() {
        let (start, ch) = chars[cursor];
        if ch.is_whitespace() {
            cursor += 1;
            continue;
        }
        if ch == '/' && chars.get(cursor + 1).is_some_and(|(_, next)| *next == '/') {
            cursor += 2;
            while cursor < chars.len() && chars[cursor].1 != '\n' {
                cursor += 1;
            }
            continue;
        }
        if ch.is_alphabetic() || ch == '_' {
            cursor += 1;
            while cursor < chars.len()
                && (chars[cursor].1.is_alphanumeric() || chars[cursor].1 == '_')
            {
                cursor += 1;
            }
            let end = chars
                .get(cursor)
                .map_or(source.len(), |(offset, _)| *offset);
            let word = &source[start..end];
            let kind = if CANONICAL_KEYWORDS.contains(&word) || RESERVED_KEYWORDS.contains(&word) {
                TokenKind::Keyword(word.to_owned())
            } else {
                TokenKind::Identifier(word.to_owned())
            };
            tokens.push(Token {
                kind,
                span: start..end,
            });
            continue;
        }
        if ch.is_ascii_digit() {
            cursor += 1;
            while cursor < chars.len() && chars[cursor].1.is_ascii_digit() {
                cursor += 1;
            }
            let end = chars
                .get(cursor)
                .map_or(source.len(), |(offset, _)| *offset);
            let value = source[start..end]
                .parse()
                .map_err(|_| Diagnostic::new("integer literal is out of range", start..end))?;
            tokens.push(Token {
                kind: TokenKind::Integer(value),
                span: start..end,
            });
            continue;
        }
        if ch == '"' {
            cursor += 1;
            let content_start = chars
                .get(cursor)
                .map_or(source.len(), |(offset, _)| *offset);
            while cursor < chars.len() && chars[cursor].1 != '"' {
                if chars[cursor].1 == '\n' {
                    return Err(Diagnostic::new(
                        "unterminated string literal",
                        start..chars[cursor].0,
                    ));
                }
                cursor += 1;
            }
            if cursor == chars.len() {
                return Err(Diagnostic::new(
                    "unterminated string literal",
                    start..source.len(),
                ));
            }
            let content_end = chars[cursor].0;
            cursor += 1;
            let end = chars
                .get(cursor)
                .map_or(source.len(), |(offset, _)| *offset);
            tokens.push(Token {
                kind: TokenKind::String(source[content_start..content_end].to_owned()),
                span: start..end,
            });
            continue;
        }

        let next = chars.get(cursor + 1).map(|(_, value)| *value);
        let pair = next.map(|next| format!("{ch}{next}"));
        if let Some(symbol) = pair
            .filter(|value| ["->", "==", "!=", "<=", ">=", "&&", "||"].contains(&value.as_str()))
        {
            cursor += 2;
            let end = chars
                .get(cursor)
                .map_or(source.len(), |(offset, _)| *offset);
            tokens.push(Token {
                kind: TokenKind::Symbol(symbol),
                span: start..end,
            });
            continue;
        }
        if "(){}:,;+-*/%!<>=.".contains(ch) {
            cursor += 1;
            let end = chars
                .get(cursor)
                .map_or(source.len(), |(offset, _)| *offset);
            tokens.push(Token {
                kind: TokenKind::Symbol(ch.to_string()),
                span: start..end,
            });
            continue;
        }
        let end = start + ch.len_utf8();
        return Err(Diagnostic::new(
            format!("unexpected character {ch:?}"),
            start..end,
        ));
    }

    tokens.push(Token {
        kind: TokenKind::Eof,
        span: source.len()..source.len(),
    });
    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_canonical_and_reserved_words() {
        let tokens = lex("fx main() { val n = 42; } shape").unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Keyword("fx".into()));
        assert_eq!(tokens[8].kind, TokenKind::Integer(42));
        assert!(tokens
            .iter()
            .any(|token| token.kind == TokenKind::Keyword("shape".into())));
    }
}
