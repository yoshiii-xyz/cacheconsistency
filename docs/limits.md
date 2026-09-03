# Limits

- JSON records: 8 MiB.
- Tree entries: 4096.
- Regular file contents: 16 MiB per file.
- Captured stdout and stderr: 1 MiB per stream.
- Paths: 4096 bytes.
- The workload record does not infer a complete dependency graph. Use
  `verify-tree` for an explicit tree snapshot.
- The simulator does not model network latency, server eviction, locking, or
  remote execution semantics.
- This is not a build system, cache server, remote execution service, or rclone.
- Human usability and physical-device validation are excluded.
