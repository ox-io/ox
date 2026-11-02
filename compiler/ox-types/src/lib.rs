use ox_ast::{Block, DeclarationKind, Expression, Program, Statement};
use ox_diagnostics::Diagnostic;
use std::collections::{HashMap, HashSet};

pub fn check(program: &Program) -> Result<(), Diagnostic> {
    check_nominal_types(program)?;
    check_entrypoint(program)?;
    check_bodies(program)
}

fn check_bodies(program: &Program) -> Result<(), Diagnostic> {
    let shapes: HashMap<_, _> = program
        .declarations
        .iter()
        .filter(|d| d.kind == DeclarationKind::Shape)
        .map(|d| (d.name.as_str(), d))
        .collect();
    let choices: HashMap<_, _> = program
        .declarations
        .iter()
        .filter(|d| d.kind == DeclarationKind::Choice)
        .map(|d| (d.name.as_str(), d))
        .collect();
    let functions: HashMap<_, _> = program
        .declarations
        .iter()
        .filter(|d| d.kind == DeclarationKind::Function)
        .map(|d| (d.name.as_str(), d))
        .collect();
    let mut variants = HashMap::new();
    for choice in choices.values() {
        for variant in &choice.variants {
            variants.insert(
                variant.name.as_str(),
                (choice.name.as_str(), variant.payload_type.as_deref()),
            );
        }
    }
    let context = Context {
        shapes,
        choices,
        functions,
        variants,
    };
    for function in context.functions.values() {
        let mut environment: HashMap<String, (String, bool)> = function
            .params
            .iter()
            .map(|p| (p.name.clone(), (p.type_name.clone(), false)))
            .collect();
        let expected = function.return_type.as_deref().unwrap_or("Unit");
        let actual = check_block(
            function.body.as_ref().unwrap(),
            &mut environment,
            expected,
            &context,
            0,
        )?;
        if actual != expected && actual != "Never" {
            return diagnostic(format!(
                "function '{}' returns {actual}, but declares {expected}",
                function.name
            ));
        }
    }
    for test in program
        .declarations
        .iter()
        .filter(|d| d.kind == DeclarationKind::Test)
    {
        check_block(
            test.body.as_ref().unwrap(),
            &mut HashMap::new(),
            "Unit",
            &context,
            0,
        )?;
    }
    for api in program
        .declarations
        .iter()
        .filter(|d| d.kind == DeclarationKind::Api)
    {
        for route in &api.routes {
            let mut environment = route
                .params
                .iter()
                .map(|p| (p.name.clone(), (p.type_name.clone(), false)))
                .collect();
            let actual = check_block(
                &route.body,
                &mut environment,
                &route.return_type,
                &context,
                0,
            )?;
            if actual != route.return_type && actual != "Never" {
                return diagnostic(format!(
                    "route '{} {}' returns {actual}, but declares {}",
                    route.method, route.path, route.return_type
                ));
            }
        }
    }
    Ok(())
}

struct Context<'a> {
    shapes: HashMap<&'a str, &'a ox_ast::Declaration>,
    choices: HashMap<&'a str, &'a ox_ast::Declaration>,
    functions: HashMap<&'a str, &'a ox_ast::Declaration>,
    variants: HashMap<&'a str, (&'a str, Option<&'a str>)>,
}

