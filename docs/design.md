# Design

`Snapshot` is a sorted list of path records. A record contains entry kind,
file size, SHA-256 for regular files, mode, observed modification seconds,
and a symlink target when applicable. `LocalBackend` uses `symlink_metadata`
and never follows a symlink while walking.

`RunRecord` adds command status, stdout and stderr digests, and cache
observations. `compare_runs` and `compare_snapshots` use ordered maps and
ordered differences, which makes equal input records produce equal JSON.
Every report carries omitted claims for remote protocol behavior and cache
server timing.
