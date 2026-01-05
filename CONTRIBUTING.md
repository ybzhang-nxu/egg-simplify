# Contributing

Thanks for helping improve mpl-simplifier. Please read AGENTS.md first; it
defines invariants and the dependency DAG that must not be broken.

## Non-negotiable constraints
- Keep canonical normalization deterministic and idempotent.
- Do not introduce dependency cycles (see AGENTS.md).
- Avoid branch-sensitive functional identities.
- Use `Result` errors instead of panics in library crates.
- New rewrite rules must include tests; risky rule groups should include a bench.
- Any canonical form change requires updates to `docs/canonical_form.md` and
  `tests/regression_normalize.rs`.

## Pre-flight checks
```bash
cargo fmt --check
cargo test --workspace
# If CI enforces clippy:
cargo clippy --all-targets --all-features -- -D warnings
```
