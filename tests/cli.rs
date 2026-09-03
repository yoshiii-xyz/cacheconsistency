use cacheconsistency::{
    CacheDecision, CacheObservation, ComparisonReport, EntryKind, MaterializationState,
    OutputSummary, RunRecord, Snapshot, TreeEntry,
};
use std::process::Command;

fn binary() -> Command {
    Command::new(env!("CARGO_BIN_EXE_cacheconsistency"))
}

fn record() -> RunRecord {
    RunRecord {
        schema_version: 1,
        command: vec!["fixture".to_owned()],
        cwd: "/tmp/work".to_owned(),
        status: "success".to_owned(),
        exit_code: Some(0),
        stdout: OutputSummary {
            bytes: 0,
            digest: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_owned(),
        },
        stderr: OutputSummary {
            bytes: 0,
            digest: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_owned(),
        },
        tree: Snapshot::empty("fixture"),
        cache: CacheObservation {
            decision: CacheDecision::Unknown,
            materialization: MaterializationState::Complete,
            declared_inputs: vec!["fixture".to_owned()],
            observed_inputs: vec!["fixture".to_owned()],
            missing_outputs: Vec::new(),
            duplicated_outputs: Vec::new(),
        },
    }
}

#[test]
fn verify_tree_command_returns_entries() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/local-tree");
    let output = binary().args(["verify-tree"]).arg(path).output().unwrap();
    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["schema_version"], 1);
    assert!(
        json["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["path"] == "./data.txt")
    );
}

#[test]
fn compare_command_reports_equivalence_deterministically() {
    let base = std::env::temp_dir().join(format!("cacheconsistency-{}", std::process::id()));
    std::fs::create_dir_all(&base).unwrap();
    let left = base.join("left.json");
    let right = base.join("right.json");
    let bytes = serde_json::to_vec(&record()).unwrap();
    std::fs::write(&left, &bytes).unwrap();
    std::fs::write(&right, &bytes).unwrap();
    let output = binary()
        .args(["compare"])
        .arg(&left)
        .arg(&right)
        .output()
        .unwrap();
    assert!(output.status.success());
    let report: ComparisonReport = serde_json::from_slice(&output.stdout).unwrap();
    assert!(report.equivalent);
    std::fs::remove_file(left).unwrap();
    std::fs::remove_file(right).unwrap();
    std::fs::remove_dir(base).unwrap();
}

#[test]
fn record_command_captures_output_and_failure_state() {
    let success = binary()
        .args(["record", "--", "sh", "-c", "printf cache"])
        .output()
        .unwrap();
    assert!(success.status.success());
    let record: RunRecord = serde_json::from_slice(&success.stdout).unwrap();
    assert_eq!(record.status, "success");
    assert_eq!(record.stdout.bytes, 5);
    let failure = binary()
        .args(["record", "--", "sh", "-c", "exit 7"])
        .output()
        .unwrap();
    assert!(failure.status.success());
    let failed: RunRecord = serde_json::from_slice(&failure.stdout).unwrap();
    assert_eq!(failed.status, "failed");
    assert_eq!(failed.exit_code, Some(7));
}

#[test]
fn snapshot_schema_can_represent_symlinks() {
    let snapshot = Snapshot {
        schema_version: 1,
        backend: "fixture".to_owned(),
        entries: vec![TreeEntry {
            path: "link".to_owned(),
            kind: EntryKind::Symlink,
            size_bytes: 0,
            digest: None,
            mode: 0,
            modified_unix_secs: None,
            symlink_target: Some("data.txt".to_owned()),
        }],
        omitted_claims: Vec::new(),
    };
    assert_eq!(
        snapshot.entries[0].symlink_target.as_deref(),
        Some("data.txt")
    );
}
