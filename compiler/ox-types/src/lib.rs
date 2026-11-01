use ox_ast::{DeclarationKind, Program};
use ox_diagnostics::Diagnostic;

pub fn check_entrypoint(program: &Program) -> Result<(), Diagnostic> {
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
