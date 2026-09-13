//! Unified session history: auto-synced from every agent CLI's local
//! store — claude-code transcripts, dsh sessions, ruagent's own chats.
//! The indexer runs at boot and rescans every 60s (metadata-incremental:
//! a file is re-parsed only when its mtime/size changed). Message bodies
//! stay in the source files; the index carries metadata + preview, and
//! the API parses turns on demand.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::Duration;

use ruagent_store::Db;
use serde::Serialize;

const RESCAN: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Serialize)]
pub struct SessionRecord {
    pub key: String,
    pub source: String,
    pub title: Option<String>,
    pub project: Option<String>,
    pub ref_path: String,
    pub started_at: i64,
    pub updated_at: i64,
    pub message_count: u32,
    pub preview: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionMessage {
    pub role: String, // user | assistant
    pub text: String,
    pub ts: i64,
}

/// Parsed index facts for one source file.
#[derive(Debug, Default)]
struct Parsed {
    title: Option<String>,
    project: Option<String>,
    started_at: i64,
    updated_at: i64,
    message_count: u32,
    preview: Option<String>,
    messages: Vec<SessionMessage>, // only when parsing for the viewer
}

pub struct SessionIndexer {
    db: Db,
    home: PathBuf,
}

impl SessionIndexer {
    pub fn new(db: Db, home: PathBuf) -> Self {
        Self { db, home }
    }

    /// The auto-sync loop: scan now, then every 60s. Cheap when nothing
    /// changed (stat-only). Run it on a spawned task.
    pub async fn run(&self) {
        loop {
            if let Err(e) = self.scan().await {
                tracing::warn!(error = %e, "session index scan failed");
            }
            tokio::time::sleep(RESCAN).await;
        }
    }

    pub async fn scan(&self) -> Result<(), ruagent_store::DbError> {
        let mut files: Vec<(String, PathBuf)> = Vec::new();

        // claude-code: ~/.claude/projects/<encoded-cwd>/<session>.jsonl
        let cc = self.home.join(".claude").join("projects");
        if let Some(project_dirs) = read_dirs(&cc) {
            for pd in project_dirs {
                for f in read_files(&pd, "jsonl") {
                    files.push(("claude-code".into(), f));
                }
            }
        }

        // dsh: ~/.dsh/sessions/<encoded-cwd>/<session-id>/session.v3.jsonl.zstd
        let dsh = self.home.join(".dsh").join("sessions");
        if let Some(ws_dirs) = read_dirs(&dsh) {
            for wd in ws_dirs {
                for sd in read_dirs(&wd).unwrap_or_default() {
                    for f in read_files(&sd, "zstd") {
                        files.push(("dsh".into(), f));
                    }
                }
            }
        }

        // ruagent: ~/.ruagent/data/transcripts/run-*.jsonl
        let tr = self.home.join(".ruagent").join("data").join("transcripts");
        for f in read_files(&tr, "jsonl") {
            files.push(("ruagent".into(), f));
        }

        for (source, path) in files {
            self.index_file(&source, &path).await?;
        }
        Ok(())
    }

    /// Index one file; skips parsing when mtime/size are unchanged.
    async fn index_file(&self, source: &str, path: &Path) -> Result<(), ruagent_store::DbError> {
        let meta = match std::fs::metadata(path) {
            Ok(m) => m,
            Err(_) => return Ok(()), // raced with deletion
        };
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        let size = meta.len() as i64;
        let key = make_key(source, path);

        // Unchanged? skip.
        let known: Option<(i64, i64)> = self
            .db
            .call({
                let key = key.clone();
                move |conn| {
                    conn.query_row(
                        "SELECT mtime_ms, size_bytes FROM sessions WHERE key = ?1",
                        [&key],
                        |r| Ok((r.get(0)?, r.get(1)?)),
                    )
                    .ok()
                }
            })
            .await?;
        if known == Some((mtime, size)) {
            return Ok(());
        }

        let bytes = std::fs::read(path).unwrap_or_default();
        let text = String::from_utf8_lossy(&bytes).into_owned();
        let parsed = match source {
            "claude-code" => parse_claude_code(&text),
            "dsh" => parse_dsh(&bytes),
            "ruagent" => parse_ruagent(&text),
            _ => Parsed::default(),
        };
        let ref_path = path.to_string_lossy().into_owned();

        let source = source.to_string();
        self.db
            .call(move |conn| {
                conn.execute(
                    "INSERT OR REPLACE INTO sessions
                         (key, source, title, project, ref_path, started_at, updated_at,
                          mtime_ms, size_bytes, message_count, preview)
                     VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
                    rusqlite::params![
                        key,
                        source,
                        parsed.title,
                        parsed.project,
                        ref_path,
                        parsed.started_at,
                        parsed.updated_at,
                        mtime,
                        size,
                        parsed.message_count,
                        parsed.preview,
                    ],
                )
            })
            .await??;
        Ok(())
    }

