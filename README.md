# cacheconsistency

cacheconsistency compares bounded filesystem observations and workload
transcripts across local or deterministic simulated backends. It reports
content, metadata, symlink, timestamp, output, cache-decision, and
materialization differences without claiming a real remote filesystem result.

Status: released v0.1.0 local and simulator MVP.

## Install

```text
cargo install cacheconsistency --version 0.1.0
```

## Quick start

```text
cacheconsistency record -- ./fixture-workload
cacheconsistency compare run-a.json run-b.json
cacheconsistency verify-tree PATH
```

`record` emits a JSON workload record. `compare` emits a deterministic report.
`verify-tree` snapshots a local tree without following symlinks.

## What it solves

The library provides a `Backend` trait, a local backend, and a deterministic
simulated backend. A comparison makes missing entries, stale cache decisions,
truncated output, symlink changes, permission changes, and interrupted
materialization visible as differences.

## Safety and privacy

The tool reads the selected tree and executes only the command explicitly
given to `record`. It does not connect to a remote backend. File contents are
hashed locally, output is capped at 1 MiB per stream, files at 16 MiB, and
trees at 4096 entries.

## Limits and non-goals

See [docs/limits.md](docs/limits.md). This is not Bazel, a remote execution
service, rclone, a cache server, or a claim about network filesystem behavior.

## Testing and development

See [CONTRIBUTING.md](CONTRIBUTING.md) and [docs/release.md](docs/release.md)
for the validation commands.

## Release and support status

Version 0.1.0 is released for local and simulator evidence. A real remote
adapter is not included.

## License

MIT. See [LICENSE](LICENSE).
