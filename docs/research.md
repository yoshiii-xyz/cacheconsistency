# Research record

## Decision

The design uses Rust filesystem metadata and content hashing with a narrow
backend trait. Cargo and filesystem documentation establish the local
metadata boundary. The simulator is intentionally a separate implementation
that produces the same snapshot schema, so the comparison logic can be
tested without claiming a network filesystem result.

## Sources

- Rust filesystem metadata: https://doc.rust-lang.org/std/fs/fn.symlink_metadata.html
- Rust directory iteration: https://doc.rust-lang.org/std/fs/fn.read_dir.html
- Rust process output: https://doc.rust-lang.org/std/process/struct.Output.html
- SHA-256 digest API: https://docs.rs/sha2/latest/sha2/
