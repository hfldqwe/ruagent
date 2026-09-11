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
pub mod rrf;
pub mod store;

pub use chunk::chunk_text;
pub use embed::{Embedder, HashEmbedder};
pub use fast::FastEmbedder;
pub use rrf::rrf;
pub use store::{Knowledge, KnowledgeError, SearchHit};

/// Deterministic hashing for the offline embedder / dedup.
pub(crate) fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}
