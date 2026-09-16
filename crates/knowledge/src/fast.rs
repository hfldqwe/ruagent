//! The fastembed-backed embedder (real semantics). Constructed on demand;
//! its ONNX model downloads from HuggingFace on first use, so callers
//! must handle failure (the daemon falls back to the hash embedder).

use std::sync::Mutex;

use crate::embed::{EmbedError, Embedder};

/// intfloat/multilingual-e5-small (384 dims) — zh/en quality on CPU.
/// (bge-small-en-v1.5, the previous model, left Chinese queries with
/// near-random ANN neighbors — recall-log finding, 2026-09-16.)
/// E5 convention: passages and queries get DISTINCT prefixes, which is
/// why the trait grew `embed_query`.
/// `TextEmbedding::embed` needs `&mut self`, hence the Mutex.
pub struct FastEmbedder {
    model: Mutex<fastembed::TextEmbedding>,
}

impl FastEmbedder {
    pub async fn try_new() -> Result<Self, EmbedError> {
        // Model load can take a while (download + init): run it on the
        // blocking pool.
        let model = tokio::task::spawn_blocking(|| {
            // Explicit model: fastembed's derived defaults have drifted
            // before (6.0 went to all-MiniLM-L6-v2); we pin
            // multilingual-e5-small — the identity stored with the
            // vector table.
            let opts = fastembed::InitOptions::new(fastembed::EmbeddingModel::MultilingualE5Small)
                .with_show_download_progress(false);
            fastembed::TextEmbedding::try_new(opts)
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
        // E5 passages carry the "passage: " prefix.
        let passages: Vec<String> = texts.iter().map(|t| format!("passage: {t}")).collect();
        let refs: Vec<&str> = passages.iter().map(String::as_str).collect();
        // fastembed is sync + CPU-bound: it is called from async contexts
        // but on small batches; the ONNX runtime parallelizes internally.
        let mut model = self
            .model
            .lock()
            .map_err(|_| EmbedError::Other("embedder lock poisoned".into()))?;
        model
            .embed(&refs, None)
            .map_err(|e| EmbedError::Other(format!("embed: {e}")))
    }

    fn embed_query(&self, text: &str) -> Result<Vec<f32>, EmbedError> {
        // E5 queries carry the "query: " prefix — a different vector
        // from the same text as a passage.
        let q = format!("query: {text}");
        let mut model = self
            .model
            .lock()
            .map_err(|_| EmbedError::Other("embedder lock poisoned".into()))?;
        let mut out = model
            .embed([q.as_str()], None)
            .map_err(|e| EmbedError::Other(format!("embed: {e}")))?;
        if out.len() != 1 {
            return Err(EmbedError::Other(format!(
                "expected 1 query vector, got {}",
                out.len()
            )));
        }
        Ok(out.remove(0))
    }

    fn name(&self) -> &'static str {
        "fastembed:multilingual-e5-small"
    }

    fn dim(&self) -> usize {
        384
    }
}
