use ox_ast::{DeclarationKind, Program};
use ox_diagnostics::Diagnostic;
use std::collections::HashSet;

pub fn check(program: &Program) -> Result<(), Diagnostic> {
    check_nominal_types(program)?;
    check_entrypoint(program)
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

    if has_main || has_api {
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
