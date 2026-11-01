use ox_ast::{Declaration, DeclarationKind, Program};
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
                });
                cursor += 2;
            }
            _ => cursor += 1,
        }
    }

    Ok(Program { declarations })
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
}
