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

// The self-link (`[[deploy-pipeline]]`) is the D.2/D.3 narrowing's falsifier: if
// `links_out` still counted self-edges, its value below would be 3, not 2.
const WIKI_PAGE_A: &str = "---\ntitle: Deploy pipeline\nsummary: \"一条命令走完全部发布\"\naliases: [deploy]\nentities: [Kubernetes]\nsources: [deploy-guide]\nsource_hashes:\n  deploy-guide: deadbeef\nstatus: generated\ngenerated_at: \"2026-09-16T00:00:00+00:00\"\ngenerator: mock\nbuild: 1\n---\n# Deploy pipeline\n\nThe deploy script lives in scripts/release.sh. See [[tea-notes]], [[k8s]] and [[deploy-pipeline]].\n";
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

    // ---- D.2/D.3: field by field, including the narrowing ------------------
    // `links_out` = distinct NON-SELF targets, broken ones INCLUDED. The page
    // links to itself, and that edge must exist in `self_links` while staying
    // OUT of `links_out` — otherwise this number would be 3.
    assert_eq!(a["self_links"], serde_json::json!(1), "{a:?}");
    assert_eq!(
        a["links_out_broken"],
        serde_json::json!(1),
        "k8s is linked and missing, and it IS counted in links_out: {a:?}"
    );
    // The third state, on a page no build ever wrote: the KEY is present and the
    // value is `null` (unknown) — never 0.00/1.00, and never omitted.
    assert!(
        a.get("cite_coverage").is_some(),
        "the key must not be omitted: {a:?}"
    );
    assert!(
        a["cite_coverage"].is_null(),
        "a hand-written page has no recorded build reading → unknown: {a:?}"
    );
    assert_eq!(a["freshness"], "stale", "{a:?}");
    assert_eq!(a["stale_sources"], serde_json::json!(["deploy-guide"]));
    let reasons: Vec<&str> = a["stale_reasons"]
        .as_array()
        .expect("stale_reasons is always an array")
        .iter()
        .filter_map(|r| r.as_str())
        .collect();
    assert!(reasons.contains(&"source hash drift"), "{a:?}");
    assert!(a["unknown_cause"].is_null(), "{a:?}");
    // `built_at` comes from the page's OWN `generated_at` (one source, not a
    // second one derived from the DB) — this fixture pages carries that stamp.
    assert_eq!(
        a["built_at"], "2026-09-16T00:00:00+00:00",
        "the page's own generated_at: {a:?}"
    );
    assert_eq!(a["citations"], serde_json::json!(0), "{a:?}");
    assert_eq!(a["uncited_sections"], serde_json::json!([]), "{a:?}");
    assert!(a["frozen_by"].is_null(), "{a:?}");
    // The frozen invariant from R-D D.2.
    assert_eq!(
        a["stale"] == serde_json::json!(true),
        a["freshness"] == serde_json::json!("stale"),
        "stale == (freshness == \"stale\"): {a:?}"
    );

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

    // ---- D.3: the graph's own readings, and the two surfaces cannot disagree
    // A self-edge is a node property, not an edge: the graph must not contain it.
    assert_eq!(edges.len(), 1, "a self-link is not an edge: {links:?}");
    assert_eq!(
        links["self_links"],
        serde_json::json!([{ "src": "deploy-pipeline", "dst": "deploy-pipeline" }]),
        "{links:?}"
    );
    assert_eq!(
        links["wanted"],
        serde_json::json!([{ "slug": "k8s", "demanders": ["deploy-pipeline"], "demand_count": 1 }]),
        "{links:?}"
    );
    // The single-source criterion: what one endpoint says about a page, the other
    // must say about the same page. (`degrees` is an ARRAY of per-node readings —
    // one row per node, the same four numbers `WikiPageInfo` carries.)
    let degs = links["degrees"].as_array().expect("degrees is an array");
    let deg = degs
        .iter()
        .find(|d| d["slug"] == "deploy-pipeline")
        .unwrap_or_else(|| panic!("no degree row for deploy-pipeline: {links:?}"));
    assert_eq!(deg["links_out"], a["links_out"], "{links:?}");
    assert_eq!(deg["links_out_broken"], a["links_out_broken"], "{links:?}");
    assert_eq!(deg["links_in"], a["links_in"], "{links:?}");
    // `links_out` is exactly "resolved out-edges + broken ones", with self-edges
    // in neither term: the fixture has one resolved edge (tea-notes), one broken
    // (k8s) and one self-link, so the number is 1 + 1 = 2 and not 3.
    let resolved_out = edges
        .iter()
        .filter(|e| e["src"] == "deploy-pipeline")
        .count() as i64;
    assert_eq!(
        resolved_out, 1,
        "tea-notes is the only resolved target: {links:?}"
    );
    assert_eq!(
        deg["links_out"].as_i64().unwrap_or(-1),
        resolved_out + deg["links_out_broken"].as_i64().unwrap_or(0),
        "links_out == resolved out-edges + links_out_broken: {links:?}"
    );
    assert_eq!(
        links["unreachable"],
        serde_json::json!(["deploy-pipeline", "orphan-page"]),
        "no page links into either of them: {links:?}"
    );
    assert!(
        links["unreachable"].is_array(),
        "unreachable is always an array: {links:?}"
    );
    let readings_at = links["readings_at"].as_str().unwrap_or_default();
    assert!(
        readings_at.contains('T'),
        "RFC3339 read stamp: {readings_at}"
    );

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

