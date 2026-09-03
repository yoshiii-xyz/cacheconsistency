# Release gates

Run from a clean checkout:

```text
cargo fmt --all -- --check
cargo check --all-targets --locked
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --all-targets --locked
RUSTDOCFLAGS=-Dwarnings cargo doc --no-deps --locked
cargo package --locked
cargo publish --dry-run --locked
cargo audit
cargo audit --file fuzz/Cargo.lock
timeout --foreground 300s env RUSTUP_TOOLCHAIN=nightly cargo fuzz build run-json
timeout --foreground 60s env RUSTUP_TOOLCHAIN=nightly cargo fuzz run run-json -- -max_total_time=10 -verbosity=0 -print_final_stats=1
```

CI, Security, CodeQL, and the tag package workflow must pass before a crate
or GitHub release is published. Keep generated evidence in the ignored
`qa/evidence/` directory.
