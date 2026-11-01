pub fn normalize_newline(source: &str) -> String {
    let trimmed = source.trim_end();
    if trimmed.is_empty() {
        String::new()
    } else {
        format!("{trimmed}\n")
    }
}
