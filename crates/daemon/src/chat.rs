//! ChatManager: terminal-like persistent conversations over ACP. One
//! spawned agent process per chat, multi-turn prompts on one session,
//! model switching (restarts the session, history preserved in the chat
//! transcript).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use chrono::Utc;
use ruagent_acp::adapter::HarnessAdapter as _;
use ruagent_acp::chat::{ChatCommand, ChatOptions, ChatSession, start_chat};
use ruagent_core::{AgentCard, RunEvent, RunId};
use ruagent_store::{TranscriptWriter, transcript_path};
use tokio::sync::mpsc;

/// One live chat.
#[derive(Clone)]
pub struct Chat {
    pub id: RunId,
    pub agent: String,
    pub model: Option<String>,
    pub created_at: String,
    session: ChatSession,
    last_active: Arc<Mutex<Instant>>,
}

impl Chat {
    pub fn send(&self, cmd: ChatCommand) -> Result<()> {
        *self.last_active.lock().expect("chat last_active lock") = Instant::now();
        self.session.send(cmd).context("sending chat command")
    }

    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<RunEvent> {
        self.session.subscribe()
    }
}

/// The chat registry + idle reaper.
type AskParker = Arc<dyn Fn(ruagent_acp::permission::PermissionAsk, RunId) + Send + Sync>;

pub struct ChatManager {
    db: ruagent_store::Db,
    root: PathBuf,
    chats: Arc<Mutex<HashMap<RunId, Chat>>>,
    park_ask: AskParker,
    mcp: crate::config::McpConfig,
}

const IDLE_TIMEOUT: Duration = Duration::from_secs(15 * 60);

impl ChatManager {
    pub fn new(
        db: ruagent_store::Db,
        root: PathBuf,
        park_ask: AskParker,
        mcp: crate::config::McpConfig,
    ) -> Arc<Self> {
        let this = Arc::new(Self {
            db,
            root,
            chats: Arc::new(Mutex::new(HashMap::new())),
            park_ask,
            mcp,
        });
        this.spawn_idle_reaper();
        this
    }

    pub fn chat(&self, id: RunId) -> Option<Chat> {
        self.chats.lock().expect("chats lock").get(&id).cloned()
    }

    pub fn list(&self) -> Vec<serde_json::Value> {
        self.chats
            .lock()
            .expect("chats lock")
            .values()
            .map(|c| {
                serde_json::json!({
                    "id": c.id.to_string(),
                    "agent": c.agent,
                    "model": c.model,
                    "created_at": c.created_at,
                })
            })
            .collect()
    }

    /// Start a new chat on the given agent (optionally with a model).
    /// Workspace = a fresh dir under workspaces/chat-<id>.
    pub fn start(&self, card: &AgentCard, model: Option<String>) -> Result<Chat> {
        let spec = ruagent_acp::adapter_for(card.harness)
            .spawn_spec(card)
            .with_context(|| format!("resolving spawn command for `{}`", card.name))?;
        let id = RunId::generate();
        let workspace = self.root.join("workspaces").join(format!("chat-{id}"));
        std::fs::create_dir_all(&workspace)
            .with_context(|| format!("creating workspace {}", workspace.display()))?;

        let mcp = self
            .mcp
            .expand_profile(card.mcp_profile.as_deref(), &card.name);

        // Per-chat permission channel: each ask is parked in the shared
        // inbox keyed by this chat's id (rules may auto-answer; otherwise
        // it reaches the human — same inbox the panel serves).
        let (ask_tx, mut ask_rx) =
            mpsc::unbounded_channel::<ruagent_acp::permission::PermissionAsk>();
        {
            let park = self.park_ask.clone();
            tokio::spawn(async move {
                while let Some(ask) = ask_rx.recv().await {
                    park(ask, id);
                }
            });
        }

        let session = start_chat(
            ChatOptions {
                program: spec.program,
                args: spec.args,
                cwd: workspace,
                mcp_servers: mcp,
                model: model.clone(),
            },
            ask_tx,
        )
        .context("starting chat session")?;

        // Transcript forwarder: every chat event lands in the chat's JSONL
        // (SSE replays from it) — matches the run transcript pattern.
        {
            let mut sub = session.subscribe();
            let path = self.transcript_path(id);
            tokio::spawn(async move {
                let mut writer = match TranscriptWriter::create(&path) {
                    Ok(w) => w,
                    Err(e) => {
                        tracing::warn!(chat = %id, error = %e, "chat transcript open failed");
                        return;
                    }
                };
                loop {
                    match sub.recv().await {
                        Ok(event) => {
                            let _ = writer.append(&event);
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            tracing::warn!(chat = %id, skipped = n, "chat forwarder lagged");
                        }
                        Err(_) => {
                            // Sender dropped: chat closed.
                            let _ = writer.flush();
                            return;
                        }
                    }
                }
            });
        }

        let chat = Chat {
            id,
            agent: card.name.clone(),
            model,
            created_at: Utc::now().to_rfc3339(),
            session,
            last_active: Arc::new(Mutex::new(Instant::now())),
        };
        self.chats
            .lock()
            .expect("chats lock")
            .insert(id, chat.clone());
        Ok(chat)
    }

    /// Close and remove a chat.
    pub fn close(&self, id: RunId) -> bool {
        let chat = self.chats.lock().expect("chats lock").remove(&id);
        match chat {
            Some(c) => {
                let _ = c.send(ChatCommand::Shutdown);
                true
            }
            None => false,
        }
    }

    /// Switch model: closes the current session and starts a fresh one on
    /// the same agent. The visible history stays in the transcript (but
    /// context resets — the new session starts blank).
    pub fn switch_model(&self, id: RunId, model: Option<String>, card: &AgentCard) -> Result<Chat> {
        self.close(id);
        let chat = self.start(card, model)?;
        Ok(chat)
    }

    /// Chat transcripts live next to run transcripts.
    pub fn transcript_path(&self, id: RunId) -> PathBuf {
        transcript_path(self.root.join("data").join("transcripts"), &id)
    }

    pub fn db(&self) -> &ruagent_store::Db {
        &self.db
    }

    fn spawn_idle_reaper(self: &Arc<Self>) {
        let chats = self.chats.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(60)).await;
                let mut to_close = Vec::new();
                {
                    let map = chats.lock().expect("chats lock");
                    for (id, c) in map.iter() {
                        let idle = c.last_active.lock().expect("last_active").elapsed();
                        if idle > IDLE_TIMEOUT {
                            tracing::info!(chat = %id, "chat idle, closing");
                            to_close.push(*id);
                        }
                    }
                }
                for id in to_close {
                    let chat = {
                        let mut map = chats.lock().expect("chats lock");
                        map.remove(&id)
                    };
                    if let Some(c) = chat {
                        let _ = c.send(ChatCommand::Shutdown);
                    }
                }
            }
        });
    }
}
