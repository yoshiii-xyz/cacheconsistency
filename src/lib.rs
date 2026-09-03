//! Compare bounded filesystem observations without claiming remote semantics.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;

pub const MAX_JSON_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_ENTRIES: usize = 4096;
pub const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;
pub const MAX_OUTPUT_BYTES: usize = 1024 * 1024;
pub const MAX_PATH_BYTES: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditError(pub String);

impl fmt::Display for AuditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for AuditError {}

impl From<std::io::Error> for AuditError {
    fn from(value: std::io::Error) -> Self {
        Self(value.to_string())
    }
}

impl From<serde_json::Error> for AuditError {
    fn from(value: serde_json::Error) -> Self {
        Self(format!("invalid consistency JSON: {value}"))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EntryKind {
    Directory,
    File,
    Symlink,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TreeEntry {
    pub path: String,
    pub kind: EntryKind,
    pub size_bytes: u64,
    pub digest: Option<String>,
    pub mode: u32,
    pub modified_unix_secs: Option<i64>,
    pub symlink_target: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Snapshot {
    pub schema_version: u32,
    pub backend: String,
    pub entries: Vec<TreeEntry>,
    pub omitted_claims: Vec<String>,
}

impl Snapshot {
    pub fn empty(backend: impl Into<String>) -> Self {
        Self {
            schema_version: 1,
            backend: backend.into(),
            entries: Vec::new(),
            omitted_claims: vec![
                "network filesystem protocol behavior".to_owned(),
                "cache server timing and eviction internals".to_owned(),
            ],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum CacheDecision {
    Hit,
    Miss,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum MaterializationState {
    #[default]
    Complete,
    Partial,
    Interrupted,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct CacheObservation {
    #[serde(default)]
    pub decision: CacheDecision,
    #[serde(default)]
    pub materialization: MaterializationState,
    #[serde(default)]
    pub declared_inputs: Vec<String>,
    #[serde(default)]
    pub observed_inputs: Vec<String>,
    #[serde(default)]
    pub missing_outputs: Vec<String>,
    #[serde(default)]
    pub duplicated_outputs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OutputSummary {
    pub bytes: usize,
    pub digest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunRecord {
    pub schema_version: u32,
    pub command: Vec<String>,
    pub cwd: String,
    pub status: String,
    pub exit_code: Option<i32>,
    pub stdout: OutputSummary,
    pub stderr: OutputSummary,
    pub tree: Snapshot,
    pub cache: CacheObservation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Difference {
    pub path: String,
    pub field: String,
    pub left: String,
    pub right: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ComparisonReport {
    pub schema_version: u32,
    pub equivalent: bool,
    pub differences: Vec<Difference>,
    pub omitted_claims: Vec<String>,
}

fn digest_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

fn mode(metadata: &fs::Metadata) -> u32 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        metadata.mode()
    }
    #[cfg(not(unix))]
    {
        let _ = metadata;
        0
    }
}

fn modified_unix_secs(metadata: &fs::Metadata) -> Option<i64> {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs() as i64)
}

fn relative_string(path: &Path) -> Result<String, AuditError> {
    let value = path
        .to_str()
        .ok_or_else(|| AuditError("tree contains a non-UTF-8 path".to_owned()))?
        .replace('\\', "/");
    if value.len() > MAX_PATH_BYTES {
        return Err(AuditError("tree path exceeds the path limit".to_owned()));
    }
    Ok(value)
}

fn hash_file(path: &Path, size: u64) -> Result<String, AuditError> {
    if size > MAX_FILE_BYTES {
        return Err(AuditError(format!(
            "file {} exceeds the {} byte limit",
            path.display(),
            MAX_FILE_BYTES
        )));
    }
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 8192];
    let mut total = 0_u64;
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        total += count as u64;
        if total > MAX_FILE_BYTES {
            return Err(AuditError(
                "file exceeded the content limit while reading".to_owned(),
            ));
        }
        hasher.update(&buffer[..count]);
    }
    Ok(hex::encode(hasher.finalize()))
}

pub fn snapshot_path(root: &Path, backend: impl Into<String>) -> Result<Snapshot, AuditError> {
    let root_metadata = fs::symlink_metadata(root)?;
    let mut snapshot = Snapshot::empty(backend);
    let mut pending = vec![(root.to_path_buf(), PathBuf::from("."), root_metadata)];
    while let Some((path, relative, metadata)) = pending.pop() {
        if snapshot.entries.len() >= MAX_ENTRIES {
            return Err(AuditError(format!(
                "tree exceeds the {MAX_ENTRIES} entry limit"
            )));
        }
        let relative_path = relative_string(&relative)?;
        let (kind, size_bytes, digest, symlink_target) = if metadata.file_type().is_symlink() {
            let target = fs::read_link(&path)?;
            let target_string = target
                .to_str()
                .ok_or_else(|| AuditError("tree contains a non-UTF-8 symlink target".to_owned()))?
                .replace('\\', "/");
            (EntryKind::Symlink, 0, None, Some(target_string))
        } else if metadata.is_dir() {
            let mut children = Vec::new();
            for child in fs::read_dir(&path)? {
                let child = child?;
                children.push(child.path());
            }
            children.sort_by(|left, right| {
                left.file_name()
                    .unwrap_or_default()
                    .cmp(right.file_name().unwrap_or_default())
            });
            for child in children.into_iter().rev() {
                let child_name = child
                    .file_name()
                    .ok_or_else(|| AuditError("tree entry has no filename".to_owned()))?;
                let child_relative = relative.join(child_name);
                let child_metadata = fs::symlink_metadata(&child)?;
                pending.push((child, child_relative, child_metadata));
            }
            (EntryKind::Directory, 0, None, None)
        } else if metadata.is_file() {
            (
                EntryKind::File,
                metadata.len(),
                Some(hash_file(&path, metadata.len())?),
                None,
            )
        } else {
            return Err(AuditError(format!(
                "unsupported filesystem entry: {}",
                path.display()
            )));
        };
        snapshot.entries.push(TreeEntry {
            path: relative_path,
            kind,
            size_bytes,
            digest,
            mode: mode(&metadata),
            modified_unix_secs: modified_unix_secs(&metadata),
            symlink_target,
        });
    }
    snapshot
        .entries
        .sort_by(|left, right| left.path.cmp(&right.path));
    validate_snapshot(&snapshot)?;
    Ok(snapshot)
}

pub fn validate_snapshot(snapshot: &Snapshot) -> Result<(), AuditError> {
    if snapshot.schema_version != 1 {
        return Err(AuditError("unsupported snapshot schema version".to_owned()));
    }
    if snapshot.entries.len() > MAX_ENTRIES {
        return Err(AuditError(format!(
            "snapshot exceeds the {MAX_ENTRIES} entry limit"
        )));
    }
    let mut paths = BTreeSet::new();
    for entry in &snapshot.entries {
        if entry.path.is_empty() || entry.path.len() > MAX_PATH_BYTES {
            return Err(AuditError(
                "snapshot entry path is outside its bounds".to_owned(),
            ));
        }
        if !paths.insert(entry.path.clone()) {
            return Err(AuditError(format!(
                "duplicate snapshot path {}",
                entry.path
            )));
        }
        if entry.kind == EntryKind::File && entry.digest.is_none() {
            return Err(AuditError(format!("file {} has no digest", entry.path)));
        }
        if entry.kind != EntryKind::Symlink && entry.symlink_target.is_some() {
            return Err(AuditError(format!(
                "non-symlink {} has a symlink target",
                entry.path
            )));
        }
    }
    Ok(())
}

fn add_difference(
    differences: &mut Vec<Difference>,
    path: impl Into<String>,
    field: impl Into<String>,
    left: impl Into<String>,
    right: impl Into<String>,
) {
    differences.push(Difference {
        path: path.into(),
        field: field.into(),
        left: left.into(),
        right: right.into(),
    });
}

fn compare_entry(left: &TreeEntry, right: &TreeEntry, differences: &mut Vec<Difference>) {
    if left.kind != right.kind {
        add_difference(
            differences,
            &left.path,
            "kind",
            format!("{:?}", left.kind),
            format!("{:?}", right.kind),
        );
    }
    if left.size_bytes != right.size_bytes {
        add_difference(
            differences,
            &left.path,
            "size_bytes",
            left.size_bytes.to_string(),
            right.size_bytes.to_string(),
        );
    }
    if left.digest != right.digest {
        add_difference(
            differences,
            &left.path,
            "digest",
            format!("{:?}", left.digest),
            format!("{:?}", right.digest),
        );
    }
    if left.mode != right.mode {
        add_difference(
            differences,
            &left.path,
            "mode",
            format!("{:o}", left.mode),
            format!("{:o}", right.mode),
        );
    }
    if left.modified_unix_secs != right.modified_unix_secs {
        add_difference(
            differences,
            &left.path,
            "modified_unix_secs",
            format!("{:?}", left.modified_unix_secs),
            format!("{:?}", right.modified_unix_secs),
        );
    }
    if left.symlink_target != right.symlink_target {
        add_difference(
            differences,
            &left.path,
            "symlink_target",
            format!("{:?}", left.symlink_target),
            format!("{:?}", right.symlink_target),
        );
    }
}

pub fn compare_snapshots(
    left: &Snapshot,
    right: &Snapshot,
) -> Result<ComparisonReport, AuditError> {
    validate_snapshot(left)?;
    validate_snapshot(right)?;
    let left_entries: BTreeMap<_, _> = left
        .entries
        .iter()
        .map(|entry| (&entry.path, entry))
        .collect();
    let right_entries: BTreeMap<_, _> = right
        .entries
        .iter()
        .map(|entry| (&entry.path, entry))
        .collect();
    let paths: BTreeSet<String> = left_entries
        .keys()
        .chain(right_entries.keys())
        .map(|path| (*path).clone())
        .collect();
    let mut differences = Vec::new();
    for path in paths {
        match (left_entries.get(&path), right_entries.get(&path)) {
            (Some(left_entry), Some(right_entry)) => {
                compare_entry(left_entry, right_entry, &mut differences)
            }
            (Some(_), None) => {
                add_difference(&mut differences, path, "presence", "present", "missing")
            }
            (None, Some(_)) => {
                add_difference(&mut differences, path, "presence", "missing", "present")
            }
            (None, None) => {}
        }
    }
    let mut omitted_claims = left.omitted_claims.clone();
    omitted_claims.extend(right.omitted_claims.clone());
    omitted_claims.sort();
    omitted_claims.dedup();
    Ok(ComparisonReport {
        schema_version: 1,
        equivalent: differences.is_empty(),
        differences,
        omitted_claims,
    })
}

fn compare_scalar<T: PartialEq + fmt::Debug>(
    differences: &mut Vec<Difference>,
    field: &str,
    left: &T,
    right: &T,
) {
    if left != right {
        add_difference(
            differences,
            "$run",
            field,
            format!("{:?}", left),
            format!("{:?}", right),
        );
    }
}

pub fn compare_runs(left: &RunRecord, right: &RunRecord) -> Result<ComparisonReport, AuditError> {
    let mut report = compare_snapshots(&left.tree, &right.tree)?;
    compare_scalar(
        &mut report.differences,
        "command",
        &left.command,
        &right.command,
    );
    compare_scalar(&mut report.differences, "cwd", &left.cwd, &right.cwd);
    compare_scalar(
        &mut report.differences,
        "status",
        &left.status,
        &right.status,
    );
    compare_scalar(
        &mut report.differences,
        "exit_code",
        &left.exit_code,
        &right.exit_code,
    );
    compare_scalar(
        &mut report.differences,
        "stdout",
        &left.stdout,
        &right.stdout,
    );
    compare_scalar(
        &mut report.differences,
        "stderr",
        &left.stderr,
        &right.stderr,
    );
    compare_scalar(
        &mut report.differences,
        "cache.decision",
        &left.cache.decision,
        &right.cache.decision,
    );
    compare_scalar(
        &mut report.differences,
        "cache.materialization",
        &left.cache.materialization,
        &right.cache.materialization,
    );
    compare_scalar(
        &mut report.differences,
        "cache.declared_inputs",
        &left.cache.declared_inputs,
        &right.cache.declared_inputs,
    );
    compare_scalar(
        &mut report.differences,
        "cache.observed_inputs",
        &left.cache.observed_inputs,
        &right.cache.observed_inputs,
    );
    compare_scalar(
        &mut report.differences,
        "cache.missing_outputs",
        &left.cache.missing_outputs,
        &right.cache.missing_outputs,
    );
    compare_scalar(
        &mut report.differences,
        "cache.duplicated_outputs",
        &left.cache.duplicated_outputs,
        &right.cache.duplicated_outputs,
    );
    report.differences.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then(left.field.cmp(&right.field))
    });
    report.equivalent = report.differences.is_empty();
    Ok(report)
}

pub trait Backend: Send + Sync {
    fn snapshot(&self) -> Result<Snapshot, AuditError>;
}

#[derive(Clone)]
pub struct SimulatedBackend {
    snapshot: Snapshot,
}

impl SimulatedBackend {
    pub fn new(snapshot: Snapshot) -> Result<Self, AuditError> {
        validate_snapshot(&snapshot)?;
        Ok(Self { snapshot })
    }
}

impl Backend for SimulatedBackend {
    fn snapshot(&self) -> Result<Snapshot, AuditError> {
        Ok(self.snapshot.clone())
    }
}

pub struct LocalBackend {
    root: PathBuf,
}

impl LocalBackend {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
}

impl Backend for LocalBackend {
    fn snapshot(&self) -> Result<Snapshot, AuditError> {
        snapshot_path(&self.root, "local")
    }
}

pub fn compare_backends(
    left: &dyn Backend,
    right: &dyn Backend,
) -> Result<ComparisonReport, AuditError> {
    compare_snapshots(&left.snapshot()?, &right.snapshot()?)
}

pub fn record_command(argv: &[String], cwd: &Path) -> Result<RunRecord, AuditError> {
    if argv.is_empty() || argv[0].is_empty() {
        return Err(AuditError("record requires a command after --".to_owned()));
    }
    let output = ProcessCommand::new(&argv[0])
        .args(&argv[1..])
        .current_dir(cwd)
        .output()?;
    if output.stdout.len() > MAX_OUTPUT_BYTES || output.stderr.len() > MAX_OUTPUT_BYTES {
        return Err(AuditError(format!(
            "command output exceeds the {MAX_OUTPUT_BYTES} byte limit"
        )));
    }
    let cwd_string = cwd
        .to_str()
        .ok_or_else(|| AuditError("current directory is not UTF-8".to_owned()))?
        .replace('\\', "/");
    let status = if output.status.success() {
        "success"
    } else if output.status.code().is_some() {
        "failed"
    } else {
        "terminated"
    };
    Ok(RunRecord {
        schema_version: 1,
        command: argv.to_vec(),
        cwd: cwd_string,
        status: status.to_owned(),
        exit_code: output.status.code(),
        stdout: OutputSummary {
            bytes: output.stdout.len(),
            digest: digest_bytes(&output.stdout),
        },
        stderr: OutputSummary {
            bytes: output.stderr.len(),
            digest: digest_bytes(&output.stderr),
        },
        tree: Snapshot::empty("local-record")
            .with_observation("workload tree snapshot is opt-in via verify-tree"),
        cache: CacheObservation {
            decision: CacheDecision::Unknown,
            materialization: MaterializationState::Complete,
            declared_inputs: vec![argv[0].clone()],
            observed_inputs: Vec::new(),
            missing_outputs: Vec::new(),
            duplicated_outputs: Vec::new(),
        },
    })
}

impl Snapshot {
    fn with_observation(mut self, observation: &str) -> Self {
        self.omitted_claims.push(observation.to_owned());
        self.omitted_claims.sort();
        self.omitted_claims.dedup();
        self
    }
}

pub fn read_run(path: &Path) -> Result<RunRecord, AuditError> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.len() as usize > MAX_JSON_BYTES {
        return Err(AuditError(
            "run record is not a bounded regular file".to_owned(),
        ));
    }
    let record: RunRecord = serde_json::from_slice(&fs::read(path)?)?;
    if record.schema_version != 1 {
        return Err(AuditError("unsupported run schema version".to_owned()));
    }
    validate_snapshot(&record.tree)?;
    Ok(record)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str, digest: &str, mode: u32) -> TreeEntry {
        TreeEntry {
            path: path.to_owned(),
            kind: EntryKind::File,
            size_bytes: 4,
            digest: Some(digest.to_owned()),
            mode,
            modified_unix_secs: Some(1),
            symlink_target: None,
        }
    }

    fn symlink(path: &str, target: &str) -> TreeEntry {
        TreeEntry {
            path: path.to_owned(),
            kind: EntryKind::Symlink,
            size_bytes: 0,
            digest: None,
            mode: 0,
            modified_unix_secs: Some(1),
            symlink_target: Some(target.to_owned()),
        }
    }

    fn snapshot(entries: Vec<TreeEntry>) -> Snapshot {
        Snapshot {
            schema_version: 1,
            backend: "simulator".to_owned(),
            entries,
            omitted_claims: vec!["remote cache internals".to_owned()],
        }
    }

    fn run(decision: CacheDecision, materialization: MaterializationState) -> RunRecord {
        RunRecord {
            schema_version: 1,
            command: vec!["fixture".to_owned()],
            cwd: "/tmp/work".to_owned(),
            status: "success".to_owned(),
            exit_code: Some(0),
            stdout: OutputSummary {
                bytes: 4,
                digest: "aaaa".to_owned(),
            },
            stderr: OutputSummary {
                bytes: 0,
                digest: digest_bytes(&[]),
            },
            tree: Snapshot::empty("simulator"),
            cache: CacheObservation {
                decision,
                materialization,
                declared_inputs: vec!["src".to_owned()],
                observed_inputs: vec!["src".to_owned()],
                missing_outputs: Vec::new(),
                duplicated_outputs: Vec::new(),
            },
        }
    }

    #[test]
    fn identical_simulated_backends_are_equivalent() {
        let left = SimulatedBackend::new(snapshot(vec![file("a", "x", 0o644)])).unwrap();
        let right = SimulatedBackend::new(snapshot(vec![file("a", "x", 0o644)])).unwrap();
        assert!(compare_backends(&left, &right).unwrap().equivalent);
    }

    #[test]
    fn stale_cache_decision_is_a_difference() {
        let left = run(CacheDecision::Hit, MaterializationState::Complete);
        let right = run(CacheDecision::Miss, MaterializationState::Complete);
        assert!(!compare_runs(&left, &right).unwrap().equivalent);
    }

    #[test]
    fn truncated_output_is_a_difference() {
        let mut left = run(CacheDecision::Unknown, MaterializationState::Complete);
        let right = run(CacheDecision::Unknown, MaterializationState::Complete);
        left.stdout.bytes = 3;
        assert_eq!(
            compare_runs(&left, &right).unwrap().differences[0].field,
            "stdout"
        );
    }

    #[test]
    fn symlink_target_mismatch_is_visible() {
        let left = SimulatedBackend::new(snapshot(vec![symlink("link", "a")])).unwrap();
        let right = SimulatedBackend::new(snapshot(vec![symlink("link", "b")])).unwrap();
        let report = compare_backends(&left, &right).unwrap();
        assert!(
            report
                .differences
                .iter()
                .any(|difference| difference.field == "symlink_target")
        );
    }

    #[test]
    fn permission_mismatch_is_visible() {
        let left = SimulatedBackend::new(snapshot(vec![file("a", "x", 0o644)])).unwrap();
        let right = SimulatedBackend::new(snapshot(vec![file("a", "x", 0o600)])).unwrap();
        assert!(
            compare_backends(&left, &right)
                .unwrap()
                .differences
                .iter()
                .any(|difference| difference.field == "mode")
        );
    }

    #[test]
    fn interrupted_materialization_is_visible() {
        let left = run(CacheDecision::Miss, MaterializationState::Interrupted);
        let right = run(CacheDecision::Miss, MaterializationState::Complete);
        assert!(!compare_runs(&left, &right).unwrap().equivalent);
    }

    #[test]
    fn duplicate_and_resource_limited_snapshots_are_rejected() {
        let duplicate = snapshot(vec![file("a", "x", 0o644), file("a", "x", 0o644)]);
        assert!(SimulatedBackend::new(duplicate).is_err());
        let too_many = Snapshot {
            schema_version: 1,
            backend: "simulator".to_owned(),
            entries: (0..=MAX_ENTRIES)
                .map(|index| file(&format!("{index}"), "x", 0o644))
                .collect(),
            omitted_claims: Vec::new(),
        };
        assert!(validate_snapshot(&too_many).is_err());
    }

    #[test]
    fn comparison_json_is_deterministic() {
        let left = SimulatedBackend::new(snapshot(vec![file("a", "x", 0o644)])).unwrap();
        let right = SimulatedBackend::new(snapshot(vec![file("a", "y", 0o644)])).unwrap();
        let first = serde_json::to_string(&compare_backends(&left, &right).unwrap()).unwrap();
        let second = serde_json::to_string(&compare_backends(&left, &right).unwrap()).unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn concurrent_simulator_readers_are_safe() {
        let backend = std::sync::Arc::new(
            SimulatedBackend::new(snapshot(vec![file("a", "x", 0o644)])).unwrap(),
        );
        let workers: Vec<_> = (0..8)
            .map(|_| {
                let backend = backend.clone();
                std::thread::spawn(move || backend.snapshot().unwrap().entries.len())
            })
            .collect();
        for worker in workers {
            assert_eq!(worker.join().unwrap(), 1);
        }
    }
}
