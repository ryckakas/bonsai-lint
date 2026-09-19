#[must_use]
pub fn strip_quotes(text: &str) -> String {
    let trimmed = text.trim();
    let unquoted = trimmed
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .or_else(|| {
            trimmed
                .strip_prefix('\'')
                .and_then(|rest| rest.strip_suffix('\''))
        });
    unquoted.unwrap_or(trimmed).to_string()
}

/// A key must not depend on how a member chain was line-wrapped.
#[must_use]
pub fn compact(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join("")
}