fn check_block(
    block: &Block,
    parent: &mut HashMap<String, (String, bool)>,
    return_type: &str,
    context: &Context<'_>,
    loop_depth: usize,
) -> Result<String, Diagnostic> {
    let mut environment = parent.clone();
    for statement in &block.statements {
        match statement {
            Statement::Let {
                name,
                mutable,
                type_name,
                value,
            } => {
                if environment.contains_key(name) {
                    return diagnostic(format!("'{name}' is already defined"));
                }
                let actual =
                    check_expression(value, &environment, return_type, context, loop_depth)?;
                if let Some(expected) = type_name {
                    expect(expected, &actual)?;
                }
                environment.insert(
                    name.clone(),
                    (type_name.clone().unwrap_or(actual), *mutable),
                );
            }
            Statement::Assign { name, value } => {
                let Some((expected, mutable)) = environment.get(name) else {
                    return diagnostic(format!("unknown variable '{name}'"));
                };
                if !mutable {
                    return diagnostic(format!("cannot assign to immutable value '{name}'"));
                }
                expect(
                    expected,
                    &check_expression(value, &environment, return_type, context, loop_depth)?,
                )?;
            }
            Statement::Loop { condition, body } => {
                expect(
                    "Bool",
                    &check_expression(condition, &environment, return_type, context, loop_depth)?,
                )?;
                check_block(body, &mut environment, return_type, context, loop_depth + 1)?;
            }
            Statement::Break | Statement::Continue if loop_depth == 0 => {
                return diagnostic("loop control is only valid inside loop")
            }
            Statement::Return(value) => expect(
                return_type,
                &check_expression(value, &environment, return_type, context, loop_depth)?,
            )?,
            Statement::Fail(value) | Statement::Expression(value) => {
                check_expression(value, &environment, return_type, context, loop_depth)?;
            }
            _ => {}
        }
    }
    if matches!(
        block.statements.last(),
        Some(Statement::Return(_) | Statement::Fail(_))
    ) {
        return Ok("Never".into());
    }
    block.tail.as_ref().map_or(Ok("Unit".into()), |value| {
        check_expression(value, &environment, return_type, context, loop_depth)
    })
}

