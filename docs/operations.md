# Operations

## Record a workload

Run `cacheconsistency record -- ./fixture-workload`. The command and its
arguments are stored in the JSON record. The command is the only process the
MVP executes.

## Compare records

Run `cacheconsistency compare run-a.json run-b.json`. An `equivalent` value of
false contains sorted field-level differences. `omitted_claims` identifies
what the report cannot prove.

## Verify a tree

Run `cacheconsistency verify-tree PATH`. Regular files are hashed, symlinks
are recorded without following them, and traversal stops at the documented
limits.
