//! Chunking: paragraph-aware packing into ~target-size chunks with
//! overlap (design §6 — simple, deterministic; heading-aware splitting
//! can refine later without changing the interface).
//!
//! Span-aware variant (`chunk_sections`): every chunk and section also
//! carries its byte range in the source text, so a chunk edit can be
//! written back into the source `.md` file (markdown is the source of
//! truth, design-study memsearch/EverOS) and a hit can be expanded into
//! its parent section (WeKnora parent-child retrieval).

const DEFAULT_TARGET: usize = 800;
const DEFAULT_OVERLAP: usize = 100;

/// One chunk with its byte span in the source document.
#[derive(Debug, Clone, PartialEq)]
pub struct ChunkSpan {
    pub content: String,
    /// Byte offset of the chunk's first character in the source.
    pub start: usize,
    /// Byte offset one past the chunk's last character.
    pub end: usize,
}

/// One section (a heading and its paragraphs, or the preamble before
/// the first heading) with its chunks. Sections never share chunks, and
/// the section is the PARENT block returned for context-complete
/// retrieval hits (WeKnora parent-child chunking).
#[derive(Debug, Clone, PartialEq)]
pub struct Section {
    /// The section text as it appears in the source (slice [start, end)).
    pub content: String,
    pub start: usize,
    pub end: usize,
    pub chunks: Vec<ChunkSpan>,
}

/// Split `text` into chunks. Heading-aware: a markdown heading starts a
/// new section, and sections never share a chunk — a chunk about
/// Kubernetes must not average in the pasta recipe above it (semantic
/// dilution measured 2026-09-13: a mixed chunk ranked BELOW an
/// unrelated one for a topic it contained).
pub fn chunk_text(text: &str) -> Vec<String> {
    chunk_with(text, DEFAULT_TARGET, DEFAULT_OVERLAP)
}

pub fn chunk_with(text: &str, target: usize, overlap: usize) -> Vec<String> {
    chunk_sections_with(text, target, overlap)
        .into_iter()
        .flat_map(|s| s.chunks.into_iter().map(|c| c.content))
        .collect()
}

/// Span-aware chunking: the same chunks `chunk_text` produces, grouped
/// into sections with byte spans into `text`.
pub fn chunk_sections(text: &str) -> Vec<Section> {
    chunk_sections_with(text, DEFAULT_TARGET, DEFAULT_OVERLAP)
}

pub fn chunk_sections_with(text: &str, target: usize, overlap: usize) -> Vec<Section> {
    // Group paragraphs into sections: a heading paragraph (# ...) starts
    // a new section; paragraphs until the next heading belong to it.
    let mut sections: Vec<Vec<Para>> = Vec::new();
    for para in paras(text) {
        if para.is_heading || sections.is_empty() {
            sections.push(Vec::new());
        }
        sections.last_mut().expect("just pushed").push(para);
    }

    // Pack each section independently so topics never mix.
    sections
        .into_iter()
        .map(|paras| {
            let start = paras.first().map(|p| p.start).unwrap_or(0);
            let end = paras.last().map(|p| p.end).unwrap_or(0);
            let chunks = pack_pieces(paras, target, overlap);
            Section {
                content: text[start..end].to_string(),
                start,
                end,
                chunks,
            }
        })
        .collect()
}

/// A paragraph, trimmed, with its byte span in the source.
struct Para {
    text: String,
    start: usize,
    end: usize,
    is_heading: bool,
}

/// Split on blank lines (`\n\n`), like the original chunker, keeping
/// byte spans of the trimmed paragraphs.
fn paras(text: &str) -> Vec<Para> {
    let mut out = Vec::new();
    let mut cursor = 0usize;
    for raw in text.split("\n\n") {
        let seg_start = cursor;
        cursor = cursor + raw.len() + 2; // + separator (harmless past the end)
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        let lead = raw.len() - raw.trim_start().len();
        let start = seg_start + lead;
        out.push(Para {
            text: trimmed.to_string(),
            start,
            end: start + trimmed.len(),
            is_heading: trimmed.starts_with('#'),
        });
    }
    out
}