fn check_expression(
    expression: &Expression,
    environment: &HashMap<String, (String, bool)>,
    return_type: &str,
    context: &Context<'_>,
    loop_depth: usize,
) -> Result<String, Diagnostic> {
    match expression {
        Expression::Integer(_) => Ok("Int".into()),
        Expression::String(_) => Ok("String".into()),
        Expression::Boolean(_) => Ok("Bool".into()),
        Expression::Object(_) => Ok("Json".into()),
        Expression::Identifier(name) => environment
            .get(name)
            .map(|v| v.0.clone())
            .or_else(|| {
                context
                    .functions
                    .get(name.as_str())
                    .map(|_| format!("fn:{name}"))
            })
            .or_else(|| {
                context
                    .variants
                    .get(name.as_str())
                    .map(|(choice, payload)| {
                        if payload.is_some() {
                            format!("variant:{name}")
                        } else {
                            (*choice).into()
                        }
                    })
            })
            .ok_or_else(|| Diagnostic::new(format!("unknown name '{name}'"), 0..0)),
        Expression::Construct { name, fields } => {
            let shape = context
                .shapes
                .get(name.as_str())
                .ok_or_else(|| Diagnostic::new(format!("unknown shape '{name}'"), 0..0))?;
            let provided: HashMap<_, _> = fields.iter().map(|(n, v)| (n.as_str(), v)).collect();
            for field in &shape.fields {
                let value = provided.get(field.name.as_str()).ok_or_else(|| {
                    Diagnostic::new(format!("missing field '{}'", field.name), 0..0)
                })?;
                expect(
                    &field.type_name,
                    &check_expression(value, environment, return_type, context, loop_depth)?,
                )?;
            }
            if provided.len() != shape.fields.len() {
                return diagnostic(format!("unknown field on shape '{name}'"));
            }
            Ok(name.clone())
        }
        Expression::Member { object, field } => {
            let owner = check_expression(object, environment, return_type, context, loop_depth)?;
            let shape = context
                .shapes
                .get(owner.as_str())
                .ok_or_else(|| Diagnostic::new(format!("type '{owner}' has no fields"), 0..0))?;
            shape
                .fields
                .iter()
                .find(|f| f.name == *field)
                .map(|f| f.type_name.clone())
                .ok_or_else(|| Diagnostic::new(format!("unknown field '{field}'"), 0..0))
        }
        Expression::Unary { operator, value } => {
            let expected = if operator == "!" { "Bool" } else { "Int" };
            expect(
                expected,
                &check_expression(value, environment, return_type, context, loop_depth)?,
            )?;
            Ok(expected.into())
        }
        Expression::Binary {
            left,
            operator,
            right,
        } => {
            let l = check_expression(left, environment, return_type, context, loop_depth)?;
            let r = check_expression(right, environment, return_type, context, loop_depth)?;
            if ["&&", "||"].contains(&operator.as_str()) {
                expect("Bool", &l)?;
                expect("Bool", &r)?;
                Ok("Bool".into())
            } else if ["==", "!=", "<", "<=", ">", ">="].contains(&operator.as_str()) {
                expect(&l, &r)?;
                Ok("Bool".into())
            } else {
                expect("Int", &l)?;
                expect("Int", &r)?;
                Ok("Int".into())
            }
        }
        Expression::Call { callee, args } => {
            if let Expression::Identifier(name) = callee.as_ref() {
                if let Some(function) = context.functions.get(name.as_str()) {
                    if args.len() != function.params.len() {
                        return diagnostic("incorrect argument count");
                    }
                    for (arg, param) in args.iter().zip(&function.params) {
                        expect(
                            &param.type_name,
                            &check_expression(arg, environment, return_type, context, loop_depth)?,
                        )?;
                    }
                    return Ok(function
                        .return_type
                        .clone()
                        .unwrap_or_else(|| "Unit".into()));
                }
                if ["print", "println"].contains(&name.as_str()) {
                    if args.len() != 1 {
                        return diagnostic("incorrect argument count");
                    }
                    check_expression(&args[0], environment, return_type, context, loop_depth)?;
                    return Ok("Unit".into());
                }
                if let Some((count, result)) = match name.as_str() {
                    "http_get" | "http_delete" => Some((1, "String")),
                    "http_post" | "http_put" => Some((2, "String")),
                    _ => None,
                } {
                    if args.len() != count {
                        return diagnostic("incorrect argument count");
                    }
                    for arg in args {
                        expect(
                            "String",
                            &check_expression(arg, environment, return_type, context, loop_depth)?,
                        )?;
                    }
                    return Ok(result.into());
                }
                if let Some((choice, payload)) = context.variants.get(name.as_str()) {
                    let expected =
                        payload.ok_or_else(|| Diagnostic::new("variant is not callable", 0..0))?;
                    if args.len() != 1 {
                        return diagnostic("variant expects one argument");
                    }
                    expect(
                        expected,
                        &check_expression(&args[0], environment, return_type, context, loop_depth)?,
                    )?;
                    return Ok((*choice).into());
                }
            }
            diagnostic("value is not callable")
        }
        Expression::If {
            condition,
            then_branch,
            else_branch,
        } => {
            expect(
                "Bool",
                &check_expression(condition, environment, return_type, context, loop_depth)?,
            )?;
            let mut e = environment.clone();
            let a = check_block(then_branch, &mut e, return_type, context, loop_depth)?;
            let b = check_block(else_branch, &mut e, return_type, context, loop_depth)?;
            if a == "Never" {
                return Ok(b);
            }
            if b == "Never" {
                return Ok(a);
            }
            expect(&a, &b)?;
            Ok(a)
        }
        Expression::Try {
            value,
            binding,
            handler,
        } => {
            let actual = check_expression(value, environment, return_type, context, loop_depth)?;
            let mut e = environment.clone();
            e.insert(binding.clone(), ("Json".into(), false));
            let caught = check_block(handler, &mut e, return_type, context, loop_depth)?;
            expect(&actual, &caught)?;
            Ok(actual)
        }
        Expression::Pick {
            value,
            cases,
            other,
        } => {
            let choice_name =
                check_expression(value, environment, return_type, context, loop_depth)?;
            let choice = context
                .choices
                .get(choice_name.as_str())
                .ok_or_else(|| Diagnostic::new("pick requires a choice", 0..0))?;
            let mut result: Option<String> = None;
            let mut seen = HashSet::new();
            for case in cases {
                if !seen.insert(case.variant.as_str()) {
                    return diagnostic("duplicate case");
                }
                let variant = choice
                    .variants
                    .iter()
                    .find(|v| v.name == case.variant)
                    .ok_or_else(|| Diagnostic::new("unknown variant", 0..0))?;
                let mut e = environment.clone();
                if let (Some(binding), Some(payload)) = (&case.binding, &variant.payload_type) {
                    e.insert(binding.clone(), (payload.clone(), false));
                }
                let t = check_block(&case.body, &mut e, return_type, context, loop_depth)?;
                if let Some(r) = &result {
                    expect(r, &t)?;
                } else {
                    result = Some(t);
                }
            }
            if other.is_none() && seen.len() != choice.variants.len() {
                return diagnostic("non-exhaustive pick");
            }
            if let Some(block) = other {
                let mut e = environment.clone();
                let t = check_block(block, &mut e, return_type, context, loop_depth)?;
                if let Some(r) = &result {
                    expect(r, &t)?;
                } else {
                    result = Some(t);
                }
            }
            Ok(result.unwrap_or_else(|| "Unit".into()))
        }
    }
}

