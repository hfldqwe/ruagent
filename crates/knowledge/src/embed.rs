//! Embedders: the trait + the deterministic offline implementation.
//! The fastembed-backed implementation lives in `fast.rs` and is loaded
//! on demand (its model downloads from HuggingFace; failure falls back
//! to the hash embedder — the platform must boot offline, design §6.3).

/// Anything that can turn texts into fixed-dim vectors.
pub trait Embedder: Send + Sync {
    /// Documents/passages.
    fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbedError>;
    /// Queries. E5-style models need distinct prefixes for queries and
    /// passages; the default shares the document path (prefix-free
    /// models).
    fn embed_query(&self, text: &str) -> Result<Vec<f32>, EmbedError> {
        Ok(self.embed(&[text])?.remove(0))
    }
    /// The offline fallback embedder. A model mismatch at open must
    /// never migrate the vector table TO the fallback (an offline boot
    /// would destroy real vectors); only a real model switch migrates.
    fn is_fallback(&self) -> bool {
        false
    }
    /// Stable model identifier — stored with the vector table; changing
    /// it requires re-embedding everything (LightRAG's lesson).
    fn name(&self) -> &'static str;
    fn dim(&self) -> usize;
}

#[derive(Debug, thiserror::Error)]
pub enum EmbedError {
    #[error("embedding failed: {0}")]
    Other(String),
}

/// Deterministic bag-of-words hashing: each word hashes into one
/// dimension; the vector is L2-normalized. No semantics, but shared
/// words → high cosine similarity — enough for offline tests and as a
/// boot-time fallback before any model downloads.
pub struct HashEmbedder {
    dim: usize,
}

impl HashEmbedder {
    pub fn new(dim: usize) -> Self {
        Self {
            dim: usize::max(dim, 8),
        }
    }
}

impl Default for HashEmbedder {
    fn default() -> Self {
        Self::new(256)
    }
}

impl Embedder for HashEmbedder {
    fn is_fallback(&self) -> bool {
        true
    }

    fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbedError> {
        Ok(texts
            .iter()
            .map(|t| {
                let mut v = vec![0f32; self.dim];
                for word in t.split_whitespace() {
                    let word: String = word
                        .chars()
                        .filter(|c| c.is_alphanumeric())
                        .collect::<String>()
                        .to_lowercase();
                    if word.is_empty() {
                        continue;
                    }
                    let idx = (crate::fnv1a(word.as_bytes()) % self.dim as u64) as usize;
                    v[idx] += 1.0;
                }
                let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
                if norm > 0.0 {
                    for x in &mut v {
                        *x /= norm;
                    }
                }
                v
            })
            .collect())
    }

    fn name(&self) -> &'static str {
        "hash-embedder"
    }

    fn dim(&self) -> usize {
        self.dim
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_words_mean_similarity() {
        let e = HashEmbedder::new(128);
        let mut v = e
            .embed(&["deploy the release script", "deploy the release branch"])
            .unwrap();
        let a = v.remove(0);
        let b = v.remove(0);
        let dot: f32 = a.iter().zip(&b).map(|(x, y)| x * y).sum();
        assert!(dot > 0.3, "shared 3/4 words: cosine = {dot}");

        let mut v = e
            .embed(&["deploy the release script", "kittens playing pianos"])
            .unwrap();
        let a = v.remove(0);
        let b = v.remove(0);
        let dot: f32 = a.iter().zip(&b).map(|(x, y)| x * y).sum();
        assert!(dot < 0.05, "disjoint words: cosine = {dot}");
    }

    #[test]
    fn deterministic_and_normalized() {
        let e = HashEmbedder::new(64);
        let a = e.embed(&["hello world hello"]).unwrap().remove(0);
        let b = e.embed(&["hello world hello"]).unwrap().remove(0);
        assert_eq!(a, b);
        let norm: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-5, "L2 normalized");
    }
}
