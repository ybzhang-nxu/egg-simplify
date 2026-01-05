# mpl-simplifier (v0.1.2)

## Project Overview
mpl-simplifier is a deterministic Rust workspace for simplifying symbolic algebraic
expressions. It provides a canonical normal form (v0.1.1 baseline), a minimal
symbol layer for log/li2, and an egg-based rewrite engine with a conservative
symbol guard to prevent rewrites from violating defined symbol constraints.

This project intentionally avoids branch-sensitive functional identities and
full MPL/GPL reconstruction. It does not implement log/li2 identities, higher
weight symbol integrability, or function reconstruction; those are deferred to
future milestones.

## Workspace Layout
Dependency DAG (no cycles, per AGENTS.md):
`mpl-ir` <- {`mpl-symbol`, `mpl-rewrite`, `mpl-verify`} <- `mpl-simplify`.

| Crate | Purpose | Key Modules / Public API | Depends On |
| --- | --- | --- | --- |
| `mpl-ir` | AST, parser, normalization, canonical printing | `Expr`, `parse_sexpr`, `Expr::normalize`, `Expr::to_canonical_string`, `ParseError` (`crates/ir/src/lib.rs`) | (none) |
| `mpl-symbol` | Symbol tensor, symbolization rules, integrability checks | `Symbol`, `Word`, `Coeff`, `symbol`, `check_integrable`, `SymbolError` (`crates/symbol/src/*.rs`) | `mpl-ir` |
| `mpl-rewrite` | egg language/rules/lowering/lifting + simplifier | `simplify_algebra`, `RewriteConfig`, `RewriteMode`, `RewriteError`, `lower_expr`, `lift_expr` (`crates/rewrite/src/*.rs`) | `mpl-ir`, `egg` |
| `mpl-verify` | Exact rational eval + sample equivalence | `eval_rational`, `equiv_on_samples`, `EvalError` (`crates/verify/src/lib.rs`) | `mpl-ir` |
| `mpl-simplify` | CLI entry point | Subcommands in `crates/cli/src/main.rs` | `mpl-ir`, `mpl-rewrite`, `mpl-symbol` |

## Expression Language (current)
- S-expression syntax.
- Operators:
  - N-ary add: `(+ a b c ...)`
  - N-ary mul: `(* a b c ...)`
  - N-ary div: `(/ a b c ...)` (desugars to multiplication by inverse)
  - Integer power: `(^ base exp)` where `exp` is an integer atom
  - Unary negation: `(- x)` (only unary; there is no binary subtraction)
  - Unary log: `(log x)` (opaque wrapper; no identities)
  - Unary li2: `(li2 x)` (opaque wrapper; no identities)
- Numbers: integers or rationals like `1/2` or `-7/3`.
- Windows / clap note: if an expression starts with `-`, pass it as
  `--expr -- -7/3` or quote it: `--expr "(- 7/3)"`.

## Canonical Normalization (v0.1.1 baseline)
Canonical rules are stable and deterministic. Key red lines:
- Flatten nested additions and multiplications.
- Remove additive identity: `(+ x 0) -> x`, empty sum -> `0`.
- Remove multiplicative identity: `(* x 1) -> x`, empty product -> `1`.
- Strict annihilator: any `Mul` containing `0` normalizes to `0`.
- Fold rational constants in `Add` and `Mul` into a single exact rational.
- Eliminate division: `(/ a b c)` -> `(* a (^ b -1) (^ c -1))`.
- Power rules: `x^0 -> 1` (including `0^0 -> 1`), `x^1 -> x`.
- Merge same-base powers in products: `x^a * x^b -> x^(a+b)`.
- Safe power folding for rationals; `(^ 0 -1)` stays as-is (no panic).
- Deterministic ordering: constants first, then canonical-string ordering.
- `log`/`li2` only normalize their argument; no functional identities.

See `docs/canonical_form.md` for the formal spec.

## Symbol Layer (current milestone)
Supported symbol rules (no branch-sensitive identities):
- `S(log l) = [l]` for algebraic letter `l`.
- `S(li2 f) = -(1-f) ⊗ f` (letter `f` must be algebraic).
- `S(log a * log b) = a ⊗ b + b ⊗ a` (algebraic letters only).

Integrability:
- `check_integrable` currently supports weight-2 symbols; weight > 2 returns
  `NotImplemented`.
- Deterministic sampling uses a fixed rational table and ordered variable
  environments; singular points are skipped.

Output format:
- CLI prints one line per word: `coeff * (l1 ⊗ l2 ⊗ ...)`.
- Coefficients are exact rationals; terms are merged and sorted deterministically
  (`Symbol` uses `BTreeMap`, `Word` orders by canonical strings).

## Rewrite Layer (egg) (current milestone)
Language nodes (opaque wrappers for functions):
- `Num(Q)`, `Var`, `Add`, `Mul`, `Pow`, `Log`, `Li2` (`crates/rewrite/src/lang.rs`).
- `Neg` is lowered as `(* -1 x)`; no separate `Neg` node.

Lowering/Lifting:
- N-ary `Add`/`Mul` lower with a fixed right-associative fold.
- `Pow` exponents are stored as numeric nodes; lifting rejects non-integers or
  out-of-range exponents via `RewriteError::InvalidExponent`.