// ---------------------------------------------------------------------------
// t5: per-leg recall configuration over the real HTTP surface
// (docs/plans/capability-plugins-design.md §11.5)
// ---------------------------------------------------------------------------

/// The load-bearing keys of one recall response, as ONE comparable value.
///
/// WHY NOT the whole body: the response is a projection of the same ranking, and
/// these are the keys that carry it — the four sections in order, the per-leg
/// memory evidence, the graph counts and the scoring provenance. A ranking that
/// moved shows up here; nothing call-relative does.
fn recall_ranking(v: &serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "memories": v["memories"],
        "knowledge": v["knowledge"],
        "wiki": v["wiki"],
        "entities": v["entities"],
        "memory_legs": v["memory_legs"],
        "graph": v["graph"],
        "scoring": v["scoring"],
        // t2: the memory fusion's effective weights are part of the ranking's
        // provenance now, so the default-equivalence reading covers them too (the
        // pair compared below both run at 1:1 — no golden is re-derived).
        "memory_fusion": v["memory_fusion"],
    })
}

/// The chunk ids of the `knowledge` section, in the page's order.
fn knowledge_order(v: &serde_json::Value) -> Vec<i64> {
    v["knowledge"]
        .as_array()
        .expect("knowledge is a list")
        .iter()
        .filter_map(|h| h["chunk_id"].as_i64())
        .collect()
}

/// Replace the whole `[capabilities]` table through the API (design §14.2) — the
/// same door a user has, and the one that installs the live plane.
async fn put_capabilities(
    http: &reqwest::Client,
    daemon_url: &str,
    capabilities: serde_json::Value,
) -> serde_json::Value {
    let resp = http
        .put(format!("{daemon_url}/api/v1/capabilities"))
        .json(&serde_json::json!({ "capabilities": capabilities }))
        .send()
        .await
        .unwrap();
    let status = resp.status();
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(status, 200, "PUT capabilities was rejected: {body:?}");
    assert_eq!(body["table_present"], serde_json::json!(true), "{body:?}");
    body
}

/// Every recall leg the registry knows, switched on at its default weight.
fn all_recall_legs_on() -> serde_json::Value {
    serde_json::json!({
        "recall_leg_memory_semantic": { "enabled": true, "weight": 1.0 },
        "recall_leg_memory_fts": { "enabled": true, "weight": 1.0 },
        "recall_leg_knowledge_semantic": { "enabled": true, "weight": 2.0 },
        "recall_leg_knowledge_fts": { "enabled": true, "weight": 1.0 },
        "recall_leg_wiki": { "enabled": true },
        "recall_leg_graph": { "enabled": true },
    })
}

/// The six recall capability ids, sorted — the wire order of `legs_disabled`.
const ALL_RECALL_LEG_IDS: [&str; 6] = [
    "recall_leg_graph",
    "recall_leg_knowledge_fts",
    "recall_leg_knowledge_semantic",
    "recall_leg_memory_fts",
    "recall_leg_memory_semantic",
    "recall_leg_wiki",
];

async fn recall_get(
    http: &reqwest::Client,
    daemon_url: &str,
    query: &str,
    strategy: Option<&str>,
) -> serde_json::Value {
    let mut req = http
        .get(format!("{daemon_url}/api/v1/recall"))
        .query(&[("q", query), ("top_n", "5")]);
    if let Some(strategy) = strategy {
        req = req.query(&[("strategy", strategy)]);
    }
    let resp = req.send().await.unwrap();
    let status = resp.status();
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(status, 200, "recall for {query:?} failed: {body:?}");
    body
}

