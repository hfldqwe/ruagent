//! Wiki pipeline integration: the full three-stage build driven by the
//! scripted mock agent (marker-keyed replies — each one-shot ACP call
//! spawns a fresh process, so order-based scripts cannot work).

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

/// A test daemon whose ONLY enabled agent is the scripted mock. The
/// replies file outlives the daemon (agents spawn per call).
async fn start_wiki_daemon(replies: &serde_json::Value) -> (String, std::path::PathBuf) {
    let seq = DIR_SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let root = std::env::temp_dir().join(format!("ruagent-wiki-{}-{seq}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let config_dir = root.join("config");
    std::fs::create_dir_all(&config_dir).unwrap();
    let replies_path = root.join("replies.json");
    std::fs::write(&replies_path, serde_json::to_string(replies).unwrap()).unwrap();

    // Windows paths must use forward slashes here: TOML basic strings
    // treat backslashes as escapes.
    let bin = env!("CARGO_BIN_EXE_ruagent-mock-agent").replace('\\', "/");
    let replies = replies_path.to_string_lossy().replace('\\', "/");
    std::fs::write(
        config_dir.join("agents.toml"),
        format!(
            "[agent.mock]
harness = \"mock\"
command = \"{bin} --behavior scripted --replies {replies}\"
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
    (format!("http://{addr}"), root)
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

const DEPLOY_BODY: &str = "# 部署流水线\n\n部署脚本位于 [[tea-notes|茶水间守则]] 旁边的 scripts/deploy.sh。\n\n## 步骤\n\n从仓库根目录运行，合并后执行。\n\n## 来源\n\n- deploy-guide\n";

const TEA_BODY: &str =
    "# 茶水间守则\n\n伯爵茶配柠檬与蜂蜜。相关：[[deploy-pipeline]]。\n\n## 来源\n\n- tea-doc\n";

#[tokio::test]
async fn dry_run_plan_then_confirmed_build_lands_pages() {
    let _serialized = ONE_BUILD_AT_A_TIME
        .get_or_init(|| tokio::sync::Mutex::new(()))
        .lock()
        .await;
    let replies = serde_json::json!([
        {"marker": "WIKI PLANNER", "reply": plan_json(serde_json::json!([
            page("deploy-pipeline", "部署流水线", &["deploy-guide"], "create"),
            page("tea-notes", "茶水间守则", &["tea-doc"], "create"),
        ]))},
        {"marker": "WIKI PAGE WRITER (slug: deploy-pipeline)", "reply": DEPLOY_BODY},
        {"marker": "WIKI PAGE WRITER (slug: tea-notes)", "reply": TEA_BODY},
    ]);
    let (url, root) = start_wiki_daemon(&replies).await;
    let http = reqwest::Client::new();

    // Two source documents (top level).
    for (name, content) in [
        (
            "deploy-guide",
            "# Deploy\n\nThe deploy script lives in scripts/deploy.sh. Run from the root.",
        ),
        ("tea-doc", "# Tea\n\nEarl grey with bergamot and honey."),
    ] {
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
    }

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
    // index.md regenerated.
    let index =
        std::fs::read_to_string(root.join("knowledge").join("wiki").join("index.md")).unwrap();
    assert!(index.contains("[[deploy-pipeline|部署流水线]]"));
    assert!(index.contains("[[tea-notes|茶水间守则]]"));

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

    // Dry-run builds are listed for review; executing one is rejected
    // after its pages moved on? (Not applicable here — but a
    // non-existent confirm id must 400.)
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
    let _serialized = ONE_BUILD_AT_A_TIME
        .get_or_init(|| tokio::sync::Mutex::new(()))
        .lock()
        .await;
    // Round 1: create the page.
    let replies = serde_json::json!([
        {"marker": "WIKI PLANNER", "reply": plan_json(serde_json::json!([
            page("deploy-pipeline", "部署流水线", &["deploy-guide"], "create"),
        ]))},
        {"marker": "WIKI PAGE WRITER (slug: deploy-pipeline)", "reply": DEPLOY_BODY},
    ]);
    let (url, root) = start_wiki_daemon(&replies).await;
    let http = reqwest::Client::new();
    http.put(format!("{url}/api/v1/knowledge/raw/deploy-guide"))
        .json(&serde_json::json!({
            "content": "# Deploy\n\nThe deploy script lives in scripts/deploy.sh. Run from the root."
        }))
        .send()
        .await
        .unwrap();
    let started: serde_json::Value = http
        .post(format!("{url}/api/v1/knowledge/wiki/build"))
        .json(&serde_json::json!({ "scope": "all" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let id1 = started["build_id"].as_i64().unwrap();
    let done = wait_build(&http, &url, id1).await;
    assert_eq!(done["build"]["status"], "done", "{done:?}");

    // Round 2 (same daemon, same script): update the UNEDITED page —
    // the pre-write backup must appear.
    let started: serde_json::Value = http
        .post(format!("{url}/api/v1/knowledge/wiki/build"))
        .json(&serde_json::json!({ "scope": "all" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let id2 = started["build_id"].as_i64().unwrap();
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

    let started: serde_json::Value = http
        .post(format!("{url}/api/v1/knowledge/wiki/build"))
        .json(&serde_json::json!({ "scope": "all" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let id3 = started["build_id"].as_i64().unwrap();
    let done = wait_build(&http, &url, id3).await;
    assert_eq!(done["build"]["status"], "done", "{done:?}");
    let page_row = done["pages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["slug"].as_str() == Some("deploy-pipeline"))
        .unwrap();
    assert_eq!(page_row["status"], "skipped", "{done:?}");
    assert!(page_row["error"].as_str().unwrap().contains("human-edited"));
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
    let _serialized = ONE_BUILD_AT_A_TIME
        .get_or_init(|| tokio::sync::Mutex::new(()))
        .lock()
        .await;
    let replies = serde_json::json!([
        {"marker": "WIKI PLANNER", "reply": plan_json(serde_json::json!([
            page("deploy-pipeline", "部署流水线", &["deploy-guide"], "delete"),
        ]))},
    ]);
    let (url, root) = start_wiki_daemon(&replies).await;
    let http = reqwest::Client::new();
    http.put(format!("{url}/api/v1/knowledge/raw/deploy-guide"))
        .json(&serde_json::json!({
            "content": "# Deploy\n\nThe deploy script lives in scripts/deploy.sh. Run from the root."
        }))
        .send()
        .await
        .unwrap();

    // Seed the page the plan will delete: it cites a source that is
    // GONE (the delete guard requires all cited sources absent).
    std::fs::create_dir_all(root.join("knowledge").join("wiki")).unwrap();
    std::fs::write(
        root.join("knowledge").join("wiki").join("deploy-pipeline.md"),
        "---\ntitle: 部署流水线\nsources: [old-doc]\nsource_hashes:\n  old-doc: deadbeef\n---\n# 部署流水线\n\n旧内容\n",
    )
    .unwrap();

    let started: serde_json::Value = http
        .post(format!("{url}/api/v1/knowledge/wiki/build"))
        .json(&serde_json::json!({ "scope": "all" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let id = started["build_id"].as_i64().unwrap();
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

    // A scope selecting nothing is a 400, and so is an empty KB.
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
    // Live incident regression (2026-09-14): a scoped build's planner
    // mistook out-of-scope sources for deleted ones. The executor must
    // refuse to delete a page that still cites an on-disk source.
    let _serialized = ONE_BUILD_AT_A_TIME
        .get_or_init(|| tokio::sync::Mutex::new(()))
        .lock()
        .await;
    let replies = serde_json::json!([
        {"marker": "WIKI PLANNER", "reply": plan_json(serde_json::json!([
            page("deploy-pipeline", "部署流水线", &["deploy-guide"], "delete"),
        ]))},
    ]);
    let (url, root) = start_wiki_daemon(&replies).await;
    let http = reqwest::Client::new();
    http.put(format!("{url}/api/v1/knowledge/raw/deploy-guide"))
        .json(&serde_json::json!({
            "content": "# Deploy\n\nThe deploy script lives in scripts/deploy.sh. Run from the root."
        }))
        .send()
        .await
        .unwrap();

    // Seed the page citing the LIVE deploy-guide.
    std::fs::create_dir_all(root.join("knowledge").join("wiki")).unwrap();
    std::fs::write(
        root.join("knowledge")
            .join("wiki")
            .join("deploy-pipeline.md"),
        "---\ntitle: 部署流水线\nsources: [deploy-guide]\n---\n# 部署流水线\n\n不该被删\n",
    )
    .unwrap();

    let started: serde_json::Value = http
        .post(format!("{url}/api/v1/knowledge/wiki/build"))
        .json(&serde_json::json!({ "scope": "all" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let id = started["build_id"].as_i64().unwrap();
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
