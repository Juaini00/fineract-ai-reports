use crate::assistant::Quantity;

pub(super) fn quantity_parts(quantity: &Quantity) -> Option<(&'static str, i64)> {
    match quantity {
        Quantity::Limit { value } => Some(("limit", *value)),
        Quantity::TopN { value } => Some(("top_n", *value)),
        Quantity::All | Quantity::Default => None,
    }
}

pub(super) fn extract_quantity(message: &str, words: &[&str]) -> Option<Quantity> {
    for (idx, word) in words.iter().enumerate() {
        let Ok(value) = word.parse::<i64>() else {
            continue;
        };
        if !(1..=100).contains(&value) {
            continue;
        }
        let near = words[idx.saturating_sub(2)..usize::min(words.len(), idx + 3)].join(" ");
        if near.contains("days") || near.contains("hari") {
            continue;
        }
        return Some(
            if near.contains("top")
                || message.contains(" most ")
                || message.contains("highest")
                || message.contains("rank")
            {
                Quantity::TopN { value }
            } else {
                Quantity::Limit { value }
            },
        );
    }
    None
}