fn expect(expected: &str, actual: &str) -> Result<(), Diagnostic> {
    if expected == actual {
        Ok(())
    } else {
        diagnostic(format!("expected {expected}, found {actual}"))
    }
}
fn diagnostic<T>(message: impl Into<String>) -> Result<T, Diagnostic> {
    Err(Diagnostic::new(message, 0..0))
}

fn check_nominal_types(program: &Program) -> Result<(), Diagnostic> {
    let builtins = ["Int", "Bool", "String", "Unit", "Json"];
    let nominal_names: HashSet<&str> = program
        .declarations
        .iter()
        .filter(|declaration| {
            matches!(
                declaration.kind,
                DeclarationKind::Shape | DeclarationKind::Choice
            )
        })
        .map(|declaration| declaration.name.as_str())
        .collect();

    for shape in program
        .declarations
        .iter()
        .filter(|declaration| declaration.kind == DeclarationKind::Shape)
    {
        let mut fields = HashSet::new();
        for field in &shape.fields {
            if !fields.insert(field.name.as_str()) {
                return Err(Diagnostic::new(
                    format!(
                        "field '{}' is already defined in shape '{}'",
                        field.name, shape.name
                    ),
                    0..0,
                ));
            }
            if !builtins.contains(&field.type_name.as_str())
                && !nominal_names.contains(field.type_name.as_str())
            {
                return Err(Diagnostic::new(
                    format!("unknown type '{}'", field.type_name),
                    0..0,
                ));
            }
            if field.type_name == shape.name {
                return Err(Diagnostic::new(
                    format!("shape '{}' cannot contain itself directly", shape.name),
                    0..0,
                ));
            }
        }
    }

    let mut all_variants = HashSet::new();
    for choice in program
        .declarations
        .iter()
        .filter(|declaration| declaration.kind == DeclarationKind::Choice)
    {
        let mut choice_variants = HashSet::new();
        for variant in &choice.variants {
            if !choice_variants.insert(variant.name.as_str())
                || !all_variants.insert(variant.name.as_str())
            {
                return Err(Diagnostic::new(
                    format!("variant '{}' is already defined", variant.name),
                    0..0,
                ));
            }
            if let Some(payload_type) = &variant.payload_type {
                if !builtins.contains(&payload_type.as_str())
                    && !nominal_names.contains(payload_type.as_str())
                {
                    return Err(Diagnostic::new(
                        format!("unknown type '{}'", payload_type),
                        0..0,
                    ));
                }
            }
        }
    }
    Ok(())
}

fn check_entrypoint(program: &Program) -> Result<(), Diagnostic> {
    let has_main = program.declarations.iter().any(|declaration| {
        declaration.kind == DeclarationKind::Function && declaration.name == "main"
    });
    let has_api = program
        .declarations
        .iter()
        .any(|declaration| declaration.kind == DeclarationKind::Api);

    let has_test = program
        .declarations
        .iter()
        .any(|d| d.kind == DeclarationKind::Test);
    if has_main || has_api || has_test {
        Ok(())
    } else {
        Err(Diagnostic::new(
            "program must define fx main() or an api",
            0..0,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ox_lexer::lex;
    use ox_parser::parse;

    #[test]
    fn accepts_known_shape_field_types() {
        let program = parse(&lex("shape User { id: Int, } fx main() {}").unwrap()).unwrap();
        assert!(check(&program).is_ok());
    }

    #[test]
    fn rejects_duplicate_shape_fields() {
        let program =
            parse(&lex("shape User { id: Int, id: Int, } fx main() {}").unwrap()).unwrap();
        assert!(check(&program)
            .unwrap_err()
            .message
            .contains("already defined"));
    }

    #[test]
    fn accepts_choice_payload_types_and_rejects_duplicate_variants() {
        let valid = parse(
            &lex("shape User { id: Int, } choice Result { Ok(User), Missing, } fx main() {}")
                .unwrap(),
        )
        .unwrap();
        assert!(check(&valid).is_ok());
        let duplicate =
            parse(&lex("choice Result { Ok(Int), Ok(String), } fx main() {}").unwrap()).unwrap();
        assert!(check(&duplicate)
            .unwrap_err()
            .message
            .contains("already defined"));
    }
}
