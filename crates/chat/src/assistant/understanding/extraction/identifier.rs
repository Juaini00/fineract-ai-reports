#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SensitiveIdentifierKind {
    SavingsAccountNumber,
    LoanNumber,
}

#[derive(Clone)]
pub(crate) struct SensitiveIdentifier {
    kind: SensitiveIdentifierKind,
    value: String,
}

impl SensitiveIdentifier {
    #[cfg(test)]
    pub(crate) fn kind(&self) -> SensitiveIdentifierKind {
        self.kind
    }

    pub(crate) fn expose(&self) -> &str {
        &self.value
    }
}

impl std::fmt::Debug for SensitiveIdentifier {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SensitiveIdentifier")
            .field("kind", &self.kind)
            .field("value", &"[REDACTED]")
            .finish()
    }
}

#[derive(Clone, Debug)]
pub(crate) struct IdentifierIntake {
    semantic_message: String,
    sensitive_identifier: Option<SensitiveIdentifier>,
}

impl IdentifierIntake {
    pub(crate) fn semantic_message(&self) -> &str {
        &self.semantic_message
    }

    pub(crate) fn sensitive_identifier(&self) -> Option<&SensitiveIdentifier> {
        self.sensitive_identifier.as_ref()
    }

    pub(crate) fn into_parts(self) -> (String, Option<SensitiveIdentifier>) {
        (self.semantic_message, self.sensitive_identifier)
    }
}

pub(crate) fn identifier_intake(message: &str) -> IdentifierIntake {
    const MARKERS: [(&str, SensitiveIdentifierKind, &str); 6] = [
        (
            "savings account number",
            SensitiveIdentifierKind::SavingsAccountNumber,
            "[SAVINGS_ACCOUNT_NUMBER]",
        ),
        (
            "nomor rekening tabungan",
            SensitiveIdentifierKind::SavingsAccountNumber,
            "[SAVINGS_ACCOUNT_NUMBER]",
        ),
        (
            "account number",
            SensitiveIdentifierKind::SavingsAccountNumber,
            "[SAVINGS_ACCOUNT_NUMBER]",
        ),
        (
            "loan account number",
            SensitiveIdentifierKind::LoanNumber,
            "[LOAN_NUMBER]",
        ),
        (
            "loan number",
            SensitiveIdentifierKind::LoanNumber,
            "[LOAN_NUMBER]",
        ),
        (
            "nomor pinjaman",
            SensitiveIdentifierKind::LoanNumber,
            "[LOAN_NUMBER]",
        ),
    ];

    let lower = message.to_ascii_lowercase();
    for (marker, kind, placeholder) in MARKERS {
        let Some(marker_start) = find_marker(&lower, marker) else {
            continue;
        };
        let value_start = marker_start + marker.len();
        let Some((span_start, span_end, normalized)) = identifier_span(message, value_start) else {
            continue;
        };
        let semantic_message = format!(
            "{}{}{}",
            &message[..span_start],
            placeholder,
            &message[span_end..]
        );
        return IdentifierIntake {
            semantic_message,
            sensitive_identifier: Some(SensitiveIdentifier {
                kind,
                value: normalized,
            }),
        };
    }

    IdentifierIntake {
        semantic_message: message.to_owned(),
        sensitive_identifier: None,
    }
}

fn find_marker(message: &str, marker: &str) -> Option<usize> {
    message.match_indices(marker).find_map(|(start, _)| {
        let before_ok = start == 0
            || message[..start]
                .chars()
                .next_back()
                .is_some_and(|character| !character.is_ascii_alphanumeric());
        let end = start + marker.len();
        let after_ok = end == message.len()
            || message[end..]
                .chars()
                .next()
                .is_some_and(|character| !character.is_ascii_alphanumeric());
        (before_ok && after_ok).then_some(start)
    })
}

fn identifier_span(message: &str, after_marker: usize) -> Option<(usize, usize, String)> {
    let suffix = message.get(after_marker..)?;
    let leading = suffix
        .char_indices()
        .take_while(|(_, character)| {
            character.is_ascii_whitespace() || matches!(character, ':' | '#')
        })
        .map(|(_, character)| character.len_utf8())
        .sum::<usize>();
    let start = after_marker + leading;
    let candidate = message.get(start..)?;
    let mut normalized = String::new();
    let mut end = start;

    for (offset, character) in candidate.char_indices() {
        if character.is_ascii_digit() {
            normalized.push(character);
            end = start + offset + character.len_utf8();
        } else if character == '-' || character.is_ascii_whitespace() {
            continue;
        } else {
            break;
        }
    }

    if normalized.len() < 4 {
        return None;
    }
    Some((start, end, normalized))
}
