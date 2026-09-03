# AGENTS.md

## Project context

- Build: `cargo build --locked`
- Test: `cargo test --all-targets --locked`
- Lint: `cargo fmt --all -- --check` and `cargo clippy --all-targets --all-features --locked -- -D warnings`
- Documentation: `RUSTDOCFLAGS=-Dwarnings cargo doc --no-deps --locked`
- Security: `cargo audit` and `cargo audit --file fuzz/Cargo.lock`
- Scope: bounded local tree observations and deterministic simulated backends.

## Working rules

Keep snapshots sorted, bounded, and explicit about omitted claims. Do not add
a network filesystem or remote execution adapter to the MVP. Never call a
simulator result a real remote result. Run the full validation commands before
describing a change as complete.
