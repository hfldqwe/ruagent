# M3 #21 part 2: fastembed embedder + injection into run prompts
# run: python .patch_m3_part2.py

# ---- 1. FastEmbedder in the knowledge crate ----
p = 'crates/knowledge/src/lib.rs'
s = open(p, encoding='utf-8').read()
if 'FastEmbedder' not in s:
    s = s.replace('pub use embed::{Embedder, HashEmbedder};',
                  'pub use embed::{Embedder, HashEmbedder};\npub use fast::FastEmbedder;')
    s = s.replace('pub mod embed;', 'pub mod embed;\npub mod fast;')
    open(p, 'w', encoding='utf-8').write(s)
    print("lib exports FastEmbedder")

# ---- 2. fast.rs ----
fast_rs = '''//! The fastembed-backed embedder (real semantics). Constructed on demand;
//! its ONNX model downloads from HuggingFace on first use, so callers
//! must handle failure (the daemon falls back to the hash embedder).

use crate::embed::{EmbedError, Embedder};

/// BAAI/bge-small-en-v1.5 (384 dims) — small, fast, good enough.
pub struct FastEmbedder {
    model: fastembed::TextEmbedding,
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
        Ok(Self { model })
    }
}

impl Embedder for FastEmbedder {
    fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbedError> {
        // fastembed is sync + CPU-bound: it is called from async contexts
        // but on small batches; the ONNX runtime parallelizes internally.
        self.model
            .embed(texts.to_vec())
            .map_err(|e| EmbedError::Other(format!("embed: {e}")))
    }

    fn name(&self) -> &'static str {
        "fastembed:bge-small-en-v1.5"
    }

    fn dim(&self) -> usize {
        384
    }
}
'''
open('crates/knowledge/src/fast.rs', 'w', encoding='utf-8').write(fast_rs)
print("fast.rs written")

# knowledge Cargo: fastembed + tokio blocking
p = 'crates/knowledge/Cargo.toml'
s = open(p, encoding='utf-8').read()
if 'fastembed' not in s:
    s = s.replace('tracing = "0.1"', 'tracing = "0.1"\nfastembed = "6"')
    s = s.replace('tokio = { version = "1", features = ["rt", "sync"] }',
                  'tokio = { version = "1", features = ["rt", "sync"] }')
    open(p, 'w', encoding='utf-8').write(s)
    print("knowledge deps")

# ---- 3. injection into run prompts (runs.rs) ----
p = 'crates/daemon/src/runs.rs'
s = open(p, encoding='utf-8').read()

# start_run: build the injection before spawning the supervisor
s = s.replace('''        run.status = RunStatus::Spawning;
        run.updated_at = chrono::Utc::now();
        self.db.insert_run(&run).await?;''',
'''        // Push path of the injection contract (design SS6.4): bounded
        // tagged memory blocks prepended to the prompt; the render is
        // also emitted as a ContextInjected event for observability.
        let injection = render_run_injection(&self.db, task).await;
        let prompt = if injection.is_empty() {
            prompt
        } else {
            format!("{injection}\\n---\\n{prompt}")
        };

        run.status = RunStatus::Spawning;
        run.updated_at = chrono::Utc::now();
        self.db.insert_run(&run).await?;''')

# pass injection into supervise
s = s.replace('''                mcp_servers,
                cwd,
                routed,
            )
            .await''',
'''                mcp_servers,
                cwd,
                routed,
                injection,
            )
            .await''')

# supervise: signature + emit ContextInjected
s = s.replace('''    cwd: PathBuf,
    routed: Option<RoutingDecision>,
) -> Result<()> {''',
'''    cwd: PathBuf,
    routed: Option<RoutingDecision>,
    injection: String,
) -> Result<()> {''')
s = s.replace('''    if let Some(decision) = routed {
        emit(&mut transcript, &broadcast, &run, RunEvent::Routed { decision });
    }''',
'''    if let Some(decision) = routed {
        emit(&mut transcript, &broadcast, &run, RunEvent::Routed { decision });
    }
    if !injection.is_empty() {
        emit(
            &mut transcript,
            &broadcast,
            &run,
            RunEvent::ContextInjected { render: injection.clone() },
        );
    }''')

# helper at the end (before tests module)
helper = '''
/// Assemble the push-path injection for a run (design SS6.4): user
/// profile + user observations, bounded by the default budget.
async fn render_run_injection(db: &Db, task: &Task) -> String {
    use ruagent_memory::{
        current_memories, render_injection, InjectionBudget, MemoryForInjection, MemoryStore,
    };
    let mut mems: Vec<MemoryForInjection> = Vec::new();
    if let Ok(profile) = current_memories(db, MemoryStore::Profile, "user", 5).await {
        mems.extend(profile.into_iter().map(|m| MemoryForInjection {
            tag: "user_profile",
            content: m.content,
            updated_at: m.updated_at,
        }));
    }
    if let Ok(obs) = current_memories(db, MemoryStore::Observation, "user", 8).await {
        mems.extend(obs.into_iter().map(|m| MemoryForInjection {
            tag: "relevant_memories",
            content: m.content,
            updated_at: m.updated_at,
        }));
    }
    if let Some(project) = &task.project {
        if let Ok(obs) = current_memories(
            db,
            MemoryStore::Observation,
            &format!("project:{project}"),
            8,
        )
        .await
        {
            mems.extend(obs.into_iter().map(|m| MemoryForInjection {
                tag: "project_context",
                content: m.content,
                updated_at: m.updated_at,
            }));
        }
    }
    if mems.is_empty() {
        return String::new();
    }
    render_injection(&mems, &InjectionBudget::default())
}
'''
anchor = '#[cfg(test)]\nmod tests {'
assert anchor in s
s = s.replace(anchor, helper + '\n' + anchor)
open(p, 'w', encoding='utf-8').write(s)
print("injection wired")
