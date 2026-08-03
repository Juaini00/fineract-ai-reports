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

/// Anchors strong enough to introduce a name the user did **not** capitalise.
///
/// Only the words that literally announce a name. A capital is corroborating
/// evidence; without it the anchor carries the whole decision, and the rest of
/// `NAME_ANCHORS` cannot carry it. `client`/`customer`/`nasabah` are followed by
/// an ordinary noun far more often than by a name ("look up a client please",
/// "client accounts"), and `for` by a date or a quantifier. They stay on the
/// capitalised path, where the capital does the work.
///
/// The names those anchors would have caught are not lost: the router names them
/// as entities, and `MODEL_VERBATIM_ENTITIES` admits any entity whose value the
/// user actually typed.
const LOWERCASE_NAME_ANCHORS: &[&str] = &["named", "name", "nama", "bernama"];

/// How many tokens a lowercase run may take. A capitalised run self-terminates
/// at the first lowercase word; a lowercase one has no such edge, so it needs a
/// hard stop. Two covers "john doe", and under-capturing is the safe direction:
/// `search` reaches SQL as `ILIKE '%value%'`, so a short value still matches the
/// full name while a long one matches nothing.
const MAX_LOWERCASE_NAME_TOKENS: usize = 2;

/// Function words that end a lowercase run. Without them "nama john doe di
/// office foo" would swallow its own tail and bind a string no client matches.
/// Only closed-class words belong here — prepositions, articles, conjunctions,
/// pronouns, question words — never a word that could be somebody's name.
const NAME_STOP_WORDS: &[&str] = &[
    "a", "ada", "all", "an", "and", "apa", "are", "as", "at", "atau", "by", "dan", "dari",
    "dengan", "di", "each", "every", "her", "his", "id", "ids", "in", "ini", "is", "itu", "kami",
    "ke", "kita", "me", "my", "no", "not", "of", "on", "only", "or", "our", "pada", "per", "punya",
    "saya", "semua", "siapa", "the", "their", "this", "tidak", "to", "us", "was", "we", "were",
    "which", "who", "with", "yang", "yg",
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
/// A run that is *not* capitalised is read too, but only directly after a
/// `LOWERCASE_NAME_ANCHORS` word — "nama john doe" was returning nothing, so a
/// user who typed their own language's phrasing in lower case got
/// `missing parameter search` even though the SQL is `ILIKE '%…%'` and would
/// have matched. See `lowercase_name_run` for what that path refuses.
///
/// ponytail: capitalisation-or-anchor heuristic with a hard ceiling — it still
/// cannot see a lowercase name with no anchor at all ("how much did john doe
/// save?"). That case is the LLM router's job, which supplies the entity
/// directly; this only has to be right when it does fire.
pub(super) fn extract_named_entities(message: &str) -> Vec<(NamedEntityKind, String)> {
    let tokens = tokens_with_spans(message);
    let mut found = Vec::new();
    let mut index = 0;
    while index < tokens.len() {
        if !is_capitalised(tokens[index].0) {
            if let Some((phrase, next)) = lowercase_name_run(message, &tokens, index) {
                found.push((NamedEntityKind::Person, phrase));
                index = next;
            } else {
                index += 1;
            }
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
        let kind = classify(&phrase, &anchor);
        found.push((kind, phrase));
    }
    found
}

/// What a phrase denotes, given the word that introduced it.
fn classify(phrase: &str, anchor: &str) -> NamedEntityKind {
    let lower = phrase.to_ascii_lowercase();
    let has = |list: &[&str]| {
        lower
            .split(' ')
            .any(|word| list.contains(&word) || list.contains(&word.trim_end_matches('s')))
    };
    if has(OFFICE_WORDS) || OFFICE_WORDS.contains(&anchor) {
        NamedEntityKind::Office
    } else if has(CHARGE_WORDS) || matches!(anchor, "type" | "tipe" | "jenis") {
        NamedEntityKind::ChargeType
    } else if has(PRODUCT_WORDS) {
        NamedEntityKind::Product
    } else {
        NamedEntityKind::Person
    }
}

/// A person's name typed in lower case, read off the anchor in front of it.
///
/// Returns the phrase and the token index to resume at. What it refuses, and
/// why each refusal is load-bearing:
/// - no anchor, or a sentence boundary between anchor and run — the anchor is
///   the only evidence there is, so it has to be adjacent;
/// - a run starting on an anchor or a stop word ("client name", "nasabah di
///   jakarta") — those are the user talking *about* names, not typing one;
/// - a run continuing past a stop word or past `MAX_LOWERCASE_NAME_TOKENS`, so
///   "nama john doe di office foo" yields "john doe" and not its own tail;
/// - a run carrying a banking noun ("client head office", "client accounts") —
///   the capitalised path may classify such a phrase as an office or a product,
///   but with no capital there is nothing corroborating it, and inventing an
///   office filter nobody asked for silently narrows the answer. Lower case
///   yields a person or nothing.
fn lowercase_name_run(
    message: &str,
    tokens: &[(&str, usize, usize)],
    start: usize,
) -> Option<(String, usize)> {
    let anchor_word = tokens.get(start.checked_sub(1)?)?.0.to_ascii_lowercase();
    if !LOWERCASE_NAME_ANCHORS.contains(&anchor_word.as_str())
        || sentence_start(message, tokens[start].1)
        || !is_name_word(tokens[start].0)
    {
        return None;
    }
    let mut end = start;
    while end + 1 - start < MAX_LOWERCASE_NAME_TOKENS
        && end + 1 < tokens.len()
        && !separated(message, tokens[end].2, tokens[end + 1].1)
        && is_name_word(tokens[end + 1].0)
    {
        end += 1;
    }
    let phrase = tokens[start..=end]
        .iter()
        .map(|token| token.0)
        .collect::<Vec<_>>()
        .join(" ");
    matches!(classify(&phrase, &anchor_word), NamedEntityKind::Person).then_some((phrase, end + 1))
}

/// Could this token be part of somebody's name? Letters (and hyphens) only, and
/// not a word that is doing grammatical work instead.
fn is_name_word(token: &str) -> bool {
    let lower = token.to_ascii_lowercase();
    token.chars().any(|ch| ch.is_ascii_alphabetic())
        && token
            .chars()
            .all(|ch| ch.is_ascii_alphabetic() || ch == '-')
        && !NAME_STOP_WORDS.contains(&lower.as_str())
        && !LOWERCASE_NAME_ANCHORS.contains(&lower.as_str())
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
