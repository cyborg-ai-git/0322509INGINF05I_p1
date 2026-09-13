//! I file di audit sono evidenze indipendenti, non lo stato condiviso del conto.
use app_atm::BLog;
use serde_json::json;
use std::{
    fs,
    time::{SystemTime, UNIX_EPOCH},
};
fn path(label: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "atm-audit-{label}-{}-{}.jsonl",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}
/// Il limite della console non elimina eventi dal file di audit.
#[test]
fn audit_keeps_events_hidden_from_terminal_and_flushes_each_line() {
    let path = path("complete");
    let mut log = BLog::new("ATM1", true)
        .with_audit_file(Some(&path))
        .unwrap()
        .with_console_hop_limit(Some(0));
    log.event("token_received", Some(4), Some(400), json!({}))
        .unwrap();
    log.event("token_forwarding", Some(5), Some(400), json!({}))
        .unwrap();
    let contents = fs::read_to_string(&path).unwrap();
    let records: Vec<serde_json::Value> = contents
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    assert_eq!(records.len(), 2);
    assert_eq!(records[0]["seq"], 1);
    assert_eq!(records[1]["seq"], 2);
    assert_eq!(records[1]["balance"], 400);
    assert_eq!(records[0]["pid"], std::process::id());
    drop(log);
    fs::remove_file(path).unwrap();
}
/// La creazione esclusiva del file preserva evidenze già presenti.
#[test]
fn existing_evidence_is_not_overwritten() {
    let path = path("existing");
    fs::write(&path, "preserved evidence").unwrap();
    assert!(
        BLog::new("ATM1", true)
            .with_audit_file(Some(&path))
            .is_err()
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), "preserved evidence");
    fs::remove_file(path).unwrap();
}