/// Hard-split over-long pieces, then pack into chunks up to target size.
/// Spans: the chunk spans from its first piece's start to its last
/// piece's end.
fn pack_pieces(input: Vec<Para>, target: usize, overlap: usize) -> Vec<ChunkSpan> {
    let mut pieces: Vec<Para> = Vec::new();
    for para in input {
        if para.text.chars().count() <= target {
            pieces.push(para);
        } else {
            // Hard-split long paragraphs at target boundaries (char
            // space, like the original; converted to bytes at the end).
            let byte_of_char: Vec<usize> = para.text.char_indices().map(|(b, _)| b).collect();
            let byte = |ci: usize| byte_of_char.get(ci).copied().unwrap_or(para.text.len());
            let mut start = 0;
            loop {
                let end = (start + target).min(byte_of_char.len());
                pieces.push(Para {
                    text: para.text[byte(start)..byte(end)].to_string(),
                    start: para.start + byte(start),
                    end: para.start + byte(end),
                    is_heading: false,
                });
                if end == byte_of_char.len() {
                    break; // reached the tail - stop (no cascade of crumbs)
                }
                start = end.saturating_sub(overlap).max(start + 1);
            }
        }
    }

    let mut chunks: Vec<ChunkSpan> = Vec::new();
    let mut current: Option<Para> = None;
    for piece in pieces {
        let push_needed = match &current {
            Some(c) => c.text.chars().count() + piece.text.chars().count() + 2 > target,
            None => false,
        };
        if push_needed && let Some(c) = current.take() {
            chunks.push(ChunkSpan {
                content: c.text,
                start: c.start,
                end: c.end,
            });
        }
        current = Some(match current {
            Some(mut c) => {
                c.text.push_str("\n\n");
                c.text.push_str(&piece.text);
                c.end = piece.end;
                c
            }
            None => piece,
        });
    }
    if let Some(c) = current {
        chunks.push(ChunkSpan {
            content: c.text,
            start: c.start,
            end: c.end,
        });
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
    fn headings_start_new_chunks_no_topic_mixing() {
        let text = "# Cooking pasta\n\nBoil water and cook spaghetti.\n\n# Kubernetes\n\nRollback uses kubectl rollout undo.";
        let chunks = chunk_text(text);
        assert_eq!(chunks.len(), 2, "each heading = its own chunk");
        assert!(chunks[0].contains("spaghetti") && !chunks[0].contains("kubectl"));
        assert!(chunks[1].contains("kubectl") && !chunks[1].contains("spaghetti"));
    }

    #[test]
    fn empty_and_whitespace() {
        assert!(chunk_text("").is_empty());
        assert!(chunk_text("  \n\n  ").is_empty());
        assert_eq!(chunk_text("single"), vec!["single".to_string()]);
    }

    #[test]
    fn span_version_matches_plain_chunking() {
        let text =
            "# Heading one\n\nalpha paragraph\n\nbeta paragraph\n\n## Sub\n\ngamma\n\n# Two\n\n"
                .to_string();
        assert_eq!(
            chunk_text(&text),
            chunk_sections(&text)
                .into_iter()
                .flat_map(|s| s.chunks.into_iter().map(|c| c.content))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn section_content_is_source_slice_starting_at_heading() {
        let text = "preamble para\n\n# Cooking pasta\n\nBoil water.\n\n# Kubernetes\n\nRollback via kubectl.";
        let sections = chunk_sections(text);
        assert_eq!(sections.len(), 3);
        assert_eq!(sections[0].content, "preamble para");
        assert!(sections[1].content.starts_with("# Cooking pasta"));
        assert!(sections[1].content.contains("Boil water."));
        assert!(sections[2].content.starts_with("# Kubernetes"));
        for s in &sections {
            assert_eq!(
                &text[s.start..s.end],
                s.content,
                "content must be the slice"
            );
        }
    }

    #[test]
    fn chunk_span_replacement_is_identity_on_normalized_text() {
        // Paragraphs already trimmed, separated by exactly one blank
        // line: replacing each chunk's span with its own content must
        // reproduce the text byte for byte (the invariant chunk edits
        // rely on).
        let text = "# A\n\npara one under A\n\nstill A\n\n# B\n\npara under B";
        let sections = chunk_sections(text);
        let mut out = text.to_string();
        for s in &sections {
            for c in &s.chunks {
                out.replace_range(c.start..c.end, &c.content);
            }
        }
        assert_eq!(out, text);
    }

    #[test]
    fn hard_split_spans_stay_on_char_boundaries() {
        // Multi-byte chars force non-trivial byte offsets.
        let long = "部署脚本 ".repeat(300); // one huge paragraph
        let text = format!("# 标题\n\n{long}");
        let sections = chunk_sections(&text);
        assert!(sections[0].chunks.len() >= 2, "must hard-split");
        assert_eq!(
            sections[0].chunks[0].content, "# 标题",
            "heading packs alone"
        );
        for c in &sections[0].chunks[1..] {
            let slice = &text[c.start..c.end]; // panics if not a boundary
            assert!(slice.contains("部署") || slice.contains("脚本"));
        }
        // Replacing every span with its content keeps every byte covered
        // (overlap chunks cover the union).
        let mut out = text.clone();
        for c in &sections[0].chunks {
            out.replace_range(c.start..c.end, &c.content);
        }
        assert!(out.starts_with("# 标题"));
    }

    #[test]
    fn consecutive_blank_lines_and_trailing_whitespace() {
        let text = "  padded  \n\n\n\n  second  \n\n";
        let sections = chunk_sections(text);
        assert_eq!(sections.len(), 1);
        let c = &sections[0].chunks[0];
        assert_eq!(c.content, "padded\n\nsecond");
        assert!(c.start >= 2 && c.end <= text.len());
    }
}
