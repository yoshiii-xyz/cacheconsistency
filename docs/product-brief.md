# Product brief

## Mission

Prove whether a workload observes equivalent inputs and outputs across local,
cached, and remote-like filesystem configurations.

## MVP

```text
cacheconsistency record -- ./fixture-workload
cacheconsistency compare run-a.json run-b.json
cacheconsistency verify-tree PATH
```

The MVP provides local tree snapshots and a deterministic simulator through a
backend trait. It compares content, metadata, symlink targets, timestamps,
outputs, cache decisions, declared and observed inputs, and materialization
status.

## Completion boundary

Identical backends, stale cache decisions, truncated output, symlink and mode
mismatches, concurrent simulator readers, interrupted materialization,
deterministic replay, and resource limits are tested. A real remote backend is
not included.