- Lifted expressions are normalized with `mpl-ir` canonicalization.

Rules:
- Safe rules (default): `(+ x 0)`, `(* x 1)`, `(* x 0)`, `(^ x 0)`, `(^ x 1)`.
- Aggressive rules (`--aggressive`): limited factoring
  `(+ (* a b) (* a c)) -> (* a (+ b c))` and symmetric variant.
- No log/li2 functional identities or distribution rules.

Runner/Extractor:
- Config: `iters`, `node_limit`, `time_limit_ms` (`RewriteConfig`).
- Cost model: `egg::AstSize` (deterministic).
- Runner returns the best extracted expression even when limits are hit.

## CLI Usage (with examples)
All commands parse then normalize input before further processing.

### normalize
```bash
cargo run -p mpl-simplify -- normalize --expr "(+ x y 0 3 x)"
# => (+ 3 x x y)

cargo run -p mpl-simplify -- normalize --expr "(/ x y z)"
# => (* (^ y -1) (^ z -1) x)

cargo run -p mpl-simplify -- normalize --expr "(^ (^ x 2) 3)"
# => (^ x 6)
```

### symbol
```bash
cargo run -p mpl-simplify -- symbol --expr "(log x)"
# => 1 * (x)

cargo run -p mpl-simplify -- symbol --expr "(li2 x)"
# => -1 * ((+ 1 (* -1 x)) ⊗ x)

cargo run -p mpl-simplify -- symbol --expr "(* (log x) (log y))"
# => 1 * (x ⊗ y)
# => 1 * (y ⊗ x)
```

### check-integrable
```bash
cargo run -p mpl-simplify -- check-integrable --expr "(* (log x) (log y))"
# => true

cargo run -p mpl-simplify -- check-integrable --expr "(li2 x)"
# => true
```

### simplify
Defaults: `iters=20`, `node_limit=50000`, `time_limit_ms=300`, safe rules, symbol guard on.

Symbol guard behavior:
- If `symbol(before) != symbol(after)`, return the baseline.
- If `check_integrable` is false, return the baseline.
- If symbolization fails (NotImplemented or other error), the guard is skipped.

```bash
cargo run -p mpl-simplify -- simplify --expr "(+ (* x y) (* x z))"
# => (+ (* x y) (* x z))  (safe rules; same canonical form)

cargo run -p mpl-simplify -- simplify --aggressive --expr "(+ (* x y) (* x z))"
# => (* (+ y z) x)

cargo run -p mpl-simplify -- simplify --aggressive --expr "(li2 (+ (* x y) (* x z)))"
# => (li2 (+ (* x y) (* x z)))  (guard blocks factoring inside li2)

cargo run -p mpl-simplify -- simplify --aggressive --no-symbol-guard --expr "(li2 (+ (* x y) (* x z)))"
# => (li2 (* (+ y z) x))
```

### version
```bash
cargo run -p mpl-simplify -- version
# => 0.1.2
```

## Testing & Benchmarks
Tests:
- `tests/regression_normalize.rs`: canonical form red lines, idempotence, and
  determinism across repeated normalizations.
- `tests/cli.rs`: CLI coverage for normalize, symbol, check-integrable, simplify,
  and guard behavior.
- `crates/rewrite/src/lib.rs` tests: lower/lift roundtrip and aggressive factoring.
- No ignored tests are present.

Benchmarks:
- `benches/parse_normalize.rs`: parse + normalize throughput in `mpl-ir`.
- `benches/rewrite_simplify.rs`: rewrite simplification throughput (aggressive).

Determinism expectations:
- Avoid reliance on `HashMap` iteration order when printing or testing.
- `mpl-ir` sorts canonical strings for `Add`/`Mul` output.
- `mpl-symbol` uses `BTreeMap` and ordered `Word` comparisons.
- `mpl-rewrite` extraction uses `AstSize` with a fixed runner config.

Run:
```bash
cargo fmt --check
cargo test --workspace
# If CI enforces clippy:
cargo clippy --all-targets --all-features -- -D warnings
cargo bench
```

## Roadmap (next small versions)
- v0.1.3: smarter symbol guard with letter-equivalence canonicalization.
  Acceptance: guard allows algebraically equivalent letters while remaining
  deterministic; new tests cover guard relaxation without symbol drift.
- v0.1.4: weight-2 projection (integrable subspace residual).
  Acceptance: `project_to_integrable` returns basis coefficients + residual with
  deterministic ordering and tests for positive/negative cases.
- v0.1.5: extend symbol nodes (Li3 or minimal GPL carrier) without identities.
  Acceptance: new node parsed/printed, symbol rule added, integrability tests added.
- Rewrite explainability (egg explain or internal record).
  Acceptance: `simplify --explain` emits a reproducible derivation trace.
- Benchmark expansion.
  Acceptance: at least one new micro-bench per new rule group or symbol feature.

## Contributing
See `CONTRIBUTING.md` for contribution guidelines and required checks.
One hard constraint: do not change canonical invariants or add rewrite rules
without updating tests (especially `tests/regression_normalize.rs`).
