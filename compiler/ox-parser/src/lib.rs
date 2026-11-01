use ox_ast::{Declaration, DeclarationKind, Field, Program};
use ox_diagnostics::Diagnostic;
use ox_lexer::{Token, TokenKind, RESERVED_KEYWORDS};

pub fn parse(tokens: &[Token]) -> Result<Program, Diagnostic> {
    let mut declarations = Vec::new();
    let mut cursor = 0;

    while cursor < tokens.len() {
        match &tokens[cursor].kind {
            TokenKind::Eof => break,
            TokenKind::Keyword(word) if RESERVED_KEYWORDS.contains(&word.as_str()) => {
                return Err(Diagnostic::new(
                    format!("'{word}' is reserved for a future Ox feature"),
                    tokens[cursor].span.clone(),
                ));
            }
            TokenKind::Keyword(word) if word == "fx" || word == "api" => {
                let kind = if word == "fx" {
                    DeclarationKind::Function
                } else {
                    DeclarationKind::Api
                };
                let name_token = tokens.get(cursor + 1).ok_or_else(|| {
                    Diagnostic::new("expected declaration name", tokens[cursor].span.clone())
                })?;
                let TokenKind::Identifier(name) = &name_token.kind else {
                    return Err(Diagnostic::new(
                        "expected declaration name",
                        name_token.span.clone(),
                    ));
                };
                declarations.push(Declaration {
                    kind,
                    name: name.clone(),
                    fields: Vec::new(),
                });
                cursor += 2;
            }
            TokenKind::Keyword(word) if word == "use" => {
                let name_token = tokens.get(cursor + 1).ok_or_else(|| {
                    Diagnostic::new("expected module name", tokens[cursor].span.clone())
                })?;
                let TokenKind::Identifier(name) = &name_token.kind else {
                    return Err(Diagnostic::new(
                        "expected module name",
                        name_token.span.clone(),
                    ));
                };
                declarations.push(Declaration {
                    kind: DeclarationKind::Use,
                    name: name.clone(),
                    fields: Vec::new(),
                });
                cursor += 2;
            }
            TokenKind::Keyword(word) if word == "shape" => {
                let (declaration, next) = parse_shape(tokens, cursor)?;
                declarations.push(declaration);
                cursor = next;
            }
            _ => cursor += 1,
        }
    }

    Ok(Program { declarations })
}

fn parse_shape(tokens: &[Token], start: usize) -> Result<(Declaration, usize), Diagnostic> {
    let name_token = tokens
        .get(start + 1)
        .ok_or_else(|| Diagnostic::new("expected shape name", tokens[start].span.clone()))?;
    let TokenKind::Identifier(name) = &name_token.kind else {
        return Err(Diagnostic::new(
            "expected shape name",
            name_token.span.clone(),
        ));
    };
    if !name.chars().next().is_some_and(char::is_uppercase) {
        return Err(Diagnostic::new(
            "shape names must begin with an uppercase letter",
            name_token.span.clone(),
        ));
    }

    expect_symbol(tokens, start + 2, "{")?;
    let mut cursor = start + 3;
    let mut fields = Vec::new();
    while !is_symbol(tokens.get(cursor), "}") {
        let field_token = tokens
            .get(cursor)
            .ok_or_else(|| Diagnostic::new("expected field name", name_token.span.clone()))?;
        let TokenKind::Identifier(field_name) = &field_token.kind else {
            return Err(Diagnostic::new(
                "expected field name",
                field_token.span.clone(),
            ));
        };
        expect_symbol(tokens, cursor + 1, ":")?;
        let type_token = tokens
            .get(cursor + 2)
            .ok_or_else(|| Diagnostic::new("expected field type", field_token.span.clone()))?;
        let TokenKind::Identifier(type_name) = &type_token.kind else {
            return Err(Diagnostic::new(
                "expected field type",
                type_token.span.clone(),
            ));
        };
        fields.push(Field {
            name: field_name.clone(),
            type_name: type_name.clone(),
        });
        cursor += 3;
        if is_symbol(tokens.get(cursor), ",") {
            cursor += 1;
        } else if !is_symbol(tokens.get(cursor), "}") {
            let span = tokens
                .get(cursor)
                .map_or(type_token.span.clone(), |token| token.span.clone());
            return Err(Diagnostic::new(
                "expected ',' or '}' after shape field",
                span,
            ));
        }
    }

    Ok((
        Declaration {
            kind: DeclarationKind::Shape,
            name: name.clone(),
            fields,
        },
        cursor + 1,
    ))
}

fn expect_symbol(tokens: &[Token], index: usize, expected: &str) -> Result<(), Diagnostic> {
    let token = tokens
        .get(index)
        .ok_or_else(|| Diagnostic::new(format!("expected '{expected}'"), 0..0))?;
    if is_symbol(Some(token), expected) {
        Ok(())
    } else {
        Err(Diagnostic::new(
            format!("expected '{expected}'"),
            token.span.clone(),
        ))
    }
}

fn is_symbol(token: Option<&Token>, expected: &str) -> bool {
    matches!(token.map(|token| &token.kind), Some(TokenKind::Symbol(symbol)) if symbol == expected)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ox_lexer::lex;

    #[test]
    fn extracts_top_level_declarations() {
        let tokens = lex("use ox.http; api Users {} fx main() {}").unwrap();
        let program = parse(&tokens).unwrap();
        assert_eq!(program.declarations.len(), 3);
        assert_eq!(program.declarations[2].name, "main");
    }

    #[test]
    fn parses_shape_fields() {
        let tokens = lex("shape User { id: Int, name: String, } fx main() {}").unwrap();
        let program = parse(&tokens).unwrap();
        assert_eq!(program.declarations[0].kind, DeclarationKind::Shape);
        assert_eq!(program.declarations[0].fields.len(), 2);
        assert_eq!(program.declarations[0].fields[1].name, "name");
    }
}
