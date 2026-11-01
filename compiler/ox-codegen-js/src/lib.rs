use ox_ast::Program;

pub fn declaration_summary(program: &Program) -> String {
    format!("// Ox declarations: {}\n", program.declarations.len())
}
