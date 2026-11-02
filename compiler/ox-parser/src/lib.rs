use ox_ast::{
    Block, Declaration, DeclarationKind, Expression, Field, Parameter, PickCase, Program, Route,
    Statement, Variant,
};
use ox_diagnostics::Diagnostic;
use ox_lexer::{Token, TokenKind, RESERVED_KEYWORDS};

pub fn parse(tokens: &[Token]) -> Result<Program, Diagnostic> {
    Parser { tokens, cursor: 0 }.program()
}

struct Parser<'a> {
    tokens: &'a [Token],
    cursor: usize,
}

impl Parser<'_> {
    fn program(&mut self) -> Result<Program, Diagnostic> {
        let mut declarations = Vec::new();
        while !self.eof() {
            if let Some(word) = self.keyword() {
                if RESERVED_KEYWORDS.contains(&word) {
                    return self.error(format!("'{word}' is reserved for a future Ox feature"));
                }
            }
            declarations.push(match self.keyword() {
                Some("use") => self.parse_use()?,
                Some("shape") => self.parse_shape()?,
                Some("choice") => self.parse_choice()?,
                Some("fx") => self.parse_function()?,
                Some("api") => self.parse_api()?,
                Some("test") => self.parse_test()?,
                _ => return self.error("expected a top-level declaration"),
            });
        }
        Ok(Program { declarations })
    }

    fn declaration(&self, kind: DeclarationKind, name: String) -> Declaration {
        Declaration {
            kind,
            name,
            fields: vec![],
            variants: vec![],
            params: vec![],
            return_type: None,
            body: None,
            routes: vec![],
        }
    }

    fn parse_use(&mut self) -> Result<Declaration, Diagnostic> {
        self.expect_keyword("use")?;
        let name = self.identifier("expected module name")?;
        while self.match_symbol(".") {
            self.identifier("expected module path component")?;
        }
        self.expect_symbol(";")?;
        Ok(self.declaration(DeclarationKind::Use, name))
    }

    fn parse_shape(&mut self) -> Result<Declaration, Diagnostic> {
        self.expect_keyword("shape")?;
        let name = self.type_identifier("shape")?;
        self.expect_symbol("{")?;
        let mut declaration = self.declaration(DeclarationKind::Shape, name);
        while !self.match_symbol("}") {
            let name = self.identifier("expected field name")?;
            self.expect_symbol(":")?;
            let type_name = self.identifier("expected field type")?;
            declaration.fields.push(Field { name, type_name });
            if !self.match_symbol(",") {
                self.expect_symbol("}")?;
                break;
            }
        }
        Ok(declaration)
    }

    fn parse_choice(&mut self) -> Result<Declaration, Diagnostic> {
        self.expect_keyword("choice")?;
        let name = self.type_identifier("choice")?;
        self.expect_symbol("{")?;
        let mut declaration = self.declaration(DeclarationKind::Choice, name);
        while !self.match_symbol("}") {
            let name = self.type_identifier("variant")?;
            let payload_type = if self.match_symbol("(") {
                let value = self.identifier("expected variant payload type")?;
                self.expect_symbol(")")?;
                Some(value)
            } else {
                None
            };
            declaration.variants.push(Variant { name, payload_type });
            if !self.match_symbol(",") {
                self.expect_symbol("}")?;
                break;
            }
        }
        Ok(declaration)
    }

    fn parse_function(&mut self) -> Result<Declaration, Diagnostic> {
        self.expect_keyword("fx")?;
        let name = self.identifier("expected function name")?;
        let mut declaration = self.declaration(DeclarationKind::Function, name);
        declaration.params = self.parameters()?;
        declaration.return_type = Some(if self.match_symbol(":") {
            self.identifier("expected return type")?
        } else {
            "Unit".into()
        });
        declaration.body = Some(self.block()?);
        Ok(declaration)
    }

    fn parse_api(&mut self) -> Result<Declaration, Diagnostic> {
        self.expect_keyword("api")?;
        let name = self.identifier("expected API name")?;
        let mut declaration = self.declaration(DeclarationKind::Api, name);
        self.expect_symbol("{")?;
        while !self.match_symbol("}") {
            let method = match self.keyword() {
                Some(value @ ("GET" | "POST" | "PUT" | "PATCH" | "DELETE")) => {
                    let owned = value.to_owned();
                    self.cursor += 1;
                    owned
                }
                _ => return self.error("expected HTTP method"),
            };
            let path = self.string("expected route path")?;
            let params = self.parameters()?;
            let return_type = if self.match_symbol(":") {
                self.identifier("expected route return type")?
            } else {
                "Unit".into()
            };
            let body = self.block()?;
            declaration.routes.push(Route {
                method,
                path,
                params,
                return_type,
                body,
            });
        }
        Ok(declaration)
    }

    fn parse_test(&mut self) -> Result<Declaration, Diagnostic> {
        self.expect_keyword("test")?;
        let name = self.string("expected test name")?;
        let mut declaration = self.declaration(DeclarationKind::Test, name);
        declaration.return_type = Some("Unit".into());
        declaration.body = Some(self.block()?);
        Ok(declaration)
    }

    fn parameters(&mut self) -> Result<Vec<Parameter>, Diagnostic> {
        self.expect_symbol("(")?;
        let mut params = vec![];
        if !self.match_symbol(")") {
            loop {
                let name = self.identifier("expected parameter name")?;
                self.expect_symbol(":")?;
                let type_name = self.identifier("expected parameter type")?;
                params.push(Parameter { name, type_name });
                if !self.match_symbol(",") {
                    self.expect_symbol(")")?;
                    break;
                }
            }
        }
        Ok(params)
    }

    fn block(&mut self) -> Result<Block, Diagnostic> {
        self.expect_symbol("{")?;
        let mut statements = vec![];
        let mut tail = None;
        while !self.match_symbol("}") {
            if self.eof() {
                return self.error("expected '}' after block");
            }
            if matches!(self.keyword(), Some("val" | "mut")) {
                let mutable = self.keyword() == Some("mut");
                self.cursor += 1;
                let name = self.identifier("expected variable name")?;
                let type_name = if self.match_symbol(":") {
                    Some(self.identifier("expected type")?)
                } else {
                    None
                };
                self.expect_symbol("=")?;
                let value = self.expression()?;
                self.expect_symbol(";")?;
                statements.push(Statement::Let {
                    name,
                    mutable,
                    type_name,
                    value,
                });
                continue;
            }
            if self.keyword() == Some("loop") {
                self.cursor += 1;
                let condition = self.expression()?;
                let body = self.block()?;
                statements.push(Statement::Loop { condition, body });
                continue;
            }
            if matches!(self.keyword(), Some("stop" | "skip")) {
                let stop = self.keyword() == Some("stop");
                self.cursor += 1;
                self.expect_symbol(";")?;
                statements.push(if stop {
                    Statement::Break
                } else {
                    Statement::Continue
                });
                continue;
            }
            if matches!(self.keyword(), Some("ret" | "fail")) {
                let fail = self.keyword() == Some("fail");
                self.cursor += 1;
                let value = self.expression()?;
                self.expect_symbol(";")?;
                statements.push(if fail {
                    Statement::Fail(value)
                } else {
                    Statement::Return(value)
                });
                continue;
            }
            if self.is_identifier() && self.next_symbol("=") {
                let name = self.identifier("expected assignment target")?;
                self.expect_symbol("=")?;
                let value = self.expression()?;
                self.expect_symbol(";")?;
                statements.push(Statement::Assign { name, value });
                continue;
            }
            let value = self.expression()?;
            if self.match_symbol(";") {
                statements.push(Statement::Expression(value));
            } else {
                tail = Some(Box::new(value));
                self.expect_symbol("}")?;
                break;
            }
        }
        Ok(Block { statements, tail })
    }

    fn expression(&mut self) -> Result<Expression, Diagnostic> {
        self.binary(1)
    }
    fn binary(&mut self, minimum: u8) -> Result<Expression, Diagnostic> {
        let mut left = self.unary()?;
        while let Some((operator, precedence)) = self.binary_operator() {
            if precedence < minimum {
                break;
            }
            self.cursor += 1;
            let right = self.binary(precedence + 1)?;
            left = Expression::Binary {
                left: Box::new(left),
                operator,
                right: Box::new(right),
            };
        }
        Ok(left)
    }
    fn unary(&mut self) -> Result<Expression, Diagnostic> {
        if matches!(self.symbol(), Some("!" | "-")) {
            let operator = self.symbol().unwrap().to_owned();
            self.cursor += 1;
            return Ok(Expression::Unary {
                operator,
                value: Box::new(self.unary()?),
            });
        }
        self.postfix()
    }
    fn postfix(&mut self) -> Result<Expression, Diagnostic> {
        let mut value = self.primary()?;
        loop {
            if self.match_symbol("(") {
                let mut args = vec![];
                if !self.match_symbol(")") {
                    loop {
                        args.push(self.expression()?);
                        if !self.match_symbol(",") {
                            self.expect_symbol(")")?;
                            break;
                        }
                    }
                }
                value = Expression::Call {
                    callee: Box::new(value),
                    args,
                };
            } else if self.match_symbol(".") {
                let field = self.identifier("expected field name")?;
                value = Expression::Member {
                    object: Box::new(value),
                    field,
                };
            } else if self.symbol() == Some("{") {
                if let Expression::Identifier(name) = value {
                    if name.chars().next().is_some_and(char::is_uppercase) {
                        self.cursor += 1;
                        value = Expression::Construct {
                            name,
                            fields: self.properties()?,
                        };
                        continue;
                    }
                    value = Expression::Identifier(name);
                }
                break;
            } else {
                break;
            }
        }
        Ok(value)
    }
    fn primary(&mut self) -> Result<Expression, Diagnostic> {
        match self.current().map(|token| &token.kind) {
            Some(TokenKind::Integer(value)) => {
                let value = *value;
                self.cursor += 1;
                Ok(Expression::Integer(value))
            }
            Some(TokenKind::String(value)) => {
                let value = value.clone();
                self.cursor += 1;
                Ok(Expression::String(value))
            }
            Some(TokenKind::Identifier(value)) => {
                let value = value.clone();
                self.cursor += 1;
                Ok(Expression::Identifier(value))
            }
            Some(TokenKind::Keyword(value)) if value == "true" || value == "false" => {
                let value = value == "true";
                self.cursor += 1;
                Ok(Expression::Boolean(value))
            }
            Some(TokenKind::Keyword(value)) if value == "if" => self.if_expression(),
            Some(TokenKind::Keyword(value)) if value == "pick" => self.pick_expression(),
            Some(TokenKind::Keyword(value)) if value == "try" => self.try_expression(),
            Some(TokenKind::Keyword(value)) if RESERVED_KEYWORDS.contains(&value.as_str()) => {
                self.error(format!("'{value}' is reserved for a future Ox feature"))
            }
            Some(TokenKind::Symbol(value)) if value == "(" => {
                self.cursor += 1;
                let value = self.expression()?;
                self.expect_symbol(")")?;
                Ok(value)
            }
            Some(TokenKind::Symbol(value)) if value == "{" => {
                self.cursor += 1;
                Ok(Expression::Object(self.properties()?))
            }
            _ => self.error("expected expression"),
        }
    }
    fn properties(&mut self) -> Result<Vec<(String, Expression)>, Diagnostic> {
        let mut values = vec![];
        if self.match_symbol("}") {
            return Ok(values);
        }
        loop {
            let name = self.identifier("expected property name")?;
            self.expect_symbol(":")?;
            values.push((name, self.expression()?));
            if !self.match_symbol(",") {
                self.expect_symbol("}")?;
                break;
            }
        }
        Ok(values)
    }
    fn if_expression(&mut self) -> Result<Expression, Diagnostic> {
        self.expect_keyword("if")?;
        let condition = self.expression()?;
        let then_branch = self.block()?;
        self.expect_keyword("else")?;
        let else_branch = self.block()?;
        Ok(Expression::If {
            condition: Box::new(condition),
            then_branch,
            else_branch,
        })
    }
    fn pick_expression(&mut self) -> Result<Expression, Diagnostic> {
        self.expect_keyword("pick")?;
        let value = self.expression()?;
        self.expect_symbol("{")?;
        let mut cases = vec![];
        while self.keyword() == Some("case") {
            self.cursor += 1;
            let variant = self.type_identifier("variant")?;
            let binding = if self.match_symbol("(") {
                let value = self.identifier("expected binding")?;
                self.expect_symbol(")")?;
                Some(value)
            } else {
                None
            };
            cases.push(PickCase {
                variant,
                binding,
                body: self.block()?,
            });
        }
        let other = if self.keyword() == Some("other") {
            self.cursor += 1;
            Some(self.block()?)
        } else {
            None
        };
        self.expect_symbol("}")?;
        Ok(Expression::Pick {
            value: Box::new(value),
            cases,
            other,
        })
    }
    fn try_expression(&mut self) -> Result<Expression, Diagnostic> {
        self.expect_keyword("try")?;
        let value = self.expression()?;
        self.expect_keyword("catch")?;
        let binding = self.identifier("expected failure binding")?;
        let handler = self.block()?;
        Ok(Expression::Try {
            value: Box::new(value),
            binding,
            handler,
        })
    }
    fn binary_operator(&self) -> Option<(String, u8)> {
        let value = self.symbol()?;
        let precedence = match value {
            "||" => 1,
            "&&" => 2,
            "==" | "!=" => 3,
            "<" | "<=" | ">" | ">=" => 4,
            "+" | "-" => 5,
            "*" | "/" | "%" => 6,
            _ => return None,
        };
        Some((value.to_owned(), precedence))
    }
    fn current(&self) -> Option<&Token> {
        self.tokens.get(self.cursor)
    }
    fn eof(&self) -> bool {
        matches!(
            self.current().map(|token| &token.kind),
            Some(TokenKind::Eof) | None
        )
    }
    fn keyword(&self) -> Option<&str> {
        match self.current().map(|token| &token.kind) {
            Some(TokenKind::Keyword(value)) => Some(value),
            _ => None,
        }
    }
    fn symbol(&self) -> Option<&str> {
        match self.current().map(|token| &token.kind) {
            Some(TokenKind::Symbol(value)) => Some(value),
            _ => None,
        }
    }
    fn is_identifier(&self) -> bool {
        matches!(
            self.current().map(|token| &token.kind),
            Some(TokenKind::Identifier(_))
        )
    }
    fn next_symbol(&self, expected: &str) -> bool {
        matches!(self.tokens.get(self.cursor + 1).map(|token| &token.kind), Some(TokenKind::Symbol(value)) if value == expected)
    }
    fn match_symbol(&mut self, expected: &str) -> bool {
        if self.symbol() == Some(expected) {
            self.cursor += 1;
            true
        } else {
            false
        }
    }
    fn expect_symbol(&mut self, expected: &str) -> Result<(), Diagnostic> {
        if self.match_symbol(expected) {
            Ok(())
        } else {
            self.error(format!("expected '{expected}'"))
        }
    }
    fn expect_keyword(&mut self, expected: &str) -> Result<(), Diagnostic> {
        if self.keyword() == Some(expected) {
            self.cursor += 1;
            Ok(())
        } else {
            self.error(format!("expected '{expected}'"))
        }
    }
    fn identifier(&mut self, message: &str) -> Result<String, Diagnostic> {
        match self.current().map(|token| &token.kind) {
            Some(TokenKind::Identifier(value)) => {
                let value = value.clone();
                self.cursor += 1;
                Ok(value)
            }
            _ => self.error(message),
        }
    }
    fn string(&mut self, message: &str) -> Result<String, Diagnostic> {
        match self.current().map(|token| &token.kind) {
            Some(TokenKind::String(value)) => {
                let value = value.clone();
                self.cursor += 1;
                Ok(value)
            }
            _ => self.error(message),
        }
    }
    fn type_identifier(&mut self, kind: &str) -> Result<String, Diagnostic> {
        let value = self.identifier(&format!("expected {kind} name"))?;
        if value.chars().next().is_some_and(char::is_uppercase) {
            Ok(value)
        } else {
            self.error(format!("{kind} names must begin with an uppercase letter"))
        }
    }
    fn error<T>(&self, message: impl Into<String>) -> Result<T, Diagnostic> {
        let span = self.current().map_or(0..0, |token| token.span.clone());
        Err(Diagnostic::new(message, span))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ox_lexer::lex;
    #[test]
    fn parses_complete_function_and_api_bodies() {
        let source = r#"shape User { id: Int, } choice Result { Ok(User), Missing, }
api Users { GET "/users/:id"(id: Int): User { User { id: id } } }
fx label(result: Result): String { try pick result { case Ok(user) { "ok" } case Missing { fail { message: "missing" }; } } catch problem { "failed" } }
fx main() { val n: Int = 1 + 2 * 3; if n > 0 { println("ok"); } else {}; }"#;
        let program = parse(&lex(source).unwrap()).unwrap();
        assert_eq!(program.declarations.len(), 5);
        assert_eq!(program.declarations[2].routes.len(), 1);
        assert!(program.declarations[3].body.is_some());
    }
}
