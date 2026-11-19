//! Identifier case conversions shared by the scanner (default names) and the helpers.

/// Splits an identifier into lowercase words. Handles `snake_case`, `kebab-case`,
/// `SCREAMING_SNAKE`, `PascalCase`, `camelCase` and acronyms (`HTTPServer` -> `http`,
/// `server`). Digits stay attached to the word before them.
pub(crate) fn words(input: &str) -> Vec<String> {
    let chars: Vec<char> = input.chars().collect();
    let mut words = Vec::new();
    let mut current = String::new();
    for (i, &c) in chars.iter().enumerate() {
        if c == '_' || c == '-' || c.is_whitespace() {
            if !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
            continue;
        }
        if c.is_uppercase() && !current.is_empty() {
            let prev = chars[i - 1];
            let next_is_lower = chars.get(i + 1).is_some_and(|n| n.is_lowercase());
            if prev.is_lowercase()
                || prev.is_ascii_digit()
                || (prev.is_uppercase() && next_is_lower)
            {
                words.push(std::mem::take(&mut current));
            }
        }
        current.extend(c.to_lowercase());
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

pub(crate) fn snake(input: &str) -> String {
    words(input).join("_")
}

pub(crate) fn screaming(input: &str) -> String {
    snake(input).to_uppercase()
}

pub(crate) fn pascal(input: &str) -> String {
    words(input).iter().map(|w| capitalize(w)).collect()
}

pub(crate) fn camel(input: &str) -> String {
    let words = words(input);
    let mut out = String::new();
    for (i, w) in words.iter().enumerate() {
        if i == 0 {
            out.push_str(w);
        } else {
            out.push_str(&capitalize(w));
        }
    }
    out
}

fn capitalize(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversions() {
        assert_eq!(snake("MaxFreeSlotStrategy"), "max_free_slot_strategy");
        assert_eq!(snake("GNPYReachTable"), "gnpy_reach_table");
        assert_eq!(snake("HTTPServer2Go"), "http_server2_go");
        assert_eq!(screaming("maxFreeSlot"), "MAX_FREE_SLOT");
        assert_eq!(pascal("MAX_FREE_SLOT"), "MaxFreeSlot");
        assert_eq!(pascal("max-free-slot"), "MaxFreeSlot");
        assert_eq!(camel("MAX_FREE_SLOT"), "maxFreeSlot");
        assert_eq!(snake("already_snake"), "already_snake");
        assert_eq!(pascal(""), "");
    }
}
