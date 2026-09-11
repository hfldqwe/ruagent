//! The fastembed-backed embedder (real semantics). Constructed on demand;
//! its ONNX model downloads from HuggingFace on first use, so callers
//! must handle failure (the daemon falls back to the hash embedder).

use std::sync::Mutex;

use crate::embed::{EmbedError, Embedder};

/// BAAI/bge-small-en-v1.5 (384 dims) — small, fast, good enough.
/// `TextEmbedding::embed` needs `&mut self`, hence the Mutex.
pub struct FastEmbedder {
    model: Mutex<fastembed::TextEmbedding>,
}

impl FastEmbedder {
    pub async fn try_new() -> Result<Self, EmbedError> {
        // Model load can take a while (download + init): run it on the
        // blocking pool.
        let model = tokio::task::spawn_blocking(|| {
            fastembed::TextEmbedding::try_new(Default::default())
                .map_err(|e| EmbedError::Other(format!("fastembed init: {e}")))
        })
        .await
        .map_err(|e| EmbedError::Other(format!("join: {e}")))??;
        Ok(Self {
            model: Mutex::new(model),
        })
    }
}

impl Embedder for FastEmbedder {
    fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbedError> {
        // fastembed is sync + CPU-bound: it is called from async contexts
        // but on small batches; the ONNX runtime parallelizes internally.
        let mut model = self
            .model
            .lock()
            .map_err(|_| EmbedError::Other("embedder lock poisoned".into()))?;
        model
            .embed(texts, None)
            .map_err(|e| EmbedError::Other(format!("embed: {e}")))
    }

    fn name(&self) -> &'static str {
        "fastembed:bge-small-en-v1.5"
    }

    fn dim(&self) -> usize {
        384
    }
}
