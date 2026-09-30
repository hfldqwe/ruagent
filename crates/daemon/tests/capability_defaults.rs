//! The capability plane's DEFAULT-behaviour proof (design §18) and the
//! end-to-end wiring t8 owns: boot assembly, the two injection gates, the
//! unattended-extraction gate, and the fact that every id the design registers
//! is reachable from BOTH the config file and the HTTP API under the same name.
//!
//! WHY THIS FILE EXISTS SEPARATELY FROM `capabilities.rs`: that file probes the
//! HTTP surface (GET/PUT + the editor); this one is the LAW file — it asserts
//! the four laws over the whole registry and the property this increment cannot
//! ship without, namely that the shipped configuration does exactly what the
//! platform did before the plane existed.
//!
//! The 11-id list below is written out ON PURPOSE, in the design's spelling.
//! A registry that quietly grows, loses or renames a row would otherwise pass
//! every other test in this repository: everything else reads the registry.

use ruagent_daemon::capability::{CapabilityId, CapabilityPlane, spec, specs};
use ruagent_daemon::config::DaemonConfig;
use ruagent_daemon::extract_plane::ExtractPlan;
use ruagent_policy::PolicyConfig;

/// The design's registry (§5.4/§6), sorted, spelled exactly as the config file
/// and the HTTP API spell it.
const DESIGN_IDS: [&str; 11] = [
    "distill_session",
    "knowledge_ingest_graph",
    "memory_inject_chat",
    "memory_inject_runs",
    "recall_leg_graph",
    "recall_leg_knowledge_fts",
    "recall_leg_knowledge_semantic",
    "recall_leg_memory_fts",
    "recall_leg_memory_semantic",
    "recall_leg_wiki",
    "session_extract_rules",
];

