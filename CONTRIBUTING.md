# Contributing

Keep the backend interface small and local. Add fixtures and tests for every
new consistency state. A simulator must identify itself and must not imply a
network filesystem result.

Before review, run:

```text
cargo fmt --all -- --check
cargo check --all-targets --locked
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --all-targets --locked
RUSTDOCFLAGS=-Dwarnings cargo doc --no-deps --locked
cargo package --locked
cargo audit
cargo audit --file fuzz/Cargo.lock
```

The repository uses trunk-based development. A human reviews and commits
changes.