/// THE FIXTURE, chosen so a toggle or a weight is OBSERVABLE: the two legs disagree
/// about the order. `both-terms-long` holds both query terms (keyword rank 0) and is
/// the semantic leg's rank 1; `one-term-short` repeats one term, so it is the
/// semantic leg's rank 0 and is in NO keyword leg — the precision stage (both terms
/// ANDed) succeeds, so the degradation never reaches the OR-ed prefix stage.
async fn seed_t5_fixture(http: &reqwest::Client, daemon_url: &str) {
    for (name, body) in [
        (
            "both-terms-long",
            "kettle descaling zzz yyy xxx www vvv uuu ttt sss rrr",
        ),
        ("one-term-short", "kettle kettle kettle kettle kettle"),
    ] {
        let resp: serde_json::Value = http
            .put(format!("{daemon_url}/api/v1/knowledge/raw/{name}"))
            .json(&serde_json::json!({ "content": body }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert!(resp["chunks"].as_i64().unwrap_or(0) >= 1, "{resp:?}");
    }
}

/// One memory row, written the way an external CLI would: the recall path's memory
/// legs read the `memories` table, so a test that needs memory EVIDENCE has to put
/// rows there. The content repeats the two query terms ADJACENTLY, which is what
/// the memory keyword leg's phrase query (`"kettle descaling"`) matches.
async fn seed_memory_row(http: &reqwest::Client, daemon_url: &str, content: &str) {
    let resp = http
        .post(format!("{daemon_url}/api/v1/memory/write"))
        .json(&serde_json::json!({
            "store": "observation", "namespace": "user", "content": content,
        }))
        .send()
        .await
        .unwrap();
    let status = resp.status();
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(status, 200, "the memory write must land: {body:?}");
}

/// The newest recall_log row, as the log endpoint publishes it (newest first).
async fn newest_recall_log_row(http: &reqwest::Client, daemon_url: &str) -> serde_json::Value {
    let log: serde_json::Value = http
        .get(format!("{daemon_url}/api/v1/recall/log"))
        .query(&[("limit", "1")])
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    log["log"]
        .as_array()
        .and_then(|rows| rows.first())
        .cloned()
        .unwrap_or_else(|| panic!("the recall log has no rows: {log:?}"))
}

/// `candidates_json`, PARSED: the log endpoint publishes that column as a JSON
/// string, and it is the OBJECT the additive recorder keys land in — `disabled_legs`
/// from the leg-configuration work, `memory_fusion` from the memory-weight work.
/// This is also the check that the column is still an object.
fn candidates_of(row: &serde_json::Value) -> serde_json::Value {
    let raw = row["candidates_json"]
        .as_str()
        .expect("candidates_json is published as a JSON string");
    serde_json::from_str(raw).expect("candidates_json must be a JSON object")
}

/// The memory row ids a recall response returned, in page order: the CORPUS-side
/// reading, which must not move when only a weight does.
fn memory_ids(v: &serde_json::Value) -> Vec<i64> {
    v["memories"]
        .as_array()
        .expect("memories is a list")
        .iter()
        .filter_map(|m| m["id"].as_i64())
        .collect()
}

/// PARSE the response's RRF-label spelling: `rrf:k=<k>,w_semantic=<ws>,w_keyword=<wk>`
/// -> `(k, w_semantic, w_keyword)`.
///
/// ONE parser for BOTH labels a recall payload carries — `scoring.fusion` (the
/// knowledge fusion) and `memory_fusion` (the memory fusion). That is the point of
/// this alignment: a reader must not have to learn `w_sem` for one label and
/// `w_semantic` for the other, so this test fails rather than adapts if either label
/// drifts to another shape — including the shape the knowledge TYPE publishes for the
/// same information, `rrf(k=60,w_sem=2,w_kw=1)` (`FusionKind::label()`,
/// crates/knowledge/src/store.rs:151), which `api.rs` deliberately does not emit.
fn parse_rrf_label(label: &str) -> (u32, f32, f32) {
    let rest = label
        .strip_prefix("rrf:k=")
        .unwrap_or_else(|| panic!("not the response's RRF spelling: {label}"));
    let parts: Vec<&str> = rest.split(',').collect();
    assert_eq!(parts.len(), 3, "three fields in {label}");
    let k: u32 = parts[0]
        .parse()
        .unwrap_or_else(|e| panic!("k in {label}: {e}"));
    let w_semantic = parts[1]
        .strip_prefix("w_semantic=")
        .unwrap_or_else(|| panic!("w_semantic in {label}"))
        .parse::<f32>()
        .unwrap_or_else(|e| panic!("w_semantic value in {label}: {e}"));
    let w_keyword = parts[2]
        .strip_prefix("w_keyword=")
        .unwrap_or_else(|| panic!("w_keyword in {label}"))
        .parse::<f32>()
        .unwrap_or_else(|e| panic!("w_keyword value in {label}: {e}"));
    (k, w_semantic, w_keyword)
}

/// The two fusion labels of ONE response, through the SAME parser, as one comparable
/// value: `(knowledge (k,w_semantic,w_keyword), memory (k,w_semantic,w_keyword))`.
fn both_fusion_labels(v: &serde_json::Value) -> ((u32, f32, f32), (u32, f32, f32)) {
    let knowledge = parse_rrf_label(
        v["scoring"]["fusion"]
            .as_str()
            .expect("scoring.fusion is a string"),
    );
    let memory = parse_rrf_label(
        v["memory_fusion"]
            .as_str()
            .expect("memory_fusion is a string"),
    );
    (knowledge, memory)
}

/// DEFAULT EQUIVALENCE (§11.5 item 2): a `[capabilities]` table with all six legs
/// enabled at their default weights must produce the SAME recall as the absent
/// table — same ids, same order, same scores, same per-leg evidence.
#[tokio::test]
async fn the_default_leg_configuration_reproduces_the_legacy_recall() {
    let (daemon_url, root) = start_test_daemon().await;
    let http = reqwest::Client::new();
    seed_t5_fixture(&http, &daemon_url).await;

    // (a) NO `[capabilities]` table (the harness writes a policy.toml without one):
    // today's behaviour, by construction.
    let legacy = recall_get(&http, &daemon_url, "kettle descaling", None).await;
    assert_eq!(legacy["legs_disabled"], serde_json::json!([]));
    assert!(
        !knowledge_order(&legacy).is_empty(),
        "the fixture must return knowledge hits: {legacy:?}"
    );
    println!(
        "READING t5 http legacy: knowledge={:?} fusion={} graph={:?}",
        knowledge_order(&legacy),
        legacy["scoring"]["fusion"],
        legacy["graph"]
    );

    // (b) the table PRESENT, every leg on at its default weight.
    put_capabilities(&http, &daemon_url, all_recall_legs_on()).await;
    let configured = recall_get(&http, &daemon_url, "kettle descaling", None).await;
    println!(
        "READING t5 http default table: knowledge={:?} fusion={} graph={:?}",
        knowledge_order(&configured),
        configured["scoring"]["fusion"],
        configured["graph"]
    );
    assert_eq!(
        recall_ranking(&legacy),
        recall_ranking(&configured),
        "the default leg configuration moved the recall ranking"
    );
    assert_eq!(configured["legs_disabled"], serde_json::json!([]));
    let _ = std::fs::remove_dir_all(&root);
}

/// §11.3's wiki and graph rows: OFF ⇒ `"wiki": []`, `graph.entities == 0`,
/// `graph.paths == 0`, `graph.empty_reason == "leg disabled"`, `entities: []` — and
/// the OTHER legs are undisturbed (the gates do not leak into the ranking).
#[tokio::test]
async fn a_disabled_wiki_or_graph_leg_is_reported_and_leaves_its_section_empty() {
    let (daemon_url, root) = start_test_daemon().await;
    let http = reqwest::Client::new();
    seed_t5_fixture(&http, &daemon_url).await;
    put_capabilities(&http, &daemon_url, all_recall_legs_on()).await;
    let on = recall_get(&http, &daemon_url, "kettle descaling", None).await;
    assert!(!knowledge_order(&on).is_empty(), "{on:?}");
    let on_top_score = on["scoring"]["top_knowledge_score"].clone();

    let mut table = all_recall_legs_on();
    table["recall_leg_wiki"] = serde_json::json!({ "enabled": false });
    table["recall_leg_graph"] = serde_json::json!({ "enabled": false });
    put_capabilities(&http, &daemon_url, table).await;
    let off = recall_get(&http, &daemon_url, "kettle descaling", None).await;
    println!(
        "READING t5 http wiki/graph off: legs_disabled={:?} wiki={:?} graph={:?} entities={:?}",
        off["legs_disabled"], off["wiki"], off["graph"], off["entities"]
    );
    assert_eq!(
        off["legs_disabled"],
        serde_json::json!(["recall_leg_graph", "recall_leg_wiki"])
    );
    assert_eq!(off["wiki"], serde_json::json!([]));
    assert_eq!(off["entities"], serde_json::json!([]));
    assert_eq!(off["graph"]["entities"], serde_json::json!(0));
    assert_eq!(off["graph"]["paths"], serde_json::json!(0));
    assert_eq!(
        off["graph"]["empty_reason"],
        serde_json::json!("leg disabled"),
        "a new state needs a NEW literal, not one of EmptyReason's"
    );
    assert_eq!(off["graph"]["truncated_by"], serde_json::Value::Null);
    // The knowledge and memory sections are untouched: neither leg feeds them.
    assert_eq!(knowledge_order(&off), knowledge_order(&on));
    assert_eq!(off["scoring"]["top_knowledge_score"], on_top_score);
    assert_eq!(off["memory_legs"], on["memory_legs"]);
    let _ = std::fs::remove_dir_all(&root);
}

/// §11.3's last row: ALL SIX legs off is a legal, EMPTY, 200 answer that NAMES every
/// leg it did not query — never a 500 and never an empty success that reads like
/// "nothing matched".
///
/// The same test asserts the section was non-empty with the legs ON, so the empty
/// answer is attributable to the configuration and not to an empty fixture.
#[tokio::test]
async fn all_six_legs_off_is_an_empty_success_that_names_every_leg() {
    let (daemon_url, root) = start_test_daemon().await;
    let http = reqwest::Client::new();
    seed_t5_fixture(&http, &daemon_url).await;
    put_capabilities(&http, &daemon_url, all_recall_legs_on()).await;
    let on = recall_get(&http, &daemon_url, "kettle descaling", None).await;
    assert!(
        !knowledge_order(&on).is_empty(),
        "the fixture must be non-empty with the legs on: {on:?}"
    );

    let off_table = serde_json::json!({
        "recall_leg_memory_semantic": { "enabled": false },
        "recall_leg_memory_fts": { "enabled": false },
        "recall_leg_knowledge_semantic": { "enabled": false },
        "recall_leg_knowledge_fts": { "enabled": false },
        "recall_leg_wiki": { "enabled": false },
        "recall_leg_graph": { "enabled": false },
    });
    put_capabilities(&http, &daemon_url, off_table).await;
    let off = recall_get(&http, &daemon_url, "kettle descaling", None).await;
    println!(
        "READING t5 http all-off: legs_disabled={:?} sections={:?} scoring={:?} graph={:?}",
        off["legs_disabled"],
        serde_json::json!({
            "memories": off["memories"].as_array().map(|v| v.len()),
            "knowledge": off["knowledge"].as_array().map(|v| v.len()),
            "wiki": off["wiki"].as_array().map(|v| v.len()),
            "entities": off["entities"].as_array().map(|v| v.len()),
        }),
        off["scoring"],
        off["graph"]
    );
    assert_eq!(
        off["legs_disabled"],
        serde_json::json!(ALL_RECALL_LEG_IDS.to_vec()),
        "every leg this call did not query is named, from the registry's own ids"
    );
    for section in ["memories", "knowledge", "wiki", "entities"] {
        assert_eq!(
            off[section],
            serde_json::json!([]),
            "{section} must be empty with every leg off"
        );
    }
    assert_eq!(off["scoring"]["candidates"], serde_json::json!(0));
    assert_eq!(off["memory_legs"]["semantic"], serde_json::json!(0));
    assert_eq!(off["memory_legs"]["keyword"], serde_json::json!(0));
    assert_eq!(
        off["memory_legs"]["top_semantic_score"],
        serde_json::Value::Null
    );
    assert_eq!(
        off["graph"]["empty_reason"],
        serde_json::json!("leg disabled")
    );
    assert_eq!(off["graph"]["entities"], serde_json::json!(0));
    let _ = std::fs::remove_dir_all(&root);
}

/// Both STRATEGIES still work over the toggleable legs (the existing recall
/// assertions stay green), and with the memory semantic leg off the two strategies
/// necessarily agree on memories — the cosine floor is that leg's own gate, and with
/// nothing measured it cannot separate anything.
#[tokio::test]
async fn both_strategies_answer_over_the_toggleable_legs() {
    let (daemon_url, root) = start_test_daemon().await;
    let http = reqwest::Client::new();
    seed_t5_fixture(&http, &daemon_url).await;
    put_capabilities(&http, &daemon_url, all_recall_legs_on()).await;

    for strategy in ["aggressive", "conservative"] {
        let body = recall_get(&http, &daemon_url, "kettle descaling", Some(strategy)).await;
        println!(
            "READING t5 http strategy {strategy}: knowledge={:?} fusion={}",
            knowledge_order(&body),
            body["scoring"]["fusion"]
        );
        assert_eq!(body["strategy"], serde_json::json!(strategy));
        assert!(
            !knowledge_order(&body).is_empty(),
            "the knowledge legs must answer for {strategy}"
        );
        assert_eq!(body["legs_disabled"], serde_json::json!([]));
    }

    let mut table = all_recall_legs_on();
    table["recall_leg_memory_semantic"] = serde_json::json!({ "enabled": false });
    put_capabilities(&http, &daemon_url, table).await;
    let aggressive = recall_get(&http, &daemon_url, "kettle descaling", Some("aggressive")).await;
    let conservative =
        recall_get(&http, &daemon_url, "kettle descaling", Some("conservative")).await;
    assert_eq!(
        aggressive["memories"], conservative["memories"],
        "a floor nothing is measured against"
    );
    assert_eq!(
        aggressive["legs_disabled"],
        serde_json::json!(["recall_leg_memory_semantic"])
    );
    assert_eq!(aggressive["memory_legs"]["semantic"], serde_json::json!(0));
    let _ = std::fs::remove_dir_all(&root);
}

/// A NON-DEFAULT weight moves the page: at the default 2:1 the keyword-ranked chunk
/// leads, and with a nearly-zero keyword weight the semantic-ranked chunk takes the
/// top — the weights are the fusion's input, not decoration.
#[tokio::test]
async fn a_non_default_leg_weight_reorders_the_recall_page() {
    let (daemon_url, root) = start_test_daemon().await;
    let http = reqwest::Client::new();
    seed_t5_fixture(&http, &daemon_url).await;
    put_capabilities(&http, &daemon_url, all_recall_legs_on()).await;

    let default = recall_get(&http, &daemon_url, "kettle descaling", None).await;
    let default_order = knowledge_order(&default);
    let default_fusion = default["scoring"]["fusion"].clone();
    println!(
        "READING t5 http weights default: order={default_order:?} fusion={default_fusion} \
         top_keyword_rank={:?} top_semantic_rank={:?}",
        default["knowledge"][0]["keyword_rank"], default["knowledge"][0]["semantic_rank"]
    );
    assert!(default_order.len() >= 2, "{default:?}");

    let mut table = all_recall_legs_on();
    table["recall_leg_knowledge_fts"] = serde_json::json!({ "enabled": true, "weight": 0.000001 });
    put_capabilities(&http, &daemon_url, table).await;
    let reordered = recall_get(&http, &daemon_url, "kettle descaling", None).await;
    println!(
        "READING t5 http weights w_kw=1e-6: order={:?} fusion={} top_keyword_rank={:?} \
         top_semantic_rank={:?}",
        knowledge_order(&reordered),
        reordered["scoring"]["fusion"],
        reordered["knowledge"][0]["keyword_rank"],
        reordered["knowledge"][0]["semantic_rank"]
    );
    let reordered_order = knowledge_order(&reordered);
    assert_ne!(
        default_order, reordered_order,
        "a non-default weight must move the page, not just be stored"
    );
    assert_eq!(
        reordered["legs_disabled"],
        serde_json::json!([]),
        "a weight change is not a toggle"
    );
    assert!(
        reordered["scoring"]["fusion"]
            .as_str()
            .unwrap_or_default()
            .contains("w_keyword=0.000001"),
        "the reported fusion must carry the weights that produced the ranking: {}",
        reordered["scoring"]["fusion"]
    );
    assert_ne!(
        default_fusion, reordered["scoring"]["fusion"],
        "the fusion label must differ when the weights do"
    );
    // With a nearly-zero keyword weight the page follows the SEMANTIC leg, so the
    // top hit is the semantic rank 0 row (which carries no keyword evidence).
    assert_eq!(
        reordered["knowledge"][0]["semantic_rank"],
        serde_json::json!(0)
    );
    assert_eq!(
        reordered["knowledge"][0]["keyword_rank"],
        serde_json::Value::Null
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// The `query_keyword_stage` label each knowledge hit carries, in page order.
fn keyword_stages(v: &serde_json::Value) -> Vec<String> {
    v["knowledge"]
        .as_array()
        .expect("knowledge is a list")
        .iter()
        .filter_map(|h| h["query_keyword_stage"].as_str().map(str::to_string))
        .collect()
}

/// t12: the stage label distinguishes a leg that was SWITCHED OFF from one that ran
/// and matched nothing — the whole point of the item (design §11.3).
///
/// Three readings, one per state the label can be in:
/// * ENABLED and matched (`kettle descaling`) -> the construction, `"precision"`;
/// * ENABLED and nothing matched (`zebra xylophone` is in no document) -> `"empty"`;
/// * DISABLED -> `"disabled"`.
/// The last two MUST differ, so a future change that collapsed them fails here.
#[tokio::test]
async fn a_disabled_keyword_leg_reports_disabled_not_empty() {
    let (daemon_url, root) = start_test_daemon().await;
    let http = reqwest::Client::new();
    seed_t5_fixture(&http, &daemon_url).await;
    put_capabilities(&http, &daemon_url, all_recall_legs_on()).await;

    // ENABLED, and the query matches lexically.
    let matched = recall_get(&http, &daemon_url, "kettle descaling", None).await;
    let matched_stages = keyword_stages(&matched);
    // ENABLED, and the query matches NOTHING lexically: the leg RUNS and comes back
    // empty, which is the state the label must keep saying "empty".
    let empty = recall_get(&http, &daemon_url, "zebra xylophone", None).await;
    let empty_stages = keyword_stages(&empty);

    // DISABLED: the same no-lexical-match query, with the leg switched off.
    let mut table = all_recall_legs_on();
    table["recall_leg_knowledge_fts"] = serde_json::json!({ "enabled": false });
    put_capabilities(&http, &daemon_url, table).await;
    let disabled = recall_get(&http, &daemon_url, "zebra xylophone", None).await;
    let disabled_stages = keyword_stages(&disabled);

    println!(
        "READING t12 stages: enabled+matched={matched_stages:?} enabled+empty={empty_stages:?} \
         disabled={disabled_stages:?} leg_disabled={:?}",
        disabled["legs_disabled"]
    );
    assert!(
        !matched_stages.is_empty() && !empty_stages.is_empty() && !disabled_stages.is_empty(),
        "each state must have knowledge hits to label: matched={matched:?} empty={empty:?} \
         disabled={disabled:?}"
    );
    assert!(
        matched_stages.iter().all(|s| s == "precision"),
        "an ENABLED leg that matched reports its construction: {matched_stages:?}"
    );
    assert!(
        empty_stages.iter().all(|s| s == "empty"),
        "an ENABLED leg that ran and matched NOTHING reports \"empty\": {empty_stages:?}"
    );
    assert!(
        disabled_stages.iter().all(|s| s == "disabled"),
        "a DISABLED leg reports \"disabled\", not \"empty\": {disabled_stages:?}"
    );
    assert_ne!(
        empty_stages, disabled_stages,
        "\"you turned this off\" must be distinguishable from \"this ran and found nothing\""
    );
    // ... and the disabled leg contributed nothing, so the label is not the only
    // evidence a reader has.
    assert_eq!(
        disabled["legs_disabled"],
        serde_json::json!(["recall_leg_knowledge_fts"])
    );
    for hit in disabled["knowledge"].as_array().unwrap() {
        assert_eq!(hit["legs"], serde_json::json!(["semantic"]), "{hit:?}");
        assert!(hit["keyword_rank"].is_null(), "{hit:?}");
        assert!(hit["keyword_score"].is_null(), "{hit:?}");
    }
    let _ = std::fs::remove_dir_all(&root);
}

// ---------------------------------------------------------------------------
// The MEMORY fusion's effective weights over the real HTTP surface
// (docs/plans/capability-plugins-design.md §20.2, the memory half)
//
// The alignment half: the labels of BOTH fusions in one payload are read here by ONE
// parser, `parse_rrf_label` — `memory_fusion` uses the spelling the response already
// emits for the knowledge fusion (`rrf:k=..,w_semantic=..,w_keyword=..`), NOT the
// different string `FusionKind::label()` renders (`rrf(k=..,w_sem=..,w_kw=..)`),
// which `api.rs` deliberately does not call.
// ---------------------------------------------------------------------------

/// t2 (§20.2, memory half): the weights the MEMORY fusion actually used reach BOTH
/// records — the response and `recall_log` — so a weight change is distinguishable
/// from a corpus change.
///
/// The demonstration is TWO runs over the SAME corpus at two weights: the corpus
/// reading (the memory row ids) is identical, the knowledge fusion is untouched, and
/// the only thing that moved is the memory fusion label. Before t2 the two runs were
/// the same record in every respect that mentions the memory legs — the complaint
/// §20.2 is about.
#[tokio::test]
async fn the_memory_fusion_weights_are_recorded_in_the_response_and_the_log() {
    let (daemon_url, root) = start_test_daemon().await;
    let http = reqwest::Client::new();
    seed_t5_fixture(&http, &daemon_url).await;
    seed_memory_row(
        &http,
        &daemon_url,
        "kettle descaling kettle descaling kettle",
    )
    .await;
    put_capabilities(&http, &daemon_url, all_recall_legs_on()).await;

    // RUN 1 — the default memory weights (1:1).
    let default = recall_get(&http, &daemon_url, "kettle descaling", None).await;
    let default_row = newest_recall_log_row(&http, &daemon_url).await;
    let default_candidates = candidates_of(&default_row);
    println!(
        "READING t2 memory weights default: response={} log={} memories={:?} leg_counts={:?}",
        default["memory_fusion"],
        default_candidates["memory_fusion"],
        memory_ids(&default),
        default["memory_legs"]
    );
    assert!(
        !memory_ids(&default).is_empty(),
        "the fixture must produce memory evidence for a weight to be worth recording: {default:?}"
    );
    assert_eq!(
        default["memory_fusion"],
        serde_json::json!("rrf:k=60,w_semantic=1,w_keyword=1"),
        "the default must read as the 1:1 fusion that reproduces the frozen `rrf`, \
         in the response's own spelling"
    );
    assert_eq!(
        default_candidates["memory_fusion"], default["memory_fusion"],
        "the response and the log must carry the SAME label"
    );
    // The DEFAULT payload's two labels are parsed by one parser, so a reader does
    // not have to learn `w_sem` for one label and `w_semantic` for the other.
    let (knowledge_weights, memory_weights) = both_fusion_labels(&default);
    println!(
        "READING both labels parsed at default: knowledge={knowledge_weights:?} memory={memory_weights:?}"
    );
    assert_eq!(knowledge_weights, (60, 2.0, 1.0));
    assert_eq!(memory_weights, (60, 1.0, 1.0));

    // RUN 2 — the SAME corpus, one memory weight moved.
    let mut table = all_recall_legs_on();
    table["recall_leg_memory_semantic"] = serde_json::json!({ "enabled": true, "weight": 3.0 });
    put_capabilities(&http, &daemon_url, table).await;
    let weighted = recall_get(&http, &daemon_url, "kettle descaling", None).await;
    let weighted_row = newest_recall_log_row(&http, &daemon_url).await;
    let weighted_candidates = candidates_of(&weighted_row);
    println!(
        "READING t2 memory weights w_sem=3: response={} log={} memories={:?} leg_counts={:?}",
        weighted["memory_fusion"],
        weighted_candidates["memory_fusion"],
        memory_ids(&weighted),
        weighted["memory_legs"]
    );
    assert_eq!(
        weighted["memory_fusion"],
        serde_json::json!("rrf:k=60,w_semantic=3,w_keyword=1"),
        "the reported weights must be the EFFECTIVE ones the ranking used, in the response's own spelling"
    );
    assert_eq!(
        weighted_candidates["memory_fusion"],
        weighted["memory_fusion"]
    );
    // BOTH labels of one payload go through ONE parser, and the knowledge label
    // — an existing key — is byte-for-byte what it was (2:1, `w_semantic` spelling).
    let (knowledge_weights, memory_weights) = both_fusion_labels(&weighted);
    println!(
        "READING both labels parsed at w_sem=3: knowledge={knowledge_weights:?} memory={memory_weights:?}"
    );
    assert_eq!(
        knowledge_weights,
        (60, 2.0, 1.0),
        "the knowledge label is untouched by this change"
    );
    assert_eq!(memory_weights, (60, 3.0, 1.0));
    assert_eq!(
        weighted["scoring"]["fusion"],
        serde_json::json!("rrf:k=60,w_semantic=2,w_keyword=1"),
        "the existing key keeps its exact string"
    );

    // THE READING THE WHOLE ITEM IS ABOUT: same corpus (same memory rows), different
    // recorded weight — so "the weight changed" no longer looks like "the corpus
    // changed".
    assert_eq!(
        memory_ids(&weighted),
        memory_ids(&default),
        "the corpus did not change between the two runs"
    );
    assert_ne!(
        weighted["memory_fusion"], default["memory_fusion"],
        "the recorded label must move when the weight does"
    );
    // The KNOWLEDGE fusion is a different recording and did not move: the memory
    // weight is not smuggled into the knowledge side's key.
    assert_eq!(
        weighted["scoring"]["fusion"], default["scoring"]["fusion"],
        "a memory weight change is not a knowledge weight change"
    );
    assert_eq!(
        weighted_candidates["fusion"], default_candidates["fusion"],
        "the log's knowledge `fusion` label is separate from `memory_fusion`"
    );
    // `top_legs_json` is a JSON ARRAY (one entry per ranked hit) and stays one: the
    // additive key went to `candidates_json`, which is an OBJECT (the reason
    // `disabled_legs` lives there too).
    let top_legs_json = weighted_row["top_legs_json"].as_str().unwrap_or_default();
    let parsed: serde_json::Value = serde_json::from_str(top_legs_json)
        .unwrap_or_else(|e| panic!("top_legs_json must parse: {e}: {top_legs_json}"));
    assert!(
        parsed.is_array(),
        "top_legs_json must keep its array shape: {top_legs_json}"
    );
    assert!(
        !top_legs_json.contains("memory_fusion"),
        "the additive key must not land in the per-hit array: {top_legs_json}"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// t2 (§20.2): a DISABLED memory leg contributes 0.0 — never the stale weight the
/// file kept next to `enabled = false` — and it is NAMED in `legs_disabled`, so
/// "off" cannot be read as "weighted to zero". The second half is what makes that
/// reading sound rather than convenient: an ENABLED leg at 0.0 is REFUSED by the
/// daemon's own weight rule, so 0.0 is a value the fusion can never be handed and an
/// effective 0.0 in the record can only mean the leg was off.
#[tokio::test]
async fn a_disabled_memory_leg_records_zero_not_its_stale_weight() {
    let (daemon_url, root) = start_test_daemon().await;
    let http = reqwest::Client::new();
    seed_t5_fixture(&http, &daemon_url).await;
    seed_memory_row(
        &http,
        &daemon_url,
        "kettle descaling kettle descaling kettle",
    )
    .await;

    // A stale weight beside `enabled = false` is what makes this a test: a label
    // built from the CONFIGURED values would say 3.
    let mut table = all_recall_legs_on();
    table["recall_leg_memory_semantic"] = serde_json::json!({ "enabled": false, "weight": 3.0 });
    put_capabilities(&http, &daemon_url, table).await;
    let off = recall_get(&http, &daemon_url, "kettle descaling", None).await;
    let off_candidates = candidates_of(&newest_recall_log_row(&http, &daemon_url).await);
    println!(
        "READING t2 memory leg off with a stale weight=3: legs_disabled={:?} response={} log={}",
        off["legs_disabled"], off["memory_fusion"], off_candidates["memory_fusion"]
    );
    assert_eq!(
        off["legs_disabled"],
        serde_json::json!(["recall_leg_memory_semantic"]),
        "the disabled leg is named, so the 0 below is attributable to a toggle"
    );
    assert_eq!(
        off["memory_fusion"],
        serde_json::json!("rrf:k=60,w_semantic=0,w_keyword=1"),
        "a disabled leg contributes 0.0, never its stale configured weight"
    );
    assert_eq!(off_candidates["memory_fusion"], off["memory_fusion"]);

    // The other half: 0.0 with the leg ENABLED is refused, not silently accepted as
    // "a leg weighted to zero" — so off and zero-weight cannot be confused.
    let mut zeroed = all_recall_legs_on();
    zeroed["recall_leg_memory_semantic"] = serde_json::json!({ "enabled": true, "weight": 0.0 });
    put_capabilities(&http, &daemon_url, zeroed).await;
    let resp = http
        .get(format!("{daemon_url}/api/v1/recall"))
        .query(&[("q", "kettle descaling"), ("top_n", "5")])
        .send()
        .await
        .unwrap();
    let status = resp.status();
    let body = resp.text().await.unwrap();
    println!("READING t2 memory leg enabled with weight=0: status={status} body={body}");
    assert_eq!(
        status, 400,
        "an ENABLED leg at weight 0 is a configuration error, not an empty ranking: {body}"
    );
    assert!(
        body.contains("memory semantic"),
        "the refusal names the leg it is about: {body}"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// t2 (§20.2): the new evidence is ADDITIVE, and the default run is unchanged.
///
/// With NO `[capabilities]` table — the harness's own default configuration, over
/// the EXISTING fixed fixture — every pre-existing key keeps its name, type and
/// meaning: `memory_legs` still has exactly the six keys it had, and the additions
/// are the new top-level `memory_fusion` and `candidates_json.memory_fusion`. No
/// golden is re-derived; the only frozen value asserted here is the NEW key's, whose
/// default 1:1 reading is exactly the configuration the frozen unweighted `rrf`
/// identity is pinned to (unit-tested bit for bit in `memembed.rs`).
#[tokio::test]
async fn the_memory_fusion_label_is_additive_at_the_default_configuration() {
    let (daemon_url, root) = start_test_daemon().await;
    let http = reqwest::Client::new();
    seed_t5_fixture(&http, &daemon_url).await;
    let body = recall_get(&http, &daemon_url, "kettle descaling", None).await;
    let row = newest_recall_log_row(&http, &daemon_url).await;
    let candidates = candidates_of(&row);
    println!(
        "READING t2 default payload: memory_fusion={} candidates={} memory_legs={:?}",
        body["memory_fusion"], candidates, body["memory_legs"]
    );

    // (a) the addition carries the DEFAULT weights, in the response's own spelling
    // (the same shape `scoring.fusion` uses, so one parser reads both).
    assert_eq!(
        body["memory_fusion"],
        serde_json::json!("rrf:k=60,w_semantic=1,w_keyword=1")
    );
    assert_eq!(candidates["memory_fusion"], body["memory_fusion"]);
    let (knowledge_weights, memory_weights) = both_fusion_labels(&body);
    assert_eq!(knowledge_weights, (60, 2.0, 1.0));
    assert_eq!(memory_weights, (60, 1.0, 1.0));
    assert_eq!(
        body["scoring"]["fusion"],
        serde_json::json!("rrf:k=60,w_semantic=2,w_keyword=1"),
        "the existing knowledge label is not part of this change"
    );

    // (b) additive only: `memory_legs` is the object it was before t2 — the same six
    // keys, untouched (a documented response key must not grow a member and silently
    // change shape for a reader that enumerates it).
    let mut leg_keys: Vec<&str> = body["memory_legs"]
        .as_object()
        .expect("memory_legs is an object")
        .keys()
        .map(String::as_str)
        .collect();
    leg_keys.sort_unstable();
    assert_eq!(
        leg_keys,
        [
            "dropped_by_top_n",
            "keyword",
            "keyword_new",
            "returned",
            "semantic",
            "top_semantic_score",
        ],
        "the memory evidence object is not the place for the addition"
    );
    // ... and the recorder's addition is exactly one new key in the OBJECT column,
    // beside the ones the `disabled_legs` key established there.
    let mut candidate_keys: Vec<&str> = candidates
        .as_object()
        .expect("candidates_json is an object")
        .keys()
        .map(String::as_str)
        .collect();
    candidate_keys.sort_unstable();
    assert_eq!(
        candidate_keys,
        [
            "candidates",
            "disabled_legs",
            "fusion",
            "leg_window",
            "memory_fusion",
            "ranked_page",
        ]
    );
    // `top_legs_json` is a JSON ARRAY of per-hit entries and stays exactly that.
    let top_legs_json = row["top_legs_json"].as_str().unwrap_or_default();
    assert!(
        serde_json::from_str::<serde_json::Value>(top_legs_json).is_ok_and(|v| v.is_array()),
        "top_legs_json must keep its array shape: {top_legs_json}"
    );

    // (c) the default run with the `[capabilities]` table PRESENT at its default
    // values is the same recall, the new key included (the helper now compares it).
    put_capabilities(&http, &daemon_url, all_recall_legs_on()).await;
    let configured = recall_get(&http, &daemon_url, "kettle descaling", None).await;
    assert_eq!(
        recall_ranking(&body),
        recall_ranking(&configured),
        "the default leg configuration moved the recall ranking"
    );
    assert_eq!(body["memory_fusion"], configured["memory_fusion"]);
    let _ = std::fs::remove_dir_all(&root);
}
