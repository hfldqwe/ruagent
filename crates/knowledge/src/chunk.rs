//! Chunking: paragraph-aware packing into ~target-size chunks with
//! overlap (design §6 — simple, deterministic; heading-aware splitting
//! can refine later without changing the interface).

const DEFAULT_TARGET: usize = 800;
const DEFAULT_OVERLAP: usize = 100;

/// Split `text` into chunks: paragraphs first, then hard-splits any
/// paragraph longer than the target. Consecutive chunks share
/// `overlap` characters of tail/head.
pub fn chunk_text(text: &str) -> Vec<String> {
    chunk_with(text, DEFAULT_TARGET, DEFAULT_OVERLAP)
}

pub fn chunk_with(text: &str, target: usize, overlap: usize) -> Vec<String> {
    let mut pieces: Vec<String> = Vec::new();
    for para in text.split("\n\n") {
        let para = para.trim();
        if para.is_empty() {
            continue;
        }
        if para.chars().count() <= target {
            pieces.push(para.to_string());
        } else {
            // Hard-split long paragraphs at target boundaries.
            let chars: Vec<char> = para.chars().collect();
            let mut start = 0;
            while start < chars.len() {
                let end = (start + target).min(chars.len());
                pieces.push(chars[start..end].iter().collect());
                if end == chars.len() {
                    break; // reached the tail — stop (no cascade of crumbs)
                }
                start = end.saturating_sub(overlap).max(start + 1);
            }
        }
    }

    // Pack pieces into chunks up to target size.
    let mut chunks: Vec<String> = Vec::new();
    let mut current = String::new();
    for piece in pieces {
        if !current.is_empty() && current.chars().count() + piece.chars().count() + 2 > target {
            chunks.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push_str("\n\n");
        }
        current.push_str(&piece);
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paragraphs_pack_to_target() {
        let text = "alpha one\n\nbeta two\n\ngamma three";
        let chunks = chunk_text(text);
        assert_eq!(chunks.len(), 1, "short paragraphs pack together");
        assert!(chunks[0].contains("alpha") && chunks[0].contains("gamma"));
    }

    #[test]
    fn long_text_splits_with_overlap() {
        let text = (0..200)
            .map(|i| format!("word{i}"))
            .collect::<Vec<_>>()
            .join(" ");
        let chunks = chunk_text(&text);
        assert!(chunks.len() >= 2, "must split");
        for w in chunks.windows(2) {
            // The successor's head must be a substring of its
            // predecessor — that IS the overlap.
            let head: String = w[1].chars().take(90).collect();
            let tail: String = w[0]
                .chars()
                .rev()
                .take(60)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            assert!(
                w[0].contains(&head),
                "chunks must overlap: ...{tail:?} / {head:?}..."
            );
        }
    }

    #[test]
    fn empty_and_whitespace() {
        assert!(chunk_text("").is_empty());
        assert!(chunk_text("  \n\n  ").is_empty());
        assert_eq!(chunk_text("single"), vec!["single".to_string()]);
    }
}
