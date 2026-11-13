#![cfg(all(target_os = "linux", target_arch = "x86_64"))]

use benilla_extractor_observer::{observe, Level, Plan, RefusalKind};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);
fn root() -> std::path::PathBuf {
    let base = std::env::var_os("CARGO_TARGET_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target")
        });
    std::fs::create_dir_all(&base).unwrap();
    base.join(format!(
        "observer-refusal-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}
fn invalid_plan() -> Plan {
    Plan {
        attempt_id: "invalid".into(),
        stage: "authored-test".into(),
        executable: Vec::new(),
        executable_sha256: String::new(),
        argv: Vec::new(),
        env: Vec::new(),
        cwd: Vec::new(),
        level: Level::Full,
        gates: Vec::new(),
        max_events: 100,
        max_blob_bytes: 1024,
    }
}

#[test]
fn invalid_launch_is_typed_and_retains_a_chained_refusal() {
    let root = root();
    let result = observe(&invalid_plan(), &root).unwrap_err();
    assert_eq!(result.kind, RefusalKind::InvalidPlan);
    let marker: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("completion.json")).unwrap()).unwrap();
    assert_eq!(marker["clean_completion"], false);
    assert_eq!(marker["count"], 1);
    let event: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("products.jsonl")).unwrap()).unwrap();
    assert_eq!(event["kind"], "refusal");
    assert_eq!(event["previous_record_sha256"], serde_json::Value::Null);
    assert_eq!(event["payload"]["first_failure"]["kind"], "invalid_plan");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_consumed_attempt_is_never_reused() {
    let root = root();
    let _ = observe(&invalid_plan(), &root);
    let before = std::fs::read(root.join("products.jsonl")).unwrap();
    let result = observe(&invalid_plan(), &root).unwrap_err();
    assert_eq!(result.kind, RefusalKind::ReceiptIo);
    assert_eq!(std::fs::read(root.join("products.jsonl")).unwrap(), before);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn unknown_plan_fields_are_rejected_instead_of_ignored() {
    let mut value = serde_json::to_value(invalid_plan()).unwrap();
    value["preload"] = serde_json::Value::Bool(true);
    assert!(serde_json::from_value::<Plan>(value).is_err());
}
