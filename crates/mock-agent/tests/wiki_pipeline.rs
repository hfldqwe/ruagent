//! Wiki pipeline integration: the full three-stage build driven by the
//! scripted mock agent (marker-keyed replies — each one-shot ACP call
//! spawns a fresh process, so order-based scripts cannot work).
//!
//! gen2 (t10) adds the citation gate, freshness/invalidation, the link-graph
//! readings and the correction loop. Every new judgment has BOTH sides here: a
//! page that verifies and one that does not, a `keep` that must be overridden and
//! one that stands, a freeze that records its reason and one that does not exist.

use std::sync::Arc;

use ruagent_daemon::api::AppState;
use ruagent_daemon::config::DaemonConfig;
use ruagent_daemon::runs::RunManager;
use ruagent_store::Db;

static DIR_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// The daemon's build single-flight flag (`BUILD_RUNNING` in wiki.rs)
/// is process-global — correct for production (one daemon per
/// process), but this test binary hosts SEVERAL daemons in parallel.
/// Wiki tests serialize around it.
static ONE_BUILD_AT_A_TIME: std::sync::OnceLock<tokio::sync::Mutex<()>> =
    std::sync::OnceLock::new();

/// Serialize the whole test body around the process-global single-flight flag.
async fn serial() -> tokio::sync::MutexGuard<'static, ()> {
    ONE_BUILD_AT_A_TIME
        .get_or_init(|| tokio::sync::Mutex::new(()))
        .lock()
        .await
}

/// A test daemon whose ONLY enabled agent is the scripted mock. The replies file
/// outlives the daemon (agents spawn per call) and is returned so a test can
/// REWRITE it between builds — that is how one root gets two different plans. The
/// `Db` handle is the daemon's own clone (one writer actor, never a second
/// connection), so a test can drive the correction loop and read the derived
/// tables without inventing an HTTP route for them.
struct WikiDaemon {
    url: String,
    root: std::path::PathBuf,
    db: Db,
    replies: std::path::PathBuf,
}

