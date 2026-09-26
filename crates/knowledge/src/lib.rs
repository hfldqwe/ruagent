//! ruagent-knowledge: the vector-backed knowledge base (design §6, §19).
//!
//! Hybrid retrieval with zero LLM at query time (design §6.6 #3):
//! LanceDB ANN (semantic leg) + SQLite FTS5 (keyword leg) fused with
//! Reciprocal Rank Fusion. The embedder is a trait — a deterministic
//! hash embedder works fully offline (tests, CI, first boot); fastembed
//! brings real semantics when its model is available.

pub mod chunk;
pub mod embed;
pub mod fast;
pub mod files;
pub mod rrf;
pub mod store;

pub use chunk::{chunk_sections, chunk_text};
pub use embed::{Embedder, HashEmbedder};
pub use fast::FastEmbedder;
pub use files::{ChunkRevision, EditOutcome, Expansion, ScanReport};
pub use rrf::rrf;
pub use store::{Knowledge, KnowledgeError, LegHit, SearchHit, SearchLegs};

/// Deterministic hashing for the offline embedder / dedup.
pub(crate) fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// Content hash for the markdown documents (change detection across
/// scans; same scheme the memory crate uses for episodes). Public for
/// the wiki builder (page-hash bookkeeping, §13-3).
pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}
