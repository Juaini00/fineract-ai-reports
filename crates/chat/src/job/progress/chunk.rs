//! Splits rendered markdown into stream-safe pieces.
//!
//! Two rules. Concatenating the output must reproduce the input exactly, so the
//! client can rebuild the message by appending. And a block that is only
//! meaningful whole — a table, a fenced code block — is never split, because
//! revealing a table row by row is theatre rather than feedback.

/// Roughly one clause. Small enough to feel like typing, large enough that a
/// long answer does not turn into hundreds of events.
const TARGET_CHARS: usize = 48;

pub fn chunk_markdown(markdown: &str) -> Vec<String> {
    let mut chunks = Vec::new();
    for block in split_blocks(markdown) {
        if is_atomic(&block) {
            chunks.push(block);
        } else {
            chunks.extend(split_prose(&block));
        }
    }
    chunks
}

/// Splits on blank-line boundaries, keeping the separators attached so the
/// result stays lossless. A fenced block is kept together even when it contains
/// blank lines.
fn split_blocks(markdown: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut current = String::new();
    let mut in_fence = false;

    for line in markdown.split_inclusive('\n') {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            current.push_str(line);
            if !in_fence {
                blocks.push(std::mem::take(&mut current));
            }
            continue;
        }
        if !in_fence && line.trim().is_empty() && !current.is_empty() {
            current.push_str(line);
            blocks.push(std::mem::take(&mut current));
            continue;
        }
        current.push_str(line);
    }
    if !current.is_empty() {
        blocks.push(current);
    }
    blocks
}

fn is_atomic(block: &str) -> bool {
    block.contains("```") || block.lines().any(|line| line.trim_start().starts_with('|'))
}

/// Accumulates whole words until the target length, then breaks. Never splits
/// inside a word, because a half-word looks like corruption rather than typing.
///
/// Breaks as soon as the target length is reached, at the word boundary in
/// hand — not only after sentence punctuation. Waiting for punctuation can
/// stall past the target (e.g. three ~20-char sentences: the second boundary
/// falls short of the target and the third lands well past it), collapsing
/// everything into one chunk instead of streaming pieces.
fn split_prose(block: &str) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();

    for word in block.split_inclusive(char::is_whitespace) {
        current.push_str(word);
        if current.len() >= TARGET_CHARS {
            chunks.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    if chunks.is_empty() && !block.is_empty() {
        chunks.push(block.to_string());
    }
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_lossless(input: &str) {
        let chunks = chunk_markdown(input);
        assert_eq!(
            chunks.concat(),
            input,
            "chunking must be lossless; the client reassembles by concatenation"
        );
    }

    #[test]
    fn concatenating_chunks_reproduces_the_input() {
        assert_lossless("Found 50 charges. The newest is Weekly Charge. Older ones follow.");
        assert_lossless("");
        assert_lossless("One sentence only");
        assert_lossless("Line one\n\nLine two\n\nLine three");
    }

    #[test]
    fn splits_prose_into_multiple_chunks() {
        let chunks = chunk_markdown("First sentence here. Second sentence here. Third one here.");
        assert!(
            chunks.len() > 1,
            "prose should stream in pieces, got {chunks:?}"
        );
    }

    #[test]
    fn never_splits_inside_a_word() {
        let input = "Ditemukan lima puluh charge terbaru pada delapan kantor cabang hari ini.";
        for chunk in chunk_markdown(input) {
            assert!(
                !chunk.starts_with(char::is_alphanumeric) || input.contains(chunk.trim_start()),
                "chunk `{chunk}` appears to split mid-word"
            );
        }
    }

    #[test]
    fn emits_a_table_block_whole() {
        let input = "Summary text.\n\n|a|b|\n|---|---|\n|1|2|\n|3|4|\n\nTrailing text.";
        let chunks = chunk_markdown(input);
        let table_chunks: Vec<&String> = chunks.iter().filter(|c| c.contains('|')).collect();
        assert_eq!(
            table_chunks.len(),
            1,
            "a table must arrive whole, not row by row: {chunks:?}"
        );
        assert_lossless(input);
    }

    #[test]
    fn emits_a_code_fence_whole() {
        let input = "Before.\n\n```sql\nSELECT 1;\nSELECT 2;\n```\n\nAfter.";
        let chunks = chunk_markdown(input);
        let fenced: Vec<&String> = chunks.iter().filter(|c| c.contains("```")).collect();
        assert_eq!(
            fenced.len(),
            1,
            "a code fence must arrive whole: {chunks:?}"
        );
        assert_lossless(input);
    }
}
