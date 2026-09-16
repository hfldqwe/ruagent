//! Knowledge file API integration: markdown source of truth, chunk
//! editing + revisions, rebuild, and parent-section recall — the whole
//! HTTP surface over a real in-test daemon.

use std::sync::Arc;

use ruagent_daemon::api::AppState;
use ruagent_daemon::config::DaemonConfig;
use ruagent_daemon::runs::RunManager;
use ruagent_store::Db;

async fn start_test_daemon() -> (String, std::path::PathBuf) {
    static DIR_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = DIR_SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let root = std::env::temp_dir().join(format!("ruagent-kbapi-{}-{seq}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let config_dir = root.join("config");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::write(
        config_dir.join("agents.toml"),
        "[agent.mock]\nharness = \"mock\"\ncommand = \"mock-agent\"\ndescription = \"x\"\nenabled = false\n",
    )
    .unwrap();
    std::fs::write(
        config_dir.join("mcp.toml"),
        "[profile.default]\nservers = []\n",
    )
    .unwrap();
    std::fs::write(
        config_dir.join("policy.toml"),
        "[permissions]\ndefault = \"ask\"\n",
    )
    .unwrap();

    let cfg = DaemonConfig::load(&root).unwrap();
    let db = Db::open(root.join("data").join("ruagent.db")).unwrap();
    let mut agents = cfg.agents.clone();
    for card in &mut agents {
        if let Some(id) = db.agent_id_by_name(&card.name).await.unwrap() {
            card.id = id;
        }
        db.upsert_agent(card).await.unwrap();
    }
    let knowledge = ruagent_knowledge::Knowledge::open(&root, db.clone())
        .await
        .unwrap();
    let chats = ruagent_daemon::chat::ChatManager::new(
        db.clone(),
        root.clone(),
        Arc::new(|_, _| {}),
        cfg.mcp.clone(),
        ruagent_daemon::distill::AutoDistill::default(),
        None,
        ruagent_daemon::distill::AgentRegistry::default(),
    );
    let mgr = Arc::new(RunManager::new(
        db.clone(),
        root.clone(),
        agents,
        cfg.policy.to_policy(),
        cfg.mcp.clone(),
    ));
    let app = ruagent_daemon::api::router(AppState {
        mgr,
        config: Arc::new(cfg),
        knowledge: Arc::new(knowledge),
        chats,
        sessions: Arc::new(ruagent_daemon::sessions::SessionIndexer::new(
            db.clone(),
            std::env::temp_dir(),
        )),
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await });
    (format!("http://{addr}"), root)
}

const DOC: &str = "# Deploy guide\n\nThe deploy script lives in scripts/release.sh.\n\nRun it from the repository root after merging the release branch.\n\n# Tea notes\n\nEarl grey tastes best with a slice of lemon.";

#[tokio::test]
async fn markdown_truth_chunk_edits_and_parent_recall() {
    let (daemon_url, root) = start_test_daemon().await;
    let http = reqwest::Client::new();

    // PUT the raw markdown: file + index in one step.
    let resp: serde_json::Value = http
        .put(format!("{daemon_url}/api/v1/knowledge/raw/deploy-guide"))
        .json(&serde_json::json!({ "content": DOC }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(resp["chunks"].as_i64().unwrap_or(0) >= 2, "{resp:?}");
    assert_eq!(resp["file"].as_str().unwrap(), "deploy-guide.md");
    let file = root.join("knowledge").join("deploy-guide.md");
    assert!(file.is_file(), "the .md is the source of truth");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), DOC);

    // GET it back raw.
    let resp = http
        .get(format!("{daemon_url}/api/v1/knowledge/raw/deploy-guide"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert!(
        resp.headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.starts_with("text/markdown"))
    );
    assert_eq!(resp.text().await.unwrap(), DOC);

    // Search finds it (FTS with punctuated tokens).
    let hits: serde_json::Value = http
        .get(format!("{daemon_url}/api/v1/knowledge/search"))
        .query(&[("q", "release.sh")])
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(!hits["hits"].as_array().unwrap().is_empty(), "{hits:?}");

    // Aggressive recall returns the PARENT section (both paragraphs of
    // the deploy section), not just the hit fragment.
    let recall: serde_json::Value = http
        .get(format!("{daemon_url}/api/v1/recall"))
        .query(&[("q", "release.sh"), ("strategy", "aggressive")])
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let knowledge = recall["knowledge"].as_array().unwrap();
    assert!(!knowledge.is_empty(), "{recall:?}");
    let hit = &knowledge[0];
    let content = hit["content"].as_str().unwrap();
    assert!(content.contains("release.sh"), "hit itself: {content}");
    assert!(
        content.contains("release branch"),
        "parent context: {content}"
    );
    assert!(hit["excerpt"].as_str().is_some_and(|e| !e.is_empty()));

    // Conservative recall: stub + expand hint.
    let recall: serde_json::Value = http
        .get(format!("{daemon_url}/api/v1/recall"))
        .query(&[("q", "release.sh"), ("strategy", "conservative")])
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let stub = &recall["knowledge"].as_array().unwrap()[0];
    assert!(
        stub["hint"]
            .as_str()
            .is_some_and(|h| h.contains("knowledge_expand"))
    );

    // Expand: the full section around the hit.
    let chunk_id = hit["chunk_id"].as_i64().unwrap();
    let expansion: serde_json::Value = http
        .get(format!("{daemon_url}/api/v1/knowledge/expand/{chunk_id}"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(
        expansion["section"]
            .as_str()
            .unwrap()
            .starts_with("# Deploy guide")
    );
    assert_eq!(expansion["file"].as_str().unwrap(), "deploy-guide.md");

    // Edit a chunk: revision + file write-through + reindex.
    let new_chunk = "The deploy script lives in scripts/deploy.sh now.";
    let edit: serde_json::Value = http
        .patch(format!("{daemon_url}/api/v1/knowledge/chunks/{chunk_id}"))
        .json(&serde_json::json!({ "content": new_chunk }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(edit["revision"].as_i64().unwrap_or(0) > 0, "{edit:?}");
    let raw = std::fs::read_to_string(&file).unwrap();
    assert!(raw.contains("scripts/deploy.sh now."), "write-through");
    assert!(!raw.contains("scripts/release.sh"));

    // Revision history: after the reindex the revision follows the
    // SURVIVING chunk (re-pointed), so re-fetch the document's chunks.
    let docs: serde_json::Value = http
        .get(format!("{daemon_url}/api/v1/knowledge/documents"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let doc_id = docs["documents"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["name"].as_str() == Some("deploy-guide"))
        .and_then(|d| d["id"].as_i64())
        .expect("deploy-guide listed");
    let chunks: serde_json::Value = http
        .get(format!("{daemon_url}/api/v1/knowledge/documents/{doc_id}"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let survivor = chunks["chunks"]
        .as_array()
        .unwrap()
        .iter()
        .find_map(|c| {
            let pair = c.as_array()?;
            let text = pair.get(1)?.as_str()?;
            if text.contains("deploy.sh now") {
                pair.first()?.as_i64()
            } else {
                None
            }
        })
        .expect("re-pointed surviving chunk");
    let revs: serde_json::Value = http
        .get(format!(
            "{daemon_url}/api/v1/knowledge/chunks/{survivor}/revisions"
        ))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let revisions = revs["revisions"].as_array().unwrap();
    assert_eq!(revisions.len(), 1);
    let rev_id = revisions[0]["id"].as_i64().unwrap();
    assert!(
        revisions[0]["old_content"]
            .as_str()
            .is_some_and(|c| c.contains("release.sh"))
    );

    // Rollback restores the file.
    let back: serde_json::Value = http
        .post(format!(
            "{daemon_url}/api/v1/knowledge/revisions/{rev_id}/rollback"
        ))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(back["revision"].as_i64().unwrap_or(0) > rev_id, "{back:?}");
    let raw = std::fs::read_to_string(&file).unwrap();
    assert!(raw.contains("scripts/release.sh"), "rollback restores");

    // Rebuild: the index is disposable.
    let rebuild: serde_json::Value = http
        .post(format!("{daemon_url}/api/v1/knowledge/rebuild"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(
        rebuild["rebuild"]["indexed"].as_i64().unwrap_or(0) >= 1,
        "{rebuild:?}"
    );
    let hits: serde_json::Value = http
        .get(format!("{daemon_url}/api/v1/knowledge/search"))
        .query(&[("q", "release.sh")])
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(
        !hits["hits"].as_array().unwrap().is_empty(),
        "searchable after rebuild"
    );

    // The ingest endpoint is file-backed too.
    let resp: serde_json::Value = http
        .post(format!("{daemon_url}/api/v1/knowledge/ingest"))
        .json(&serde_json::json!({
            "name": "tea-notes",
            "content": "# Tea\n\nEarl grey with lemon and honey."
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(resp["chunks"].as_i64().unwrap_or(0) >= 1, "{resp:?}");
    assert!(root.join("knowledge").join("tea-notes.md").is_file());

    // Deleting a file-backed document takes its .md with it (a row-only
    // delete would be resurrected by the next scan).
    let docs: serde_json::Value = http
        .get(format!("{daemon_url}/api/v1/knowledge/documents"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let tea_id = docs["documents"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["name"].as_str() == Some("tea-notes"))
        .and_then(|d| d["id"].as_i64())
        .expect("tea-notes listed");
    let resp = http
        .delete(format!("{daemon_url}/api/v1/knowledge/documents/{tea_id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 204);
    assert!(
        !root.join("knowledge").join("tea-notes.md").exists(),
        "delete removes the file too"
    );

    // Invalid names are rejected — `..` never escapes the knowledge
    // tree (`/` itself is now a legal path separator).
    for bad in ["..%2Fevil", "a%2F..%2Fb", "%2Fleading"] {
        let resp = http
            .put(format!("{daemon_url}/api/v1/knowledge/raw/{bad}"))
            .json(&serde_json::json!({ "content": "x" }))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 400, "traversal rejected: {bad}");
        assert!(!root.join("evil.md").exists(), "nothing written outside");
    }

    // Nested documents (the wiki-mode layout): write, read, search,
    // out-of-band pickup via the recursive rebuild.
    let resp: serde_json::Value = http
        .put(format!("{daemon_url}/api/v1/knowledge/raw/wiki/deploy"))
        .json(&serde_json::json!({ "content": DOC }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(resp["chunks"].as_i64().unwrap_or(0) >= 2, "{resp:?}");
    assert!(
        root.join("knowledge")
            .join("wiki")
            .join("deploy.md")
            .is_file()
    );
    let resp = http
        .get(format!("{daemon_url}/api/v1/knowledge/raw/wiki/deploy"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text().await.unwrap(), DOC);

    // A hand-dropped nested file (how wiki pages will land) is picked
    // up by the recursive walk — rebuild proves it end to end.
    std::fs::write(
        root.join("knowledge").join("wiki").join("tea.md"),
        "# Tea\n\nEarl grey with lemon and honey.",
    )
    .unwrap();
    let rebuild: serde_json::Value = http
        .post(format!("{daemon_url}/api/v1/knowledge/rebuild"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(
        rebuild["rebuild"]["indexed"].as_i64().unwrap_or(0) >= 2,
        "{rebuild:?}"
    );
    let hits: serde_json::Value = http
        .get(format!("{daemon_url}/api/v1/knowledge/search"))
        .query(&[("q", "lemon honey")])
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(
        hits["hits"]
            .as_array()
            .unwrap()
            .iter()
            .any(|h| h["document"].as_str() == Some("wiki/tea")),
        "{hits:?}"
    );

    let _ = std::fs::remove_dir_all(&root);
}

const WIKI_PAGE_A: &str = "---\ntitle: Deploy pipeline\nsummary: \"一条命令走完全部发布\"\naliases: [deploy]\nentities: [Kubernetes]\nsources: [deploy-guide]\nsource_hashes:\n  deploy-guide: deadbeef\nstatus: generated\ngenerated_at: \"2026-09-16T00:00:00+00:00\"\ngenerator: mock\nbuild: 1\n---\n# Deploy pipeline\n\nThe deploy script lives in scripts/release.sh. See [[tea-notes]] and [[k8s]].\n";
const WIKI_PAGE_B: &str = "---\ntitle: Tea notes\nsummary: \"伯爵茶加柠檬\"\naliases: []\nentities: []\nsources: [deploy-guide]\nsource_hashes:\n  deploy-guide: deadbeef\nstatus: generated\ngenerated_at: \"2026-09-16T00:00:00+00:00\"\ngenerator: mock\nbuild: 1\n---\n# Tea notes\n\nEarl grey tastes best with a slice of lemon.\n";
const WIKI_ORPHAN: &str = "---\ntitle: Orphan\nsummary: \"\"\naliases: []\nentities: []\nsources: [deploy-guide]\nsource_hashes:\n  deploy-guide: deadbeef\nstatus: generated\ngenerated_at: \"2026-09-16T00:00:00+00:00\"\ngenerator: mock\nbuild: 1\n---\n# Orphan\n\nNobody links here, I link nowhere.\n";

/// M2 read APIs: the page inventory (stale/edited markers, link counts)
/// and the link graph (edges, broken/wanted, orphans).
#[tokio::test]
async fn wiki_pages_and_links_inventory() {
    let (daemon_url, root) = start_test_daemon().await;
    let http = reqwest::Client::new();
    for (name, content) in [
        ("deploy-guide", DOC),
        ("wiki/deploy-pipeline", WIKI_PAGE_A),
        ("wiki/tea-notes", WIKI_PAGE_B),
        ("wiki/orphan-page", WIKI_ORPHAN),
    ] {
        let resp: serde_json::Value = http
            .put(format!("{daemon_url}/api/v1/knowledge/raw/{name}"))
            .json(&serde_json::json!({ "content": content }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert!(
            resp["chunks"].as_i64().unwrap_or(0) >= 1,
            "{name}: {resp:?}"
        );
    }

    let pages: serde_json::Value = http
        .get(format!("{daemon_url}/api/v1/knowledge/wiki/pages"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let list = pages["pages"].as_array().unwrap();
    assert_eq!(list.len(), 3, "index.md excluded: {pages:?}");
    let find = |slug: &str| {
        list.iter()
            .find(|p| p["slug"].as_str() == Some(slug))
            .unwrap_or_else(|| panic!("no {slug} in {pages:?}"))
            .clone()
    };
    let a = find("deploy-pipeline");
    assert_eq!(a["title"].as_str().unwrap(), "Deploy pipeline");
    // deadbeef is not the real source hash → stale
    assert_eq!(a["stale"], serde_json::json!(true), "{a:?}");
    // no build wrote it → not edited
    assert_eq!(a["edited"], serde_json::json!(false));
    assert_eq!(a["links_out"], serde_json::json!(2)); // tea-notes + k8s
    assert_eq!(a["links_in"], serde_json::json!(0));
    let b = find("tea-notes");
    assert_eq!(b["stale"], serde_json::json!(true));
    assert_eq!(b["links_in"], serde_json::json!(1));
    assert_eq!(find("orphan-page")["links_out"], serde_json::json!(0));

    let links: serde_json::Value = http
        .get(format!("{daemon_url}/api/v1/knowledge/wiki/links"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let nodes: Vec<&str> = links["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|n| n.as_str())
        .collect();
    assert_eq!(nodes, ["deploy-pipeline", "orphan-page", "tea-notes"]);
    let edges = links["edges"].as_array().unwrap();
    assert_eq!(edges.len(), 1, "{links:?}");
    assert_eq!(edges[0]["src"].as_str().unwrap(), "deploy-pipeline");
    assert_eq!(edges[0]["dst"].as_str().unwrap(), "tea-notes");
    // k8s is linked but missing — the wanted-pages loop
    assert_eq!(links["broken"], serde_json::json!(["k8s"]), "{links:?}");
    assert_eq!(links["orphans"], serde_json::json!(["orphan-page"]));

    let _ = std::fs::remove_dir_all(&root);
}

/// §13-2: wiki hits come back as their own section, never inside
/// `knowledge`, and are ALWAYS stubs (even on the aggressive strategy).
#[tokio::test]
async fn recall_returns_wiki_section_separate_from_knowledge() {
    let (daemon_url, root) = start_test_daemon().await;
    let http = reqwest::Client::new();
    for (name, content) in [("deploy-guide", DOC), ("wiki/deploy-pipeline", WIKI_PAGE_A)] {
        http.put(format!("{daemon_url}/api/v1/knowledge/raw/{name}"))
            .json(&serde_json::json!({ "content": content }))
            .send()
            .await
            .unwrap();
    }

    let resp: serde_json::Value = http
        .get(format!("{daemon_url}/api/v1/recall"))
        .query(&[("q", "release.sh deploy"), ("strategy", "aggressive")])
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let wiki = resp["wiki"].as_array().cloned().unwrap_or_default();
    assert!(
        !wiki.is_empty(),
        "wiki hit must land in the wiki section: {resp:?}"
    );
    let stub = &wiki[0];
    assert_eq!(stub["kind"].as_str().unwrap(), "wiki");
    assert_eq!(stub["slug"].as_str().unwrap(), "deploy-pipeline");
    assert_eq!(stub["title"].as_str().unwrap(), "Deploy pipeline");
    assert_eq!(stub["stale"], serde_json::json!(true));
    assert!(stub["chunk_id"].as_i64().is_some());
    // ALWAYS conservative stubs — no full content even on aggressive
    assert!(stub.get("content").is_none(), "{stub:?}");
    // and never inside the knowledge section
    let knowledge = resp["knowledge"].as_array().cloned().unwrap_or_default();
    assert!(
        knowledge
            .iter()
            .all(|h| !h["document"].as_str().unwrap_or("").starts_with("wiki/")),
        "wiki documents must not appear as knowledge hits: {resp:?}"
    );

    let _ = std::fs::remove_dir_all(&root);
}

/// §12-2: entity recall stubs carry the wiki pages that cite them
/// (case-insensitive match between graph names and page entities).
#[tokio::test]
async fn recall_entity_carries_related_wiki() {
    let (daemon_url, root) = start_test_daemon().await;
    let http = reqwest::Client::new();
    // a wiki page whose entities frontmatter cites "Kubernetes"
    http.put(format!(
        "{daemon_url}/api/v1/knowledge/raw/wiki/deploy-pipeline"
    ))
    .json(&serde_json::json!({ "content": WIKI_PAGE_A }))
    .send()
    .await
    .unwrap();
    // a graph entity in DIFFERENT casing — the match must be case-insensitive
    http.post(format!("{daemon_url}/api/v1/graph/entity"))
        .json(&serde_json::json!({ "name": "kubernetes" }))
        .send()
        .await
        .unwrap();

    let resp: serde_json::Value = http
        .get(format!("{daemon_url}/api/v1/recall"))
        .query(&[("q", "kubernetes"), ("strategy", "conservative")])
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let entities = resp["entities"].as_array().cloned().unwrap_or_default();
    assert!(!entities.is_empty(), "entity must match: {resp:?}");
    let wiki = entities[0]["related"]["wiki"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        !wiki.is_empty(),
        "entity stub must carry the citing wiki page: {resp:?}"
    );
    assert_eq!(wiki[0]["slug"].as_str().unwrap(), "deploy-pipeline");
    assert_eq!(wiki[0]["title"].as_str().unwrap(), "Deploy pipeline");
    assert_eq!(wiki[0]["stale"], serde_json::json!(true));

    let _ = std::fs::remove_dir_all(&root);
}

/// M6: every recall call lands in the usage log with per-section
/// counts and raw top scores — the threshold-tuning dataset.
#[tokio::test]
async fn recall_calls_are_logged() {
    let (daemon_url, root) = start_test_daemon().await;
    let http = reqwest::Client::new();
    http.put(format!("{daemon_url}/api/v1/knowledge/raw/deploy-guide"))
        .json(&serde_json::json!({ "content": DOC }))
        .send()
        .await
        .unwrap();
    for (q, strategy) in [("release.sh", "aggressive"), ("kettle tea", "conservative")] {
        http.get(format!("{daemon_url}/api/v1/recall"))
            .query(&[("q", q), ("strategy", strategy), ("top_n", "5")])
            .send()
            .await
            .unwrap();
    }

    let log: serde_json::Value = http
        .get(format!("{daemon_url}/api/v1/recall/log"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let rows = log["log"].as_array().unwrap();
    assert_eq!(rows.len(), 2, "{log:?}");
    // newest first
    assert_eq!(rows[0]["query"].as_str().unwrap(), "kettle tea");
    assert_eq!(rows[0]["strategy"].as_str().unwrap(), "conservative");
    assert_eq!(rows[0]["top_n"], serde_json::json!(5));
    // the deploy-guide query returned a knowledge hit and logged a
    // raw top score
    assert_eq!(rows[1]["query"].as_str().unwrap(), "release.sh");
    assert!(rows[1]["knowledge"].as_i64().unwrap_or(0) >= 1, "{log:?}");
    assert!(
        rows[1]["top_knowledge_score"].as_f64().unwrap_or(0.0) > 0.0,
        "{log:?}"
    );

    let _ = std::fs::remove_dir_all(&root);
}
