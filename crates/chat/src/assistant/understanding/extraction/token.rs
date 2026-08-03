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

/// Words that introduce a name the user is about to spell out.
///
/// `nama` is here because `client_name_lookup`'s own documented example is
/// Indonesian ("ada gak nama Tony di client kita?") — the extractor is reading
/// what the user typed, which is not the same thing as the assistant replying
/// in Indonesian, so this does not touch the English-only product rule.
const NAME_ANCHORS: &[&str] = &[
    "named", "name", "nama", "bernama", "client", "customer", "nasabah", "for",
];

/// Nouns that make a capitalised phrase a *thing*, not a person. "Head Office",
/// "Weekly Charge" and "Current Account USD" are all capitalised runs sitting in
/// the same grammatical slot as "John Doe"; the only cheap signal separating
/// them is that they contain a banking noun and a person's name does not.
///
/// Getting this wrong is not symmetric: binding the wrong `search` value
/// silently returns a different customer's accounts, so anything carrying a
/// domain noun is classified away from `PersonName`, never towards it.
const OFFICE_WORDS: &[&str] = &["office", "branch", "kantor", "cabang"];
const CHARGE_WORDS: &[&str] = &["charge", "fee", "penalty", "biaya", "denda"];
const PRODUCT_WORDS: &[&str] = &[
    "account", "product", "savings", "saving", "deposit", "tabungan", "rekening",
];

/// What a capitalised run in the message denotes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NamedEntityKind {
    Person,
    Office,
    ChargeType,
    Product,
}

/// Capitalised phrases in `message`, each classified by what it names.
///
/// The old version of this fired only after the literal English token
/// `named`/`name` and captured exactly one word, so "named Jonathan Doe" bound
/// `search = "Jonathan"` and the Indonesian `nama` never matched at all. It also
/// had no notion of an office, a charge type or a product, which is why every
/// capability needing one of those was unreachable.
///
/// Precision rules, in the order they matter:
/// - a run must be capitalised, and a run that starts the sentence is ignored —
///   otherwise every "Show", "Find" and "How" becomes a name;
/// - a single-word run needs an explicit anchor in front of it ("named Tony"),
///   because one stray capital ("the savings account ID") is not evidence;
/// - a multi-word run stands on its own — two adjacent capitals mid-sentence is
///   a proper noun in both English and Indonesian;
/// - a run containing a domain noun is that thing, never a person.
///
/// ponytail: capitalisation heuristic with a hard ceiling — it cannot see a
/// lowercase-typed name ("find client tony"). That case is the LLM router's
/// job, which supplies the entity directly; this only has to be right when it
/// does fire.
pub(super) fn extract_named_entities(message: &str) -> Vec<(NamedEntityKind, String)> {
    let tokens = tokens_with_spans(message);
    let mut found = Vec::new();
    let mut index = 0;
    while index < tokens.len() {
        if !is_capitalised(tokens[index].0) {
            index += 1;
            continue;
        }
        let start = index;
        let mut end = index;
        while end + 1 < tokens.len()
            && is_capitalised(tokens[end + 1].0)
            && !separated(message, tokens[end].2, tokens[end + 1].1)
        {
            end += 1;
        }
        index = end + 1;

        // A sentence-initial capital is grammar, not a name.
        if start == 0 || sentence_start(message, tokens[start].1) {
            continue;
        }
        let anchor = tokens[start - 1].0.to_ascii_lowercase();
        let words = tokens[start..=end]
            .iter()
            .map(|token| token.0)
            .collect::<Vec<_>>();
        if words.len() == 1 && !NAME_ANCHORS.contains(&anchor.as_str()) {
            continue;
        }
        let phrase = words.join(" ");
        let lower = phrase.to_ascii_lowercase();
        let has = |list: &[&str]| {
            lower
                .split(' ')
                .any(|word| list.contains(&word) || list.contains(&word.trim_end_matches('s')))
        };
        let kind = if has(OFFICE_WORDS) || OFFICE_WORDS.contains(&anchor.as_str()) {
            NamedEntityKind::Office
        } else if has(CHARGE_WORDS) || matches!(anchor.as_str(), "type" | "tipe" | "jenis") {
            NamedEntityKind::ChargeType
        } else if has(PRODUCT_WORDS) {
            NamedEntityKind::Product
        } else {
            NamedEntityKind::Person
        };
        found.push((kind, phrase));
    }
    found
}

/// The exact decimal a user quoted as a transaction amount. Anchored on the
/// word "amount" so a bare number elsewhere in the sentence — a limit, a year —
/// is never mistaken for one, and required to carry a decimal point so an
/// integer count cannot slip through.
pub(super) fn extract_transaction_amount(message: &str) -> Option<String> {
    let lower = message.to_ascii_lowercase();
    let after = ["amount", "jumlah", "sebesar"]
        .iter()
        .filter_map(|anchor| lower.find(anchor).map(|at| at + anchor.len()))
        .min()?;
    // `tokens_with_spans` splits on '.', so the decimal has to be read off the
    // raw text: a truncated "0" would bind a different transaction entirely.
    let rest = &message[after..];
    let start = rest.find(|ch: char| ch.is_ascii_digit())?;
    let digits: String = rest[start..]
        .chars()
        .take_while(|ch| ch.is_ascii_digit() || *ch == '.')
        .collect();
    digits.contains('.').then_some(digits)
}

fn is_capitalised(token: &str) -> bool {
    token
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_uppercase())
}

/// True when only spaces separate two tokens; punctuation ends a run.
fn separated(message: &str, end: usize, next: usize) -> bool {
    message[end..next].chars().any(|ch| ch != ' ')
}

fn sentence_start(message: &str, at: usize) -> bool {
    message[..at]
        .chars()
        .rev()
        .find(|ch| !ch.is_whitespace())
        .is_none_or(|ch| matches!(ch, '.' | '?' | '!'))
}