fn fresh_root(tag: &str) -> std::path::PathBuf {
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!(
        "ruagent-capdefaults-{tag}-{}-{n}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn plane_of(toml_text: &str) -> CapabilityPlane {
    let policy = PolicyConfig::parse(toml_text).expect("policy text parses");
    CapabilityPlane::from_policy(&policy).expect("plane builds")
}

/// §18 "Registry conformance": one row per `CapabilityId::ALL` entry, ids
/// unique, non-empty and spelled as the design spells them, and the L4
/// implications over the whole registry.
#[test]
fn the_registry_is_conformant_and_carries_the_designs_eleven_ids() {
    assert_eq!(specs().len(), CapabilityId::ALL.len());
    assert_eq!(specs().len(), DESIGN_IDS.len());
    for (i, id) in CapabilityId::ALL.iter().enumerate() {
        assert_eq!(
            specs()[i].id,
            *id,
            "registry order must follow CapabilityId::ALL"
        );
        assert_eq!(spec(*id).id, *id);
        assert!(!id.as_str().is_empty());
        let s = spec(*id);
        assert!(!s.description.is_empty(), "{}", id.as_str());
        assert!(!s.gates.is_empty(), "{}", id.as_str());
        // L4: an llm-tier capability — and anything new in this increment —
        // defaults OFF.
        if s.tier.as_str() == "llm" {
            assert!(!s.default_enabled, "llm tier defaults off: {}", id.as_str());
        }
        if s.new_in_this_increment {
            assert!(
                !s.default_enabled,
                "a new capability defaults off: {}",
                id.as_str()
            );
        } else if s.tier.as_str() == "free" {
            assert!(
                s.default_enabled,
                "a free capability that exists today defaults ON: {}",
                id.as_str()
            );
        }
    }
    let mut names: Vec<&str> = specs().iter().map(|s| s.id.as_str()).collect();
    names.sort_unstable();
    assert_eq!(
        names,
        DESIGN_IDS.to_vec(),
        "the registry IS the design's list"
    );
}

/// §18 "Exhaustive pass-through" (L1/L2) on a FRESH ROOT, i.e. through the real
/// `DaemonConfig::load` path rather than a hand-built plane: with no
/// `[capabilities]` table every gate answers with today's behaviour and every
/// resolved option is the registry default.
#[test]
fn a_fresh_root_is_legacy_mode_and_every_gate_passes_through() {
    let dir = fresh_root("fresh");
    std::fs::create_dir_all(&dir).unwrap();
    let cfg = DaemonConfig::load(&dir).expect("a fresh root boots");
    assert!(
        !cfg.capabilities.table_present(),
        "the shipped policy.toml must leave the table ABSENT"
    );
    for id in CapabilityId::ALL {
        assert!(
            cfg.capabilities.gate(*id, true),
            "L1: legacy mode passes the gate through ({})",
            id.as_str()
        );
        assert!(
            !cfg.capabilities.gate(*id, false),
            "L2: and can never start work the legacy flag did not ask for ({})",
            id.as_str()
        );
        assert_eq!(
            cfg.capabilities.options(*id),
            spec(*id).defaults,
            "{}",
            id.as_str()
        );
        assert_eq!(cfg.capabilities.configured(*id), "legacy");
    }
    // The two new free capabilities are OFF, which is what "today" means: they
    // did not exist, and nothing new runs.
    assert!(!cfg.capabilities.enabled(CapabilityId::SessionExtractRules));
    assert!(!cfg.capabilities.enabled(CapabilityId::KnowledgeIngestGraph));
    // The llm tier is off as well (L4) — today `[distill].auto` is false.
    assert!(!cfg.capabilities.enabled(CapabilityId::DistillSession));
    std::fs::remove_dir_all(&dir).ok();
}

/// Every registered id is reachable from the CONFIG face under its own name,
/// and no other spelling is accepted (L3) — so the file cannot drift from the
/// registry the HTTP API, the MCP tools and the panel all read.
#[test]
fn every_registered_id_is_readable_from_the_config_file() {
    for id in CapabilityId::ALL {
        let name = id.as_str();
        let plane = plane_of(&format!("[capabilities.{name}]\nenabled = false\n"));
        assert!(plane.table_present());
        assert!(!plane.enabled(*id), "{name}");
        assert_eq!(plane.configured(*id), "file");
        // A near-miss is refused rather than silently ignored.
        let near = format!("[capabilities.{}_x]\nenabled = true\n", name);
        let policy = PolicyConfig::parse(&near).expect("the file itself parses");
        assert!(
            CapabilityPlane::from_policy(&policy).is_err(),
            "`{name}_x` must not be accepted as `{name}`"
        );
    }
    // And the whole set survives as ONE table — exactly what the panel and the
    // MCP `capability_set` tool write.
    let mut text = String::new();
    for id in CapabilityId::ALL {
        text.push_str(&format!("[capabilities.{}]\nenabled = true\n", id.as_str()));
    }
    let plane = plane_of(&text);
    let rows = plane.rows();
    assert_eq!(rows.len(), DESIGN_IDS.len());
    for row in &rows {
        assert_eq!(row.configured, "file", "{row:?}");
        assert!(row.enabled, "{row:?}");
    }
}

/// §18 "llm inertness", at the DECISION level: the shipped configuration enables
/// no unattended extraction at all, and the free tier is the only path that can
/// run without `[distill].auto`.
#[test]
fn the_shipped_configuration_enables_no_unattended_extraction() {
    let legacy = CapabilityPlane::legacy();
    assert!(
        ExtractPlan::unattended(&legacy, false).is_empty(),
        "with the table absent and auto = false, nothing runs"
    );
    let today = ExtractPlan::unattended(&legacy, true);
    assert_eq!(
        (today.rules, today.acp),
        (false, true),
        "with the table absent, [distill].auto is the only switch (L1)"
    );
    // The free tier is opt-in and needs no legacy flag: it is the ONE path the
    // user can enable that spends nothing.
    let free = plane_of("[capabilities.session_extract_rules]\nenabled = true\n");
    let p = ExtractPlan::unattended(&free, false);
    assert_eq!((p.rules, p.acp), (true, false), "{p:?}");
    // A table that exists without naming `distill_session` narrows the llm tier
    // off (L4) — the direction that can never spend tokens by accident.
    let unnamed = plane_of("[capabilities.recall_leg_wiki]\nenabled = false\n");
    assert!(
        ExtractPlan::unattended(&unnamed, true).is_empty(),
        "the llm tier defaults off once the table exists"
    );
    let on = plane_of("[capabilities.distill_session]\nenabled = true\n");
    let p = ExtractPlan::unattended(&on, true);
    assert_eq!((p.rules, p.acp), (false, true), "{p:?}");
}

/// §18 "No new always-on background work", as a property of the default
/// configuration: the three capabilities that could START something are off,
/// and every capability that names behaviour existing today still passes
/// through. The boot's spawn inventory itself is unchanged (lib.rs gained two
/// statements, no `tokio::spawn`); the KB→graph sweep rides the pre-existing
/// 60 s knowledge scan.
#[test]
fn the_default_configuration_starts_no_new_work() {
    let dir = fresh_root("nowork");
    std::fs::create_dir_all(&dir).unwrap();
    let cfg = DaemonConfig::load(&dir).expect("a fresh root boots");
    let plane = &cfg.capabilities;
    for id in [
        CapabilityId::KnowledgeIngestGraph,
        CapabilityId::SessionExtractRules,
        CapabilityId::DistillSession,
    ] {
        assert!(
            !plane.enabled(id),
            "{} must be off in the default configuration",
            id.as_str()
        );
    }
    // Today's behaviour is untouched: the two injection gates and all six recall
    // legs pass through (legacy), so recall and injection cannot move.
    for id in [
        CapabilityId::MemoryInjectChat,
        CapabilityId::MemoryInjectRuns,
        CapabilityId::RecallLegMemorySemantic,
        CapabilityId::RecallLegMemoryFts,
        CapabilityId::RecallLegKnowledgeSemantic,
        CapabilityId::RecallLegKnowledgeFts,
        CapabilityId::RecallLegWiki,
        CapabilityId::RecallLegGraph,
    ] {
        assert!(plane.gate(id, true), "{}", id.as_str());
    }
    std::fs::remove_dir_all(&dir).ok();
}
