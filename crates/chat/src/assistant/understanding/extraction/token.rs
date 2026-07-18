pub(super) fn tokens_with_spans(message: &str) -> Vec<(&str, usize, usize)> {
    let mut result = Vec::new();
    let mut start = None;
    for (index, ch) in message
        .char_indices()
        .chain(std::iter::once((message.len(), ' ')))
    {
        if ch.is_ascii_alphanumeric() || ch == '-' {
            start.get_or_insert(index);
        } else if let Some(begin) = start.take() {
            result.push((&message[begin..index], begin, index));
        }
    }
    result
}

pub(super) fn words(message: &str) -> Vec<&str> {
    message
        .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '-')
        .filter(|part| !part.is_empty())
        .collect()
}

pub(super) fn extract_currency(message: &str) -> Option<String> {
    message
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .find(|word| matches!(*word, "IDR" | "USD" | "EUR" | "AED" | "SAR"))
        .map(str::to_string)
}

pub(super) fn extract_person_name(message: &str) -> Option<String> {
    let parts = message
        .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '-' && ch != '_')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    for pair in parts.windows(2) {
        if matches!(pair[0].to_ascii_lowercase().as_str(), "named" | "name")
            && pair[1].chars().any(char::is_alphabetic)
        {
            return Some(pair[1].to_string());
        }
    }
    for pair in parts.windows(2) {
        let next = pair[1].to_ascii_lowercase();
        if matches!(pair[0].to_ascii_lowercase().as_str(), "client" | "find")
            && !matches!(next.as_str(), "client" | "name" | "named")
            && pair[1].chars().any(char::is_alphabetic)
        {
            return Some(pair[1].to_string());
        }
    }
    None
}