    /// The index, newest first.
    pub async fn list(&self, limit: u32) -> Result<Vec<SessionRecord>, ruagent_store::DbError> {
        self.db
            .call(move |conn| {
                let mut stmt = conn
                    .prepare(
                        "SELECT key, source, title, project, ref_path, started_at, updated_at,
                                message_count, preview
                         FROM sessions ORDER BY updated_at DESC LIMIT ?1",
                    )
                    .map_err(ruagent_store::DbError::from)?;
                let rows = stmt.query_map([limit], |r| {
                    Ok(SessionRecord {
                        key: r.get(0)?,
                        source: r.get(1)?,
                        title: r.get(2)?,
                        project: r.get(3)?,
                        ref_path: r.get(4)?,
                        started_at: r.get(5)?,
                        updated_at: r.get(6)?,
                        message_count: r.get(7)?,
                        preview: r.get(8)?,
                    })
                })?;
                let mut out = Vec::new();
                for r in rows {
                    if let Ok(rec) = r {
                        out.push(rec);
                    }
                }
                Ok(out)
            })
            .await?
    }

    /// Load one session's turns, parsed from the source file on demand.
    pub async fn messages(&self, key: &str) -> Result<Vec<SessionMessage>, ruagent_store::DbError> {
        let key = key.to_string();
        let (source, ref_path): (String, String) = self
            .db
            .call(move |conn| {
                conn.query_row(
                    "SELECT source, ref_path FROM sessions WHERE key = ?1",
                    [key],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
            })
            .await??;
        let path = PathBuf::from(&ref_path);
        let parsed = match source.as_str() {
            "claude-code" => parse_claude_code(&std::fs::read_to_string(&path).unwrap_or_default()),
            "dsh" => parse_dsh(&std::fs::read(&path).unwrap_or_default()),
            "ruagent" => parse_ruagent(&std::fs::read_to_string(&path).unwrap_or_default()),
            _ => Parsed::default(),
        };
        Ok(parsed.messages)
    }
}

// ---------------------------------------------------------------------------
// Parsers
// ---------------------------------------------------------------------------

fn parse_claude_code(text: &str) -> Parsed {
    let mut p = Parsed::default();
    for line in text.lines() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let ts = v
            .get("timestamp")
            .and_then(|t| t.as_str())
            .and_then(parse_iso_ms)
            .unwrap_or(0);
        match v.get("type").and_then(|t| t.as_str()) {
            Some("user") => {
                if v.get("isSidechain").and_then(|s| s.as_bool()) == Some(true) {
                    continue;
                }
                let msg = v.get("message");
                let content = msg.and_then(|m| m.get("content"));
                // Real user prompts are plain strings; tool_result blocks
                // arrive as lists — skip those.
                if let Some(text) = content.and_then(|c| c.as_str()) {
                    let text = text.trim();
                    if text.is_empty() || text.starts_with('<') {
                        continue; // command meta like <command-name>…
                    }
                    if p.started_at == 0 {
                        p.started_at = ts;
                    }
                    if p.preview.is_none() {
                        p.preview = Some(truncate(text, 140));
                    }
                    p.messages.push(SessionMessage {
                        role: "user".into(),
                        text: text.to_string(),
                        ts,
                    });
                    p.updated_at = p.updated_at.max(ts);
                }
            }
            Some("assistant") => {
                let Some(blocks) = v
                    .get("message")
                    .and_then(|m| m.get("content"))
                    .and_then(|c| c.as_array())
                else {
                    continue;
                };
                let text: String = blocks
                    .iter()
                    .filter_map(|b| {
                        (b.get("type").and_then(|t| t.as_str()) == Some("text"))
                            .then(|| b.get("text").and_then(|t| t.as_str()).unwrap_or(""))
                            .map(str::to_owned)
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                if !text.trim().is_empty() {
                    if p.started_at == 0 {
                        p.started_at = ts;
                    }
                    p.messages.push(SessionMessage {
                        role: "assistant".into(),
                        text,
                        ts,
                    });
                    p.updated_at = p.updated_at.max(ts);
                }
            }
            Some("ai-title") => {
                if let Some(t) = v.get("aiTitle").and_then(|t| t.as_str()) {
                    p.title = Some(t.to_string());
                }
            }
            _ => {}
        }
        if p.project.is_none() {
            if let Some(cwd) = v.get("cwd").and_then(|c| c.as_str()) {
                p.project = Some(cwd.to_string());
            }
        }
    }
    if p.started_at == 0 {
        p.started_at = p.updated_at;
    }
    p.message_count = p.messages.len() as u32;
    p
}

fn parse_dsh(bytes: &[u8]) -> Parsed {
    // session.v3.jsonl.zstd — dsh APPENDS one zstd frame per write, so a
    // file holds many frames. A single decoder pass stops at the first
    // frame boundary; decode frames until the input is exhausted.
    use std::io::Read;
    let mut cursor = std::io::Cursor::new(bytes);
    let mut text = String::new();
    loop {
        let pos_before = cursor.position();
        let mut frame = match ruzstd::StreamingDecoder::new(&mut cursor) {
            Ok(d) => d,
            Err(_) => break, // no (valid) frame left
        };
        let mut buf = String::new();
        let _ = frame.read_to_string(&mut buf);
        text.push_str(&buf);
        if cursor.position() == pos_before {
            break; // no progress — avoid spinning on trailing junk
        }
    }
    parse_dsh_jsonl(&text)
}

fn parse_dsh_jsonl(text: &str) -> Parsed {
    let mut p = Parsed::default();
    for line in text.lines() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let ts = v.get("time").and_then(|t| t.as_i64()).unwrap_or(0);
        let data = v.get("data");
        match v.get("type").and_then(|t| t.as_str()) {
            Some("session") => {
                p.started_at = v.get("createdAt").and_then(|t| t.as_i64()).unwrap_or(ts);
                p.project = v.get("cwd").and_then(|c| c.as_str()).map(str::to_string);
            }
            Some("session/title") => {
                if let Some(t) = data.and_then(|d| d.get("title")).and_then(|t| t.as_str()) {
                    p.title = Some(t.to_string());
                }
            }
            Some("user/message") => {
                // Only real user prompts: data.source.kind == "user".
                // plugin / skill-catalog / runtime-context injections
                // arrive on the same event type — skip them.
                let kind = data
                    .and_then(|d| d.get("source"))
                    .and_then(|s| s.get("kind"))
                    .and_then(|k| k.as_str());
                if kind != Some("user") {
                    continue;
                }
                let text = blocks_text(data.and_then(|d| d.get("content")));
                if text.trim().is_empty() {
                    continue;
                }
                if p.preview.is_none() {
                    p.preview = Some(truncate(&text, 140));
                }
                p.messages.push(SessionMessage {
                    role: "user".into(),
                    text,
                    ts,
                });
                p.updated_at = p.updated_at.max(ts);
            }
            Some("assistant/message") => {
                // data.message.content blocks; only `text` blocks are the
                // reply (reasoning / tool-call blocks are skipped).
                let text = blocks_text(
                    data.and_then(|d| d.get("message"))
                        .and_then(|m| m.get("content")),
                );
                if text.trim().is_empty() {
                    continue;
                }
                p.messages.push(SessionMessage {
                    role: "assistant".into(),
                    text,
                    ts,
                });
                p.updated_at = p.updated_at.max(ts);
            }
            _ => {}
        }
    }
    if p.updated_at == 0 {
        p.updated_at = p.started_at;
    }
    p.message_count = p.messages.len() as u32;
    p
}

fn parse_ruagent(text: &str) -> Parsed {
    // Transcript JSONL: {"ts": "...", "seq": n, "event": {...}}. User
    // prompts are not (yet) recorded as events; replies stream as
    // agent_message_chunk runs closed by `stopped`.
    let mut p = Parsed::default();
    let mut current = String::new();
    let mut first_ts = 0i64;
    for line in text.lines() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let ts = v
            .get("ts")
            .and_then(|t| t.as_str())
            .and_then(parse_iso_ms)
            .unwrap_or(0);
        if first_ts == 0 {
            first_ts = ts;
        }
        let Some(event) = v.get("event") else {
            continue;
        };
        match event.get("type").and_then(|t| t.as_str()) {
            Some("agent_message_chunk") => {
                if let Some(blocks) = event.get("content").and_then(|c| c.as_array()) {
                    for b in blocks {
                        current.push_str(b.get("text").and_then(|t| t.as_str()).unwrap_or(""));
                    }
                }
            }
            Some("stopped") => {
                if !current.trim().is_empty() {
                    p.messages.push(SessionMessage {
                        role: "assistant".into(),
                        text: std::mem::take(&mut current),
                        ts,
                    });
                } else {
                    current.clear();
                }
                p.updated_at = p.updated_at.max(ts);
            }
            _ => {}
        }
    }
    if !current.trim().is_empty() {
        p.messages.push(SessionMessage {
            role: "assistant".into(),
            text: current,
            ts: p.updated_at,
        });
    }
    p.started_at = first_ts;
    if p.updated_at == 0 {
        p.updated_at = first_ts;
    }
    p.preview = p.messages.first().map(|m| truncate(&m.text, 140));
    p.message_count = p.messages.len() as u32;
    p
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_key(source: &str, path: &Path) -> String {
    let mut h = DefaultHasher::new();
    path.to_string_lossy().hash(&mut h);
    format!("{source}:{:016x}", h.finish())
}

/// Join the `text` blocks of a dsh content array.
fn blocks_text(content: Option<&serde_json::Value>) -> String {
    content
        .and_then(|c| c.as_array())
        .map(|blocks| {
            blocks
                .iter()
                .filter_map(|b| {
                    (b.get("type").and_then(|t| t.as_str()) == Some("text"))
                        .then(|| b.get("text").and_then(|t| t.as_str()).unwrap_or(""))
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        s.chars().take(n).collect::<String>() + "…"
    }
}

fn parse_iso_ms(s: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|d| d.timestamp_millis())
}

fn read_dirs(p: &Path) -> Option<Vec<PathBuf>> {
    Some(
        std::fs::read_dir(p)
            .ok()?
            .filter_map(Result::ok)
            .filter(|e| e.path().is_dir())
            .map(|e| e.path())
            .collect(),
    )
}

fn read_files(p: &Path, ext: &str) -> Vec<PathBuf> {
    std::fs::read_dir(p)
        .map(|rd| {
            rd.filter_map(Result::ok)
                .filter(|e| e.path().is_file() && e.path().extension().is_some_and(|x| x == ext))
                .map(|e| e.path())
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_code_session_parses() {
        let jsonl = r#"{"type":"ai-title","aiTitle":"AutoHotkey 配置","sessionId":"s1"}
{"type":"user","timestamp":"2026-09-13T11:12:35.213Z","cwd":"C:\\work\\demo","message":{"role":"user","content":"帮我写个脚本"}}
{"type":"assistant","timestamp":"2026-09-13T11:16:05.593Z","message":{"role":"assistant","content":[{"type":"thinking","thinking":"..."},{"type":"text","text":"好的，我来写。"}]}}
{"type":"user","timestamp":"2026-09-13T11:20:00.000Z","message":{"role":"user","content":[{"type":"tool_result","content":"x"}]}}"#;
        let p = parse_claude_code(jsonl);
        assert_eq!(p.title.as_deref(), Some("AutoHotkey 配置"));
        assert_eq!(p.project.as_deref(), Some("C:\\work\\demo"));
        assert_eq!(p.message_count, 2); // tool_result user line skipped
        assert_eq!(p.preview.as_deref(), Some("帮我写个脚本"));
        assert!(p.updated_at > p.started_at);
        assert_eq!(p.messages[1].text, "好的，我来写。");
    }

    #[test]
    fn dsh_session_parses_real_user_only() {
        let jsonl = r#"{"type":"session","id":"s","createdAt":1789226638326,"cwd":"C:/work/demo"}
{"type":"session/title","data":{"title":"找 skill"}}
{"type":"user/message","seq":9,"time":1789226638326,"data":{"content":[{"type":"text","text":"帮我找个 skill"}],"source":{"kind":"user"}}}
{"type":"user/message","seq":10,"time":1789226638327,"data":{"content":[{"type":"text","text":"runtime context injection"}],"source":{"kind":"plugin"}}}
{"type":"assistant/message","seq":19,"time":1789226650952,"data":{"turn":1,"message":{"role":"assistant","content":[{"type":"reasoning","text":"thinking..."},{"type":"text","text":"找到了，是这个。"}]}}}"#;
        let p = parse_dsh_jsonl(jsonl);
        assert_eq!(p.title.as_deref(), Some("找 skill"));
        assert_eq!(p.message_count, 2); // plugin injection skipped
        assert_eq!(p.preview.as_deref(), Some("帮我找个 skill"));
        assert_eq!(p.messages[1].text, "找到了，是这个。");
    }

    #[test]
    fn ruagent_transcript_groups_chunks() {
        let jsonl = r#"{"ts":"2026-09-13T02:38:59.519Z","seq":0,"event":{"type":"state_changed","status":"running"}}
{"ts":"2026-09-13T02:39:00.1Z","seq":1,"event":{"type":"agent_message_chunk","content":[{"type":"text","text":"hel"}]}}
{"ts":"2026-09-13T02:39:00.2Z","seq":2,"event":{"type":"agent_message_chunk","content":[{"type":"text","text":"lo"}]}}
{"ts":"2026-09-13T02:39:00.3Z","seq":3,"event":{"type":"stopped","stop_reason":"end_turn"}}"#;
        let p = parse_ruagent(jsonl);
        assert_eq!(p.message_count, 1);
        assert_eq!(p.messages[0].text, "hello");
    }
}