async fn start_wiki_daemon(replies: &serde_json::Value) -> WikiDaemon {
    let seq = DIR_SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let root = std::env::temp_dir().join(format!("ruagent-wiki-{}-{seq}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let config_dir = root.join("config");
    std::fs::create_dir_all(&config_dir).unwrap();
    let replies_path = root.join("replies.json");
    write_replies(&replies_path, replies);

    // Windows paths must use forward slashes here: TOML basic strings
    // treat backslashes as escapes.
    let bin = env!("CARGO_BIN_EXE_ruagent-mock-agent").replace('\\', "/");
    let replies_arg = replies_path.to_string_lossy().replace('\\', "/");
    std::fs::write(
        config_dir.join("agents.toml"),
        format!(
            "[agent.mock]
harness = \"mock\"
command = \"{bin} --behavior scripted --replies {replies_arg}\"
description = \"scripted mock\"
"
        ),
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
    WikiDaemon {
        url: format!("http://{addr}"),
        root,
        db,
        replies: replies_path,
    }
}

fn write_replies(path: &std::path::Path, replies: &serde_json::Value) {
    std::fs::write(path, serde_json::to_string(replies).unwrap()).unwrap();
}

async fn wait_build(http: &reqwest::Client, url: &str, id: i64) -> serde_json::Value {
    for _ in 0..240 {
        let b: serde_json::Value = http
            .get(format!("{url}/api/v1/knowledge/wiki/builds/{id}"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let status = b["build"]["status"].as_str().unwrap_or("?");
        if status == "done" || status == "failed" {
            return b;
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
    panic!("build {id} did not finish in time");
}

fn plan_json(pages: serde_json::Value) -> String {
    serde_json::json!({ "pages": pages, "notes": "test plan" }).to_string()
}

fn page(slug: &str, title: &str, sources: &[&str], action: &str) -> serde_json::Value {
    serde_json::json!({
        "slug": slug, "title": title, "summary": format!("{title} 摘要"),
        "aliases": [], "entities": ["ruagent"], "sources": sources, "action": action
    })
}

/// Put a source document and return the chunk ids the knowledge base assigned it.
/// The anchors a body cites depend on those ids, so they are a MEASURED
/// precondition: if chunking changes, the test fails HERE with the reason instead
/// of three steps later as a mystery "dangling citation".
async fn put_source(http: &reqwest::Client, url: &str, name: &str, content: &str) -> Vec<i64> {
    let resp: serde_json::Value = http
        .put(format!("{url}/api/v1/knowledge/raw/{name}"))
        .json(&serde_json::json!({ "content": content }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(resp["chunks"].as_i64().unwrap_or(0) >= 1, "{resp:?}");
    let docs: serde_json::Value = http
        .get(format!("{url}/api/v1/knowledge/documents"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let id = docs["documents"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["name"].as_str() == Some(name))
        .unwrap_or_else(|| panic!("no document {name}: {docs:?}"))["id"]
        .as_i64()
        .unwrap();
    let chunks: serde_json::Value = http
        .get(format!("{url}/api/v1/knowledge/documents/{id}"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    chunks["chunks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c[0].as_i64().unwrap())
        .collect()
}

async fn start_build(http: &reqwest::Client, url: &str, body: serde_json::Value) -> i64 {
    let started: serde_json::Value = http
        .post(format!("{url}/api/v1/knowledge/wiki/build"))
        .json(&body)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    started["build_id"]
        .as_i64()
        .unwrap_or_else(|| panic!("no build id in {started:?}"))
}

async fn wiki_pages(http: &reqwest::Client, url: &str) -> Vec<serde_json::Value> {
    let v: serde_json::Value = http
        .get(format!("{url}/api/v1/knowledge/wiki/pages"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    v["pages"].as_array().cloned().unwrap_or_default()
}

fn page_row<'a>(done: &'a serde_json::Value, slug: &str) -> &'a serde_json::Value {
    done["pages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["slug"].as_str() == Some(slug))
        .unwrap_or_else(|| panic!("no row for {slug} in {done:?}"))
}

const DEPLOY_SOURCE: &str =
    "# Deploy\n\nThe deploy script lives in scripts/deploy.sh. Run from the root.";
const TEA_SOURCE: &str = "# Tea\n\nEarl grey with bergamot and honey.";

/// The good side of the citation gate: every content section carries an anchor
/// into a chunk of a cited source, and `## 来源` lists exactly those documents.
const DEPLOY_BODY: &str = "# 部署流水线\n\n部署脚本位于 [[tea-notes|茶水间守则]] 旁边的 scripts/deploy.sh。\n\n## 步骤\n\n从仓库根目录运行，合并后执行。\n<!-- cite: deploy-guide#1 -->\n\n## 来源\n\n- deploy-guide\n";

const TEA_BODY: &str = "# 茶水间守则\n\n伯爵茶配柠檬与蜂蜜。相关：[[deploy-pipeline]]。\n\n## 茶\n\n伯爵茶加一片柠檬更好喝。\n<!-- cite: tea-doc#2 -->\n\n## 来源\n\n- tea-doc\n";

/// A verifiable one-page body for the `src-alpha` fixture (chunk 1 exists after
/// `put_source`, and the assertion above proves it).
const ALPHA_BODY_V1: &str = "# Alpha 笔记\n\n## 事实\n\nAlpha 有两个分部。\n<!-- cite: src-alpha#1 -->\n\n## 来源\n\n- src-alpha\n";

#[tokio::test]
async fn dry_run_plan_then_confirmed_build_lands_pages() {
    let _serialized = serial().await;
    let replies = serde_json::json!([
        {"marker": "WIKI PLANNER", "reply": plan_json(serde_json::json!([
            page("deploy-pipeline", "部署流水线", &["deploy-guide"], "create"),
            page("tea-notes", "茶水间守则", &["tea-doc"], "create"),
        ]))},
        {"marker": "WIKI PAGE WRITER (slug: deploy-pipeline)", "reply": DEPLOY_BODY},
        {"marker": "WIKI PAGE WRITER (slug: tea-notes)", "reply": TEA_BODY},
    ]);
    let WikiDaemon { url, root, .. } = start_wiki_daemon(&replies).await;
    let http = reqwest::Client::new();

    // Two top-level source documents. The chunk ids the bodies cite are read back.
    assert_eq!(
        put_source(&http, &url, "deploy-guide", DEPLOY_SOURCE).await,
        [1]
    );
    assert_eq!(put_source(&http, &url, "tea-doc", TEA_SOURCE).await, [2]);

    // Dry-run: the plan comes back for review (the human gate).
    let plan: serde_json::Value = http
        .post(format!("{url}/api/v1/knowledge/wiki/build"))
        .json(&serde_json::json!({ "scope": "all", "dry_run": true }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(plan["status"], "planned", "{plan:?}");
    assert_eq!(plan["pages_planned"], 2);
    let slugs: Vec<&str> = plan["plan"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["slug"].as_str().unwrap())
        .collect();
    assert_eq!(slugs, vec!["deploy-pipeline", "tea-notes"]);
    let build_id = plan["build_id"].as_i64().unwrap();

    // No pages written by a dry-run.
    assert!(
        !root
            .join("knowledge")
            .join("wiki")
            .join("deploy-pipeline.md")
            .exists()
    );

    // Confirm the reviewed plan: executes it without re-planning.
    let started: serde_json::Value = http
        .post(format!("{url}/api/v1/knowledge/wiki/build"))
        .json(&serde_json::json!({ "confirm_plan": build_id }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(started["status"], "running", "{started:?}");
    let confirmed_id = started["build_id"].as_i64().unwrap();

    let done = wait_build(&http, &url, confirmed_id).await;
    assert_eq!(done["build"]["status"], "done", "{done:?}");
    assert_eq!(done["build"]["pages_written"], 2);
    for p in done["pages"].as_array().unwrap() {
        assert_eq!(p["status"], "written", "{p:?}");
    }

    // Pages landed with daemon-serialized frontmatter.
    let page_text = std::fs::read_to_string(
        root.join("knowledge")
            .join("wiki")
            .join("deploy-pipeline.md"),
    )
    .unwrap();
    assert!(
        page_text.starts_with("---\n"),
        "frontmatter first: {page_text:?}"
    );
    assert!(page_text.contains("title: 部署流水线"));
    assert!(page_text.contains("sources: [deploy-guide]"));
    assert!(page_text.contains("generator: mock"));
    assert!(page_text.contains(&format!("build: {confirmed_id}")));
    assert!(page_text.contains("[[tea-notes|茶水间守则]]"));
    // G1/G2: the daemon's own verdict, and one anchor per content section.
    assert!(page_text.contains("verified: verified"), "{page_text}");
    assert!(page_text.contains("citations:"), "{page_text}");
    assert!(page_text.contains("chunk_id: 1"), "{page_text}");
    assert!(page_text.contains("chunk_hash:"), "{page_text}");
    // index.md regenerated.
    let index =
        std::fs::read_to_string(root.join("knowledge").join("wiki").join("index.md")).unwrap();
    assert!(index.contains("[[deploy-pipeline|部署流水线]]"));
    assert!(index.contains("[[tea-notes|茶水间守则]]"));

    // The read API reports the same verifiability reading. This is the BUILD's
    // recorded number read back from `wiki_pages` (RV-D-1: the old shape
    // re-derived it from frontmatter, where it could only be 0.0 or 1.0).
    let pages = wiki_pages(&http, &url).await;
    let deploy = pages
        .iter()
        .find(|p| p["slug"] == "deploy-pipeline")
        .unwrap();
    assert_eq!(deploy["citations"], 1, "{deploy:?}");
    assert_eq!(deploy["cite_coverage"], 1.0, "{deploy:?}");
    assert_eq!(deploy["freshness"], "fresh", "{deploy:?}");
    assert_eq!(deploy["stale_reasons"], serde_json::json!([]), "{deploy:?}");
    assert_eq!(
        deploy["uncited_sections"],
        serde_json::json!([]),
        "{deploy:?}"
    );
    let tea = pages.iter().find(|p| p["slug"] == "tea-notes").unwrap();
    assert_eq!(tea["cite_coverage"], 1.0, "{tea:?}");
    assert_eq!(tea["freshness"], "fresh", "{tea:?}");
    // The RV-D-1 invariant on the good side: a full coverage and a non-empty
    // uncited list may never appear in the same response, and a coverage is quoted
    // only while the page is fresh.
    for p in &pages {
        let full = p["cite_coverage"].as_f64() == Some(1.0);
        let uncited_non_empty = p["uncited_sections"].as_array().map(|a| !a.is_empty());
        assert!(
            !(full && uncited_non_empty == Some(true)),
            "cite_coverage=1.0 next to uncited sections: {p:?}"
        );
        assert_eq!(
            p["cite_coverage"].is_null(),
            p["freshness"] != "fresh",
            "coverage is quoted only on a fresh page: {p:?}"
        );
    }

    // Pages are searchable documents (eager index via save).
    let hits: serde_json::Value = http
        .get(format!("{url}/api/v1/knowledge/search"))
        .query(&[("q", "deploy.sh")])
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
            .any(|h| h["document"].as_str() == Some("wiki/deploy-pipeline")),
        "{hits:?}"
    );

    // A confirm id that does not exist is a 400, not a silent new plan.
    let resp = http
        .post(format!("{url}/api/v1/knowledge/wiki/build"))
        .json(&serde_json::json!({ "confirm_plan": 999 }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);

    let _ = std::fs::remove_dir_all(&root);
}

#[tokio::test]
async fn rebuild_updates_backup_edited_skip_and_delete() {
    let _serialized = serial().await;
    // Round 1: create the page.
    let replies = serde_json::json!([
        {"marker": "WIKI PLANNER", "reply": plan_json(serde_json::json!([
            page("deploy-pipeline", "部署流水线", &["deploy-guide"], "create"),
        ]))},
        {"marker": "WIKI PAGE WRITER (slug: deploy-pipeline)", "reply": DEPLOY_BODY},
    ]);
    let WikiDaemon { url, root, .. } = start_wiki_daemon(&replies).await;
    let http = reqwest::Client::new();
    put_source(&http, &url, "deploy-guide", DEPLOY_SOURCE).await;
    let id1 = start_build(&http, &url, serde_json::json!({ "scope": "all" })).await;
    let done = wait_build(&http, &url, id1).await;
    assert_eq!(done["build"]["status"], "done", "{done:?}");

    // Round 2 (same daemon, same script): update the UNEDITED page —
    // the pre-write backup must appear.
    let id2 = start_build(&http, &url, serde_json::json!({ "scope": "all" })).await;
    let done = wait_build(&http, &url, id2).await;
    assert_eq!(done["build"]["status"], "done", "{done:?}");
    let backup = root
        .join("data")
        .join("wiki-backups")
        .join(id2.to_string())
        .join("deploy-pipeline.md");
    assert!(backup.is_file(), "pre-overwrite backup taken");

    // Round 3: hand-edit the page (§13-3) — the next update is skipped.
    let page_path = root
        .join("knowledge")
        .join("wiki")
        .join("deploy-pipeline.md");
    let mut hand_edited = std::fs::read_to_string(&page_path).unwrap();
    hand_edited.push_str("\n<!-- 手工批注：别动这段 -->\n");
    std::fs::write(&page_path, &hand_edited).unwrap();

    let id3 = start_build(&http, &url, serde_json::json!({ "scope": "all" })).await;
    let done = wait_build(&http, &url, id3).await;
    assert_eq!(done["build"]["status"], "done", "{done:?}");
    let row = page_row(&done, "deploy-pipeline");
    assert_eq!(row["status"], "skipped", "{done:?}");
    assert!(row["error"].as_str().unwrap().contains("human-edited"));
    let current = std::fs::read_to_string(&page_path).unwrap();
    assert!(current.contains("手工批注"), "hand edit survives the build");
    assert!(
        !root
            .join("data")
            .join("wiki-backups")
            .join(id3.to_string())
            .exists(),
        "no backup for a skipped page"
    );

    let _ = std::fs::remove_dir_all(&root);
}

#[tokio::test]
async fn delete_action_removes_page_and_empty_scope_is_rejected() {
    let _serialized = serial().await;
    let replies = serde_json::json!([
        {"marker": "WIKI PLANNER", "reply": plan_json(serde_json::json!([
            page("deploy-pipeline", "部署流水线", &["deploy-guide"], "delete"),
        ]))},
    ]);
    let WikiDaemon { url, root, .. } = start_wiki_daemon(&replies).await;
    let http = reqwest::Client::new();
    put_source(&http, &url, "deploy-guide", DEPLOY_SOURCE).await;

    // Seed the page the plan will delete: it cites a source that is GONE (the
    // delete guard requires every cited source to be absent).
    std::fs::create_dir_all(root.join("knowledge").join("wiki")).unwrap();
    std::fs::write(
        root.join("knowledge")
            .join("wiki")
            .join("deploy-pipeline.md"),
        "---\ntitle: 部署流水线\nsources: [old-doc]\nsource_hashes:\n  old-doc: deadbeef\n---\n# 部署流水线\n\n旧内容\n",
    )
    .unwrap();

    let id = start_build(&http, &url, serde_json::json!({ "scope": "all" })).await;
    let done = wait_build(&http, &url, id).await;
    assert_eq!(done["build"]["status"], "done", "{done:?}");
    let row = &done["pages"].as_array().unwrap()[0];
    assert_eq!(row["status"], "deleted", "{done:?}");
    assert!(
        !root
            .join("knowledge")
            .join("wiki")
            .join("deploy-pipeline.md")
            .exists(),
        "delete removes the file"
    );

    // A scope selecting nothing is a 400.
    let resp = http
        .post(format!("{url}/api/v1/knowledge/wiki/build"))
        .json(&serde_json::json!({ "scope": ["no-such-doc"] }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);

    let _ = std::fs::remove_dir_all(&root);
}

#[tokio::test]
async fn delete_guard_protects_pages_with_live_sources() {
    // Live incident regression (2026-09-14): a scoped build's planner mistook
    // out-of-scope sources for deleted ones. The executor must refuse to delete a
    // page that still cites an on-disk source.
    let _serialized = serial().await;
    let replies = serde_json::json!([
        {"marker": "WIKI PLANNER", "reply": plan_json(serde_json::json!([
            page("deploy-pipeline", "部署流水线", &["deploy-guide"], "delete"),
        ]))},
    ]);
    let WikiDaemon { url, root, .. } = start_wiki_daemon(&replies).await;
    let http = reqwest::Client::new();
    put_source(&http, &url, "deploy-guide", DEPLOY_SOURCE).await;

    std::fs::create_dir_all(root.join("knowledge").join("wiki")).unwrap();
    std::fs::write(
        root.join("knowledge")
            .join("wiki")
            .join("deploy-pipeline.md"),
        "---\ntitle: 部署流水线\nsources: [deploy-guide]\n---\n# 部署流水线\n\n不该被删\n",
    )
    .unwrap();

    let id = start_build(&http, &url, serde_json::json!({ "scope": "all" })).await;
    let done = wait_build(&http, &url, id).await;
    assert_eq!(done["build"]["status"], "done", "{done:?}");
    let row = &done["pages"].as_array().unwrap()[0];
    assert_eq!(row["status"], "skipped", "{done:?}");
    assert!(
        row["error"]
            .as_str()
            .unwrap()
            .contains("cited sources still exist"),
        "{row:?}"
    );
    assert!(
        root.join("knowledge")
            .join("wiki")
            .join("deploy-pipeline.md")
            .exists(),
        "the page survives the bogus delete"
    );

    let _ = std::fs::remove_dir_all(&root);
}

// ---------------------------------------------------------------------------
// gen2: the citation gate (G1/G2). Every defect family gets a body that
// satisfies the pre-gen2 validation (H1 + length) and must not land.
// ---------------------------------------------------------------------------

/// One source document, one page, one build — the smallest rig that can answer
/// "did this body land".
async fn build_body(body: &str) -> (WikiDaemon, serde_json::Value, Vec<i64>) {
    let replies = serde_json::json!([
        {"marker": "WIKI PLANNER", "reply": plan_json(serde_json::json!([
            page("alpha-notes", "Alpha 笔记", &["src-alpha"], "create"),
        ]))},
        {"marker": "WIKI PAGE WRITER (slug: alpha-notes)", "reply": body},
    ]);
    let d = start_wiki_daemon(&replies).await;
    let http = reqwest::Client::new();
    let ids = put_source(
        &http,
        &d.url,
        "src-alpha",
        "# Alpha\n\nAlpha has two divisions.\n\n# Beta\n\nBeta ships in autumn.\n",
    )
    .await;
    let id = start_build(&http, &d.url, serde_json::json!({ "scope": "all" })).await;
    let done = wait_build(&http, &d.url, id).await;
    (d, done, ids)
}

#[tokio::test]
async fn the_citation_gate_fails_a_page_whose_claims_cannot_be_traced() {
    let _serialized = serial().await;
    // Every case satisfies the pre-gen2 validation (starts with `# `, under the
    // length cap) and used to land as `written`: on 0a39e5b8, a body with a
    // fabricated fact and no `## 来源` section was accepted (canary reading,
    // 2026-09-27T22:29:50+08:00).
    let cases: Vec<(&str, &str, &str)> = vec![
        (
            "fabricated fact, one section uncited",
            "# Alpha 笔记\n\n## 事实\n\nAlpha 有两个分部，年收入 123 亿美元。\n\n## 结论\n\n因此 alpha 最重要。\n<!-- cite: src-alpha#1 -->\n\n## 来源\n\n- src-alpha\n",
            "uncited section",
        ),
        (
            "no 来源 section at all",
            "# Alpha 笔记\n\n## 事实\n\nAlpha 有两个分部。\n<!-- cite: src-alpha#1 -->\n",
            "missing sources section",
        ),
        (
            "anchor names a chunk that does not exist",
            "# Alpha 笔记\n\n## 事实\n\nAlpha 有两个分部。\n<!-- cite: src-alpha#999 -->\n\n## 来源\n\n- src-alpha\n",
            "dangling citation",
        ),
        (
            "anchor names a document that is not this page's source",
            "# Alpha 笔记\n\n## 事实\n\nAlpha 有两个分部。\n<!-- cite: other-doc#1 -->\n\n## 来源\n\n- src-alpha\n",
            "unaligned citation",
        ),
        (
            "来源 lists a document the page does not cite",
            "# Alpha 笔记\n\n## 事实\n\nAlpha 有两个分部。\n<!-- cite: src-alpha#1 -->\n\n## 来源\n\n- src-alpha\n- other-doc\n",
            "sources section mismatch",
        ),
        (
            "nothing that can be cited (no content section)",
            "# Alpha 笔记\n\n## 来源\n\n- src-alpha\n",
            "no content section",
        ),
    ];
    for (what, body, want) in cases {
        let (d, done, ids) = build_body(body).await;
        assert!(ids.len() >= 2, "[{what}] precondition: two chunks, {ids:?}");
        assert_eq!(done["build"]["status"], "done", "[{what}] {done:?}");
        assert_eq!(done["build"]["pages_written"], 0, "[{what}] {done:?}");
        assert_eq!(done["build"]["pages_failed"], 1, "[{what}] {done:?}");
        let row = page_row(&done, "alpha-notes");
        assert_eq!(row["status"], "failed", "[{what}] {done:?}");
        let err = row["error"].as_str().unwrap_or_default();
        assert!(err.contains(want), "[{what}] error {err:?} lacks {want:?}");
        assert!(
            !d.root
                .join("knowledge")
                .join("wiki")
                .join("alpha-notes.md")
                .exists(),
            "[{what}] a rejected page must not be written"
        );
        // A failed page is not in the inventory either: the file is the truth.
        let pages = wiki_pages(&reqwest::Client::new(), &d.url).await;
        assert!(pages.is_empty(), "[{what}] {pages:?}");
        let _ = std::fs::remove_dir_all(&d.root);
    }
}

// ---------------------------------------------------------------------------
// gen2: the empty-shell family on its own (`CiteProblemKind::NoContentSection`)
// ---------------------------------------------------------------------------

/// A judgement must fail on the EMPTY SET. Every other rule in the gate is a
/// universal statement over the content sections ("each carries a live anchor"),
/// and a universal statement over the empty set is TRUE — so without this family a
/// shell (`# T` plus a 来源 list) would verify as `written`. Named separately from
/// the six-family loop because it must be attributed to its OWN kind: the failure
/// is not an uncited section (there are none) and not a missing 来源 section.
#[tokio::test]
async fn a_page_with_nothing_to_cite_fails_by_its_own_name() {
    let _serialized = serial().await;
    let (d, done, _ids) = build_body("# Alpha 笔记\n\n## 来源\n\n- src-alpha\n").await;
    assert_eq!(done["build"]["status"], "done", "{done:?}");
    assert_eq!(done["build"]["pages_written"], 0, "{done:?}");
    assert_eq!(done["build"]["pages_failed"], 1, "{done:?}");
    let row = page_row(&done, "alpha-notes");
    assert_eq!(row["status"], "failed", "{done:?}");
    let err = row["error"].as_str().unwrap_or_default();
    assert!(err.contains("no content section"), "{err:?}");
    // …and it is THIS family, not a neighbour: there is no section to be uncited,
    // and the 来源 list is present and correct.
    assert!(!err.contains("uncited section"), "{err:?}");
    assert!(!err.contains("missing sources section"), "{err:?}");
    assert!(!err.contains("sources section mismatch"), "{err:?}");
    assert!(!err.contains("dangling citation"), "{err:?}");
    assert!(
        !d.root
            .join("knowledge")
            .join("wiki")
            .join("alpha-notes.md")
            .exists(),
        "an empty shell must not be written"
    );
    let _ = std::fs::remove_dir_all(&d.root);
}

// ---------------------------------------------------------------------------
// gen2: dry-run vocabulary (G7)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn dry_run_has_one_vocabulary_and_rejects_the_combined_request() {
    let _serialized = serial().await;
    let replies = serde_json::json!([
        {"marker": "WIKI PLANNER", "reply": plan_json(serde_json::json!([
            page("alpha-notes", "Alpha 笔记", &["src-alpha"], "create"),
        ]))},
        {"marker": "WIKI PAGE WRITER (slug: alpha-notes)", "reply": ALPHA_BODY_V1},
    ]);
    let d = start_wiki_daemon(&replies).await;
    let http = reqwest::Client::new();
    put_source(
        &http,
        &d.url,
        "src-alpha",
        "# Alpha\n\nAlpha has two divisions.\n",
    )
    .await;

    let planned: serde_json::Value = http
        .post(format!("{}/api/v1/knowledge/wiki/build", d.url))
        .json(&serde_json::json!({ "scope": "all", "dry_run": true }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let plan_id = planned["build_id"].as_i64().unwrap();
    // The response and the ROW now say the same word. Before gen2 the response
    // said `planned` and the row said `planned_only`.
    assert_eq!(planned["status"], "planned", "{planned:?}");
    let detail: serde_json::Value = http
        .get(format!("{}/api/v1/knowledge/wiki/builds/{plan_id}", d.url))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(detail["build"]["status"], "planned", "{detail:?}");
    assert_eq!(detail["build"]["dry_run"], true, "{detail:?}");
    assert!(detail["build"]["finished_at"].is_string(), "{detail:?}");
    // A plan-only page row is not `pending`: it will never run.
    assert_eq!(detail["pages"][0]["status"], "planned", "{detail:?}");

    // No row anywhere still spells it the old way.
    let builds: serde_json::Value = http
        .get(format!("{}/api/v1/knowledge/wiki/builds?limit=100", d.url))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    for b in builds["builds"].as_array().unwrap() {
        assert_ne!(b["status"], "planned_only", "{b:?}");
    }
    let before = builds["builds"].as_array().unwrap().len();

    // `dry_run` and `confirm_plan` in ONE body used to be answered 200 with a
    // brand-new build id (the confirm_plan silently ignored). It must be rejected
    // and it must NAME both fields.
    let resp = http
        .post(format!("{}/api/v1/knowledge/wiki/build", d.url))
        .json(&serde_json::json!({ "dry_run": true, "scope": "all", "confirm_plan": plan_id }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
    let text = resp.text().await.unwrap();
    assert!(text.contains("dry_run"), "{text}");
    assert!(text.contains("confirm_plan"), "{text}");

    // …and nothing was recorded for the rejected request.
    let builds: serde_json::Value = http
        .get(format!("{}/api/v1/knowledge/wiki/builds?limit=100", d.url))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(builds["builds"].as_array().unwrap().len(), before);

    let _ = std::fs::remove_dir_all(&d.root);
}

// ---------------------------------------------------------------------------
// gen2: freshness / invalidation (G3/G4)
// ---------------------------------------------------------------------------

async fn server_built_at(http: &reqwest::Client, url: &str, slug: &str) -> String {
    wiki_pages(http, url)
        .await
        .into_iter()
        .find(|p| p["slug"] == slug)
        .and_then(|p| p["built_at"].as_str().map(str::to_string))
        .unwrap_or_default()
}

#[tokio::test]
async fn a_stale_page_is_repaired_even_when_the_plan_says_keep() {
    let _serialized = serial().await;
    let create = serde_json::json!([
        {"marker": "WIKI PLANNER", "reply": plan_json(serde_json::json!([
            page("alpha-notes", "Alpha 笔记", &["src-alpha"], "create"),
        ]))},
        {"marker": "WIKI PAGE WRITER (slug: alpha-notes)", "reply": ALPHA_BODY_V1},
    ]);
    let d = start_wiki_daemon(&create).await;
    let http = reqwest::Client::new();
    put_source(
        &http,
        &d.url,
        "src-alpha",
        "# Alpha\n\nAlpha has two divisions.\n",
    )
    .await;
    let id1 = start_build(&http, &d.url, serde_json::json!({ "scope": "all" })).await;
    let done = wait_build(&http, &d.url, id1).await;
    assert_eq!(done["build"]["pages_written"], 1, "{done:?}");
    let built_at_1 = server_built_at(&http, &d.url, "alpha-notes").await;

    // The source moves under the page: the drift is visible at once…
    let ids2 = put_source(
        &http,
        &d.url,
        "src-alpha",
        "# Alpha\n\nAlpha has THREE divisions now.\n",
    )
    .await;
    let pages = wiki_pages(&http, &d.url).await;
    assert_eq!(pages[0]["stale"], true, "{pages:?}");
    assert_eq!(pages[0]["freshness"], "stale", "{pages:?}");
    assert_eq!(pages[0]["stale_sources"], serde_json::json!(["src-alpha"]));
    // …and the OBSERVATION TIME now comes from the READ path (t74/A2): whichever
    // read first sees the drift stamps it. "Stale with no start" was a reading that
    // cannot answer the one question a human asks of it ("just broke, or broken for
    // months?") and the field the G3 criterion is computed from, so it is gone.
    // This assertion used to pin the OPPOSITE judgement — "the observation time is
    // written by the next build … nothing else observes drift" — which is exactly
    // the defect the audit named; the judgement moved here, with the code.
    let since = pages[0]["stale_since"].as_str().unwrap_or_default();
    assert!(
        !since.is_empty(),
        "stale_since must be observed on the read path: {pages:?}"
    );
    // …and a second read returns the SAME start: this is a first observation, not
    // a fresh `now` on every poll.
    let again = wiki_pages(&http, &d.url).await;
    assert_eq!(
        again[0]["stale_since"], pages[0]["stale_since"],
        "reading twice must not move the start: {again:?}"
    );

    // The planner now answers `keep` for the page it should repair. The repair
    // re-cites the source, and chunk ids are NOT stable across a re-ingest, so the
    // reply cites the ids the knowledge base reports NOW.
    write_replies(
        &d.replies,
        &serde_json::json!([
            {"marker": "WIKI PLANNER", "reply": plan_json(serde_json::json!([
                page("alpha-notes", "Alpha 笔记", &["src-alpha"], "keep"),
            ]))},
            {"marker": "WIKI PAGE WRITER (slug: alpha-notes)",
             "reply": format!("# Alpha 笔记\n\n## 事实\n\nAlpha 现在有三个分部。\n<!-- cite: src-alpha#{} -->\n\n## 来源\n\n- src-alpha\n", ids2[0])},
        ]),
    );
    let id2 = start_build(&http, &d.url, serde_json::json!({ "scope": "changed" })).await;
    let done = wait_build(&http, &d.url, id2).await;
    assert_eq!(done["build"]["status"], "done", "{done:?}");
    // NOT "skipped/kept by plan": a stale page may not end a build unchanged. The
    // row also records the action PERFORMED — the plan said `keep`, the executor
    // updated (G4 asks exactly this).
    let row = page_row(&done, "alpha-notes");
    assert_eq!(row["status"], "written", "{done:?}");
    assert_eq!(row["action"], "update", "{done:?}");

    let pages = wiki_pages(&http, &d.url).await;
    assert_eq!(pages[0]["stale"], false, "{pages:?}");
    assert_eq!(pages[0]["freshness"], "fresh", "{pages:?}");
    let rebuilt =
        std::fs::read_to_string(d.root.join("knowledge").join("wiki").join("alpha-notes.md"))
            .unwrap();
    assert!(rebuilt.contains("三个分部"), "{rebuilt}");
    assert_ne!(
        server_built_at(&http, &d.url, "alpha-notes").await,
        built_at_1
    );

    let _ = std::fs::remove_dir_all(&d.root);
}

/// A page that stays stale ACROSS a build (hand-edited, so §13-3 freezes it) is
/// where `stale_since` becomes observable, and where the index marker and the
/// recorded reason must both appear.
#[tokio::test]
async fn a_page_that_stays_stale_records_since_and_says_why() {
    let _serialized = serial().await;
    let replies = serde_json::json!([
        {"marker": "WIKI PLANNER", "reply": plan_json(serde_json::json!([
            page("alpha-notes", "Alpha 笔记", &["src-alpha"], "create"),
        ]))},
        {"marker": "WIKI PAGE WRITER (slug: alpha-notes)", "reply": ALPHA_BODY_V1},
    ]);
    let d = start_wiki_daemon(&replies).await;
    let http = reqwest::Client::new();
    put_source(
        &http,
        &d.url,
        "src-alpha",
        "# Alpha\n\nAlpha has two divisions.\n",
    )
    .await;
    let id1 = start_build(&http, &d.url, serde_json::json!({ "scope": "all" })).await;
    let done = wait_build(&http, &d.url, id1).await;
    assert_eq!(done["build"]["pages_written"], 1, "{done:?}");

    // A human annotates the page AND the source moves: two reasons at once.
    let path = d.root.join("knowledge").join("wiki").join("alpha-notes.md");
    let mut text = std::fs::read_to_string(&path).unwrap();
    text.push_str("\n<!-- 人工批注 -->\n");
    std::fs::write(&path, &text).unwrap();
    put_source(
        &http,
        &d.url,
        "src-alpha",
        "# Alpha\n\nAlpha has four divisions.\n",
    )
    .await;

    let id2 = start_build(&http, &d.url, serde_json::json!({ "scope": "all" })).await;
    let done = wait_build(&http, &d.url, id2).await;
    assert_eq!(done["build"]["status"], "done", "{done:?}");
    let row = page_row(&done, "alpha-notes");
    assert_eq!(row["status"], "skipped", "{done:?}");
    let err = row["error"].as_str().unwrap();
    assert!(err.contains("human-edited"), "{err}");
    // The freeze is RECORDED, not merely executed (G6).
    assert!(
        err.contains("recorded: human-edited since build #"),
        "{err}"
    );

    let pages = wiki_pages(&http, &d.url).await;
    assert_eq!(pages[0]["stale"], true, "{pages:?}");
    assert_eq!(pages[0]["freshness"], "stale", "{pages:?}");
    assert_eq!(pages[0]["stale_sources"], serde_json::json!(["src-alpha"]));
    // RV-D-3: when `stale_sources` has nothing to name (a page whose recorded
    // `source_hashes` is empty), the REASON still answers "why is this stale" —
    // and it also answers it here, where the names are present.
    let reasons: Vec<&str> = pages[0]["stale_reasons"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap_or_default())
        .collect();
    assert!(reasons.contains(&"source hash drift"), "{pages:?}");
    assert!(reasons.contains(&"hand edited"), "{pages:?}");
    // RV-D-1: the page is stale, so the build's recorded coverage is no longer
    // quotable — unknown, not a stale-but-proud 1.0.
    assert!(pages[0]["cite_coverage"].is_null(), "{pages:?}");
    let since = pages[0]["stale_since"].as_str().unwrap_or_default();
    assert!(!since.is_empty(), "stale_since must be observed: {pages:?}");
    assert!(since.contains('T'), "RFC3339: {since}");
    // The human-facing catalogue agrees with the API (same function).
    let index =
        std::fs::read_to_string(d.root.join("knowledge").join("wiki").join("index.md")).unwrap();
    assert!(index.contains("⚠️ 源已更新"), "{index}");

    // And the correction loop holds the reason.
    let corrections = ruagent_daemon::wiki::corrections(&d.db, "alpha-notes").await;
    assert_eq!(corrections.len(), 1, "{corrections:?}");
    assert_eq!(corrections[0].author, "daemon");
    assert!(corrections[0].reason.contains("human-edited"));
    assert!(corrections[0].at.contains('T'));

    let _ = std::fs::remove_dir_all(&d.root);
}

/// A pin freezes the page whatever the plan says, and the reason travels with the
/// skip. The negative side is the same page WITHOUT a pin: it is rewritten.
#[tokio::test]
async fn a_pin_skips_the_page_and_the_reason_is_visible() {
    let _serialized = serial().await;
    let replies = serde_json::json!([
        {"marker": "WIKI PLANNER", "reply": plan_json(serde_json::json!([
            page("alpha-notes", "Alpha 笔记", &["src-alpha"], "update"),
        ]))},
        {"marker": "WIKI PAGE WRITER (slug: alpha-notes)", "reply": ALPHA_BODY_V1},
    ]);
    let d = start_wiki_daemon(&replies).await;
    let http = reqwest::Client::new();
    put_source(
        &http,
        &d.url,
        "src-alpha",
        "# Alpha\n\nAlpha has two divisions.\n",
    )
    .await;
    let id1 = start_build(&http, &d.url, serde_json::json!({ "scope": "all" })).await;
    let done = wait_build(&http, &d.url, id1).await;
    assert_eq!(done["build"]["pages_written"], 1, "{done:?}");

    // WITHOUT the pin the plan's `update` rewrites the page.
    let id2 = start_build(&http, &d.url, serde_json::json!({ "scope": "all" })).await;
    let done = wait_build(&http, &d.url, id2).await;
    assert_eq!(
        page_row(&done, "alpha-notes")["status"],
        "written",
        "{done:?}"
    );

    // An empty reason must be REJECTED: the reason is the point of the table.
    let rejected = ruagent_daemon::wiki::add_correction(
        &d.db,
        &ruagent_daemon::wiki::Correction {
            slug: "alpha-notes".into(),
            kind: ruagent_daemon::wiki::CorrectionKind::Pin,
            reason: "   ".into(),
            author: "wiki".into(),
            at: String::new(),
        },
    )
    .await;
    assert!(rejected.is_err(), "an empty reason must not be stored");

    let pinned = ruagent_daemon::wiki::add_correction(
        &d.db,
        &ruagent_daemon::wiki::Correction {
            slug: "alpha-notes".into(),
            kind: ruagent_daemon::wiki::CorrectionKind::Pin,
            reason: "人工核对过，冻结".into(),
            author: "wiki".into(),
            at: String::new(),
        },
    )
    .await
    .expect("pin");
    assert!(pinned > 0);

    let id3 = start_build(&http, &d.url, serde_json::json!({ "scope": "all" })).await;
    let done = wait_build(&http, &d.url, id3).await;
    assert_eq!(done["build"]["status"], "done", "{done:?}");
    let row = page_row(&done, "alpha-notes");
    assert_eq!(row["status"], "skipped", "{done:?}");
    let err = row["error"].as_str().unwrap();
    assert!(err.contains("frozen:"), "{err}");
    assert!(err.contains("人工核对过，冻结"), "{err}");

    let pages = wiki_pages(&http, &d.url).await;
    assert_eq!(pages[0]["frozen_by"], "pin", "{pages:?}");

    // Releasing it unfreezes the page again.
    ruagent_daemon::wiki::add_correction(
        &d.db,
        &ruagent_daemon::wiki::Correction {
            slug: "alpha-notes".into(),
            kind: ruagent_daemon::wiki::CorrectionKind::Release,
            reason: "核对完成，解冻".into(),
            author: "wiki".into(),
            at: String::new(),
        },
    )
    .await
    .expect("release");
    let pages = wiki_pages(&http, &d.url).await;
    assert!(pages[0]["frozen_by"].is_null(), "{pages:?}");

    let _ = std::fs::remove_dir_all(&d.root);
}

// ---------------------------------------------------------------------------
// gen2: the third freshness state (G3)
// ---------------------------------------------------------------------------

/// A freshness reading the knowledge base cannot answer must be `unknown`, never
/// folded into `fresh`. The window that produces it is real: a page exists on
/// disk while the evidence it cites is not indexable yet (the scanner indexes the
/// wiki directory every 60s, so right after a page appears the KB can still hold
/// nothing).
#[tokio::test]
async fn an_unanswerable_freshness_reading_is_unknown_never_fresh() {
    let _serialized = serial().await;
    let _d = start_wiki_daemon(&serde_json::json!([])).await;
    let http = reqwest::Client::new();
    std::fs::create_dir_all(_d.root.join("knowledge").join("wiki")).unwrap();
    std::fs::write(
        _d.root
            .join("knowledge")
            .join("wiki")
            .join("ghost-notes.md"),
        "---\ntitle: Ghost\nsummary: s\naliases: []\nentities: []\nsources: []\nsource_hashes: {}\nstatus: generated\ngenerated_at: \"2026-09-28T00:00:00+00:00\"\ngenerator: mock\nbuild: 1\nverified: verified\ncitations:\n  - section: 事实\n    document: ghost-src\n    chunk_id: 1\n    chunk_hash: aaaa\n---\n\n# Ghost\n\n## 事实\n\nfact\n<!-- cite: ghost-src#1 -->\n\n## 来源\n\n- ghost-src\n",
    )
    .unwrap();

    // The document count is read FIRST: it is the precondition of the state under
    // test, so a rig that has already indexed something fails here with the reason.
    let docs: serde_json::Value = http
        .get(format!("{}/api/v1/knowledge/documents", _d.url))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        docs["documents"].as_array().unwrap().len(),
        0,
        "precondition: nothing is indexed yet: {docs:?}"
    );

    let pages = wiki_pages(&http, &_d.url).await;
    assert_eq!(pages.len(), 1, "{pages:?}");
    assert_eq!(pages[0]["freshness"], "unknown", "{pages:?}");
    // D.2 freezes `stale: bool` (so the CLI/panel keep working) and makes
    // `freshness` the carrier of the third state, with the invariant
    // `stale == (freshness == "stale")`. "Cannot tell" therefore reads as
    // `freshness="unknown"`, and the reading it must NEVER produce is
    // `freshness="fresh"` — which is what this pins down.
    assert_eq!(pages[0]["stale"], false, "{pages:?}");
    assert_eq!(
        pages[0]["stale"] == serde_json::json!(true),
        pages[0]["freshness"] == serde_json::json!("stale"),
        "the frozen invariant `stale == (freshness == \"stale\")`: {pages:?}"
    );
    assert_ne!(pages[0]["freshness"], "fresh", "{pages:?}");
    // RV-D-1: a page with no `wiki_pages` row has NO recorded coverage. The old
    // shape reported 1.0 here (a constructively full mark) while the SAME response
    // listed `uncited_sections: ["事实"]`.
    assert!(
        pages[0]["cite_coverage"].is_null(),
        "no recorded build reading ⇒ unknown, never a full mark: {pages:?}"
    );
    assert!(
        !pages[0]["uncited_sections"].as_array().unwrap().is_empty(),
        "the anchor does not resolve, so its section is uncited: {pages:?}"
    );
    // RV-D-3: "why must I not trust this reading" stays answerable — but the two
    // third states answer it with DIFFERENT words, because "the KB could not
    // answer" is not a drift reason. `unknown` ⇒ `unknown_cause` carries it, and
    // the reason list stays empty rather than being padded with a fake reason.
    assert_eq!(
        pages[0]["stale_reasons"],
        serde_json::json!([]),
        "{pages:?}"
    );
    assert!(
        pages[0]["unknown_cause"].as_str().unwrap_or_default().len() > 4,
        "an unanswerable reading must say why: {pages:?}"
    );

    let _ = std::fs::remove_dir_all(&_d.root);
}

/// RV-D-1's black-box criterion, on a page that HAS a recorded build reading: once
/// its anchor stops resolving, the response must not keep quoting the build's 1.0.
/// The recorded number describes the page as it was built, so it may only be
/// quoted while the page still matches it — otherwise the same response would say
/// 100% coverage and, at the same time, list the section as uncited.
#[tokio::test]
async fn an_unresolvable_anchor_cannot_keep_reporting_full_coverage() {
    let _serialized = serial().await;
    let replies = serde_json::json!([
        {"marker": "WIKI PLANNER", "reply": plan_json(serde_json::json!([
            page("alpha-notes", "Alpha 笔记", &["src-alpha"], "create"),
        ]))},
        // cites the SECOND chunk of the source (the one whose re-ingest removes it)
        {"marker": "WIKI PAGE WRITER (slug: alpha-notes)",
         "reply": "# Alpha 笔记\n\n## 事实\n\nBeta 秋天发布。\n<!-- cite: src-alpha#2 -->\n\n## 来源\n\n- src-alpha\n"},
    ]);
    let d = start_wiki_daemon(&replies).await;
    let http = reqwest::Client::new();
    let ids = put_source(
        &http,
        &d.url,
        "src-alpha",
        "# Alpha\n\nAlpha has two divisions.\n\n# Beta\n\nBeta ships in autumn.\n",
    )
    .await;
    assert!(ids.len() >= 2, "precondition: two chunks, {ids:?}");
    let id = start_build(&http, &d.url, serde_json::json!({ "scope": "all" })).await;
    let done = wait_build(&http, &d.url, id).await;
    assert_eq!(done["build"]["pages_written"], 1, "{done:?}");

    // While the page is fresh, the BUILD's reading is quoted: 1/1.
    let pages = wiki_pages(&http, &d.url).await;
    assert_eq!(pages[0]["freshness"], "fresh", "{pages:?}");
    assert_eq!(pages[0]["cite_coverage"], 1.0, "{pages:?}");
    assert_eq!(
        pages[0]["stale_reasons"],
        serde_json::json!([]),
        "{pages:?}"
    );

    // The source is re-ingested with ONE section, so the anchored chunk is gone.
    put_source(
        &http,
        &d.url,
        "src-alpha",
        "# Alpha\n\nAlpha has one division now.\n",
    )
    .await;

    let pages = wiki_pages(&http, &d.url).await;
    let p = &pages[0];
    assert_eq!(p["stale"], true, "{p:?}");
    assert_eq!(p["freshness"], "stale", "{p:?}");
    // The invariant RV-D-1 asks for, asserted directly: the SAME response may not
    // carry a full coverage next to an uncited section.
    let coverage = p["cite_coverage"].as_f64();
    let uncited = p["uncited_sections"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    // Named parts, then one `&&`: `nonminimal_bool` rejects the compact
    // `!(a && !b)` form (it rewrites to `!a || b`, which no longer reads like the
    // invariant). The assertion itself is unchanged — this is the RV-D-1 judgement
    // body, and it stays asserted verbatim in meaning.
    let full_coverage = coverage == Some(1.0);
    let uncited_non_empty = !uncited.is_empty();
    assert!(
        coverage.is_none(),
        "the recorded 1.0 no longer applies: {p:?}"
    );
    assert!(!uncited.is_empty(), "{p:?}");
    assert!(
        !(full_coverage && uncited_non_empty),
        "full coverage next to an uncited section: {p:?}"
    );
    // …and the reason is named, so "why" is answerable without guessing.
    let reasons: Vec<&str> = p["stale_reasons"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap_or_default())
        .collect();
    assert!(
        reasons.contains(&"chunk missing") || reasons.contains(&"chunk hash drift"),
        "{p:?}"
    );

    // The invariants hold for EVERY page the rig ever built (cheap global check).
    let pages = wiki_pages(&http, &d.url).await;
    for p in &pages {
        let full = p["cite_coverage"].as_f64() == Some(1.0);
        let uncited = p["uncited_sections"].as_array().map(|a| !a.is_empty());
        assert!(
            !(full && uncited == Some(true)),
            "cite_coverage=1.0 with uncited sections: {p:?}"
        );
        assert_eq!(
            p["cite_coverage"].is_null(),
            p["freshness"] != "fresh",
            "coverage is quoted only on a fresh page: {p:?}"
        );
    }

    let _ = std::fs::remove_dir_all(&d.root);
}

// ---------------------------------------------------------------------------
// gen2: link-graph readings (G5)
// ---------------------------------------------------------------------------

const ALPHA_LINKS_BODY: &str = "# Alpha\n\n## 事实\n\nfact\n<!-- cite: src-alpha#1 -->\n\n## 相关\n\n- [[beta-notes]]\n- [[missing-page]]\n\n## 来源\n\n- src-alpha\n";
const BETA_LINKS_BODY: &str = "# Beta\n\n## 事实\n\nfact\n<!-- cite: src-beta#2 -->\n\n## 相关\n\n- [[alpha-notes]]\n- [[beta-notes]]\n\n## 来源\n\n- src-beta\n";

#[tokio::test]
async fn the_two_graph_endpoints_cannot_disagree() {
    let _serialized = serial().await;
    let replies = serde_json::json!([
        {"marker": "WIKI PLANNER", "reply": plan_json(serde_json::json!([
            page("alpha-notes", "Alpha 笔记", &["src-alpha"], "create"),
            page("beta-notes", "Beta 笔记", &["src-beta"], "create"),
        ]))},
        {"marker": "WIKI PAGE WRITER (slug: alpha-notes)", "reply": ALPHA_LINKS_BODY},
        {"marker": "WIKI PAGE WRITER (slug: beta-notes)", "reply": BETA_LINKS_BODY},
    ]);
    let d = start_wiki_daemon(&replies).await;
    let http = reqwest::Client::new();
    put_source(
        &http,
        &d.url,
        "src-alpha",
        "# Alpha\n\nAlpha has two divisions.\n",
    )
    .await;
    put_source(
        &http,
        &d.url,
        "src-beta",
        "# Beta\n\nBeta ships in autumn.\n",
    )
    .await;
    let id = start_build(&http, &d.url, serde_json::json!({ "scope": "all" })).await;
    let done = wait_build(&http, &d.url, id).await;
    assert_eq!(done["build"]["pages_written"], 2, "{done:?}");

    let links: serde_json::Value = http
        .get(format!("{}/api/v1/knowledge/wiki/links", d.url))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let pages = wiki_pages(&http, &d.url).await;

    // Per-node degrees are ONE reading: `/wiki/pages` and `/wiki/links` come out
    // of the same computation, so they cannot disagree. Before gen2 they did on 2
    // of the 4 live pages (raw targets vs resolved edges).
    for p in &pages {
        let slug = p["slug"].as_str().unwrap();
        let degree = links["degrees"]
            .as_array()
            .unwrap()
            .iter()
            .find(|x| x["slug"] == slug)
            .unwrap_or_else(|| panic!("no degree for {slug}: {links:?}"));
        assert_eq!(p["links_out"], degree["links_out"], "{slug}");
        assert_eq!(p["links_in"], degree["links_in"], "{slug}");
        assert_eq!(p["links_out_broken"], degree["links_out_broken"], "{slug}");
    }
    // The wanted page knows who wants it.
    assert_eq!(links["broken"], serde_json::json!(["missing-page"]));
    assert_eq!(links["wanted"][0]["slug"], "missing-page");
    assert_eq!(
        links["wanted"][0]["demanders"],
        serde_json::json!(["alpha-notes"])
    );
    assert_eq!(links["wanted"][0]["demand_count"], 1);
    // A self-link is reported instead of silently dropped.
    assert_eq!(
        links["self_links"].as_array().unwrap().len(),
        1,
        "{links:?}"
    );
    assert_eq!(links["self_links"][0]["src"], "beta-notes");
    assert_eq!(links["self_links"][0]["dst"], "beta-notes");
    // alpha reaches beta (resolved) + missing-page (broken); beta reaches alpha,
    // and its self-link is NOT an out-link.
    let alpha = pages.iter().find(|p| p["slug"] == "alpha-notes").unwrap();
    let beta = pages.iter().find(|p| p["slug"] == "beta-notes").unwrap();
    assert_eq!(alpha["links_out"], 2, "{alpha:?}");
    assert_eq!(alpha["links_out_broken"], 1, "{alpha:?}");
    assert_eq!(beta["links_out"], 1, "{beta:?}");
    assert_eq!(beta["self_links"], 1, "{beta:?}");
    assert_eq!(links["unreachable"], serde_json::json!([]), "{links:?}");
    assert_eq!(links["orphans"], serde_json::json!([]), "{links:?}");

    // The build's graph reading was PERSISTED (before gen2 the numbers were
    // computed on every request and thrown away).
    let reading = ruagent_daemon::wiki::graph_reading(&d.db, id)
        .await
        .expect("a completed build records its graph reading");
    assert_eq!(reading.nodes, 2, "{reading:?}");
    assert_eq!(reading.edges, 2, "{reading:?}");
    assert_eq!(reading.broken, 1, "{reading:?}");
    assert_eq!(reading.self_links, 1, "{reading:?}");
    assert!(reading.read_at.contains('T'));

    let _ = std::fs::remove_dir_all(&d.root);
}

/// A page whose file disappears without a build deleting it leaves its hash row
/// behind: `Knowledge::scan` owns `documents`, nobody owned `wiki_page_hashes`.
/// The live database had exactly this (5 rows for 4 pages).
#[tokio::test]
async fn a_page_hash_with_no_page_is_swept_by_the_next_build() {
    let _serialized = serial().await;
    let create = serde_json::json!([
        {"marker": "WIKI PLANNER", "reply": plan_json(serde_json::json!([
            page("alpha-notes", "Alpha 笔记", &["src-alpha"], "create"),
        ]))},
        {"marker": "WIKI PAGE WRITER (slug: alpha-notes)", "reply": ALPHA_BODY_V1},
    ]);
    let d = start_wiki_daemon(&create).await;
    let http = reqwest::Client::new();
    put_source(
        &http,
        &d.url,
        "src-alpha",
        "# Alpha\n\nAlpha has two divisions.\n",
    )
    .await;
    let id1 = start_build(&http, &d.url, serde_json::json!({ "scope": "all" })).await;
    wait_build(&http, &d.url, id1).await;
    assert_eq!(
        ruagent_daemon::wiki::page_hashes(&d.db).await,
        vec![("alpha-notes".to_string(), id1)]
    );

    // The page's DOCUMENT goes away without any build deleting it (the file goes
    // with it), and the plan moves on to another page. This is the live shape of
    // the leak: the indexing side forgets, `wiki_page_hashes` does not.
    let docs: serde_json::Value = http
        .get(format!("{}/api/v1/knowledge/documents", d.url))
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
        .find(|x| x["name"].as_str() == Some("wiki/alpha-notes"))
        .unwrap_or_else(|| panic!("the page is indexed as a document: {docs:?}"))["id"]
        .as_i64()
        .unwrap();
    let deleted = http
        .delete(format!("{}/api/v1/knowledge/documents/{doc_id}", d.url))
        .send()
        .await
        .unwrap();
    assert!(deleted.status().is_success(), "{deleted:?}");
    assert!(
        !d.root
            .join("knowledge")
            .join("wiki")
            .join("alpha-notes.md")
            .exists(),
        "deleting the document removes the file too"
    );
    // The chunk ids are read back before the reply cites one.
    let ids = put_source(
        &http,
        &d.url,
        "src-beta",
        "# Beta\n\nBeta ships in autumn.\n",
    )
    .await;
    write_replies(
        &d.replies,
        &serde_json::json!([
            {"marker": "WIKI PLANNER", "reply": plan_json(serde_json::json!([
                page("beta-notes", "Beta 笔记", &["src-beta"], "create"),
            ]))},
            {"marker": "WIKI PAGE WRITER (slug: beta-notes)",
             "reply": format!("# Beta 笔记\n\n## 事实\n\nBeta 秋天发布。\n<!-- cite: src-beta#{} -->\n\n## 来源\n\n- src-beta\n", ids[0])},
        ]),
    );
    let id2 = start_build(&http, &d.url, serde_json::json!({ "scope": "all" })).await;
    let done = wait_build(&http, &d.url, id2).await;
    assert_eq!(done["build"]["status"], "done", "{done:?}");

    let hashes = ruagent_daemon::wiki::page_hashes(&d.db).await;
    assert!(
        !hashes.iter().any(|(slug, _)| slug == "alpha-notes"),
        "the leak must be swept: {hashes:?}"
    );
    assert!(
        hashes.iter().any(|(slug, _)| slug == "beta-notes"),
        "{hashes:?}"
    );

    let _ = std::fs::remove_dir_all(&d.root);
}

// ---------------------------------------------------------------------------
// gen2: renames (G9)
// ---------------------------------------------------------------------------

/// Renaming a page used to be a delete plus a create: the topic kept its links
/// nowhere and the old slug vanished (`rose-gardening` → `gardening-roses` in the
/// live database). A delete whose topic survives under another slug is a MOVE.
#[tokio::test]
async fn a_rename_moves_the_topic_instead_of_destroying_it() {
    let _serialized = serial().await;
    let create = serde_json::json!([
        {"marker": "WIKI PLANNER", "reply": plan_json(serde_json::json!([
            page("alpha-notes", "Alpha 笔记", &["src-alpha"], "create"),
        ]))},
        {"marker": "WIKI PAGE WRITER (slug: alpha-notes)", "reply": ALPHA_BODY_V1},
    ]);
    let d = start_wiki_daemon(&create).await;
    let http = reqwest::Client::new();
    put_source(
        &http,
        &d.url,
        "src-alpha",
        "# Alpha\n\nAlpha has two divisions.\n",
    )
    .await;
    let id1 = start_build(&http, &d.url, serde_json::json!({ "scope": "all" })).await;
    let done = wait_build(&http, &d.url, id1).await;
    assert_eq!(done["build"]["pages_written"], 1, "{done:?}");

    write_replies(
        &d.replies,
        &serde_json::json!([
            {"marker": "WIKI PLANNER", "reply": plan_json(serde_json::json!([
                page("alpha-notes", "Alpha 笔记", &["src-alpha"], "delete"),
                page("alpha-facts", "Alpha 事实", &["src-alpha"], "create"),
            ]))},
            {"marker": "WIKI PAGE WRITER (slug: alpha-facts)",
             "reply": "# Alpha 事实\n\n## 事实\n\nAlpha 有两个分部。\n<!-- cite: src-alpha#1 -->\n\n## 来源\n\n- src-alpha\n"},
        ]),
    );
    let id2 = start_build(&http, &d.url, serde_json::json!({ "scope": "all" })).await;
    let done = wait_build(&http, &d.url, id2).await;
    assert_eq!(done["build"]["status"], "done", "{done:?}");
    let old = page_row(&done, "alpha-notes");
    assert_eq!(old["status"], "renamed", "{done:?}");
    assert!(
        old["error"]
            .as_str()
            .unwrap()
            .contains("renamed to alpha-facts"),
        "{old:?}"
    );
    assert_eq!(
        page_row(&done, "alpha-facts")["status"],
        "written",
        "{done:?}"
    );
    for p in done["pages"].as_array().unwrap() {
        assert_ne!(p["status"], "deleted", "a rename is not a loss: {done:?}");
    }

    let pages = wiki_pages(&http, &d.url).await;
    assert_eq!(pages.len(), 1, "{pages:?}");
    assert_eq!(pages[0]["slug"], "alpha-facts");
    assert!(
        pages[0]["aliases"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a == "alpha-notes"),
        "the old slug is recorded on the survivor: {pages:?}"
    );

    let _ = std::fs::remove_dir_all(&d.root);
}
