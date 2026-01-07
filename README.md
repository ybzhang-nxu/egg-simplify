# mpl-simplifier (v0.1.5 node release)

## Overview
mpl-simplifier is a deterministic Rust workspace for simplifying symbolic algebraic
expressions. It provides a canonical normal form (v0.1.1 baseline), a symbol
layer for log/li2 with shuffle algebra for products/powers, general weight-n
integrability and a space engine, an egg-based rewrite engine with a
conservative symbol guard, plus an optional symbol-aware extractor backed by
deterministic fingerprints and fuel-limited symbolization.

This project intentionally avoids branch-sensitive functional identities and
full MPL/GPL reconstruction. It does not implement log/li2 identities or
function reconstruction; those are deferred to future milestones.

## Architecture & Crates
Dependency DAG (no cycles, per AGENTS.md):
`mpl-ir` <- {`mpl-symbol`, `mpl-rewrite`, `mpl-verify`}; `mpl-rewrite-symbol` <- {`mpl-ir`, `mpl-rewrite`, `mpl-symbol`}; `mpl-simplify` -> {`mpl-ir`, `mpl-rewrite`, `mpl-symbol`, `mpl-rewrite-symbol`}.

| Crate | Purpose | Key Modules / Public API | Depends On |
| --- | --- | --- | --- |
| `mpl-ir` | AST, parser, normalization, canonical printing | `Expr`, `parse_sexpr`, `Expr::normalize`, `Expr::to_canonical_string`, `ParseError` (`crates/ir/src/lib.rs`) | (none) |
| `mpl-symbol` | Symbol tensor, symbolization rules, integrability checks + space engine | `Symbol`, `Word`, `Coeff`, `ShuffleFuel`, `symbol`, `symbol_with_fuel`, `check_integrable`, `space::{check_integrable_n, Alphabet, WordConstraints, Basis, BasisStats, build_integrable_basis, reduce_to_basis}`, `Coproduct`, `SymbolError` (`crates/symbol/src/*.rs`) | `mpl-ir` |
| `mpl-rewrite` | egg language/rules/lowering/lifting + simplifier | `simplify_algebra`, `RewriteConfig`, `RewriteMode`, `RewriteError`, `lower_expr`, `lift_expr` (`crates/rewrite/src/*.rs`) | `mpl-ir`, `egg` |
| `mpl-rewrite-symbol` | Symbol-aware rewrite pipeline + fingerprinting + deterministic extractor | `simplify_symbol_aware`, `SymbolContext`, `FingerprintConfig`, `PenaltyConfig` (`crates/rewrite-symbol/src/lib.rs`) | `mpl-ir`, `mpl-rewrite`, `mpl-symbol` |
| `mpl-verify` | Exact rational eval + sample equivalence | `eval_rational`, `equiv_on_samples`, `EvalError` (`crates/verify/src/lib.rs`) | `mpl-ir` |
| `mpl-simplify` | CLI entry point | Subcommands in `crates/cli/src/main.rs` | `mpl-ir`, `mpl-rewrite`, `mpl-symbol`, `mpl-rewrite-symbol` |

See `docs/ARCHITECTURE.md` for a short architecture overview.

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
- Eliminate division: `(/ a b c)` -> `(* (^ b -1) (^ c -1) a)`.
- Power rules: `x^0 -> 1` (including `0^0 -> 1`), `x^1 -> x`.
- Merge same-base powers in products: `x^a * x^b -> x^(a+b)`.
- Safe power folding for rationals; `(^ 0 -1)` stays as-is (no panic).
- Deterministic ordering: constants first, then canonical-string ordering.
- `log`/`li2` only normalize their argument; no functional identities.

See `docs/canonical_form.md` for the formal spec.

## Symbol Layer (current milestone)
Supported symbol rules (no branch-sensitive identities):
- `S(log l) = [l]` for algebraic letter `l`.
- `S(li2 f) = -(1-f) otimes f` (letter `f` must be algebraic).
- Products use the shuffle algebra: `S(f * g) = shuffle(S(f), S(g))`.
- Powers use shuffle powers: `S(f^n) = shuffle_pow(S(f), n)` for `n > 0`,
  and `S(1) = 0` for `n = 0`.
- Non-rational prefactors on non-algebraic factors return `SymbolError::NotImplemented`.
Here `otimes` denotes word concatenation (tensor product) in the symbol.

Integrability:
- `check_integrable` supports weight-2 symbols (legacy entry point).
- For weights greater than 2, `check_integrable` returns `SymbolError::NotImplemented`.
- `mpl_symbol::space::check_integrable_n` supports general weight-n symbols.
- The CLI guard uses `check_integrable` for weight <= 2 and `check_integrable_n`
  for weight > 2; errors skip the guard.
- Deterministic sampling uses a fixed rational table and ordered variable
  environments; singular points are skipped (insufficient samples return
  `SymbolError::InsufficientSamples`).

Output format:
- CLI prints one line per word: `coeff * (l1 <sep> l2 <sep> ...)` where `<sep>`
  is the literal separator used by the CLI.
- Coefficients are exact rationals; terms are merged and sorted deterministically
  (`Symbol` uses `BTreeMap`, `Word` orders by canonical strings).

## Space Engine (general weight=n)
The general weight-n space engine lives under `mpl_symbol::space`.

Core API:
- `Alphabet`: normalized letters + names with a deterministic canonical-string map.
- `WordConstraints`: first-letter and adjacency constraints.
- `Basis`: word columns + nullspace vectors with a free-variable convention.
- `BasisStats`: standardized diagnostics for basis construction.
- `build_integrable_basis(alpha, constraints, weight) -> Result<Basis, SymbolError>`
- `reduce_to_basis(sym, basis, alpha) -> Result<(Vec<Coeff>, Symbol), SymbolError>`
- `check_integrable_n(sym) -> Result<bool, SymbolError>`
- `Basis::stats()` exposes `BasisStats` for deterministic diagnostics.

The CLI does not expose `check_integrable_n` or basis building; use the library
API or `crates/symbol/src/space/tests_weight_n.rs` as a reference.

See `docs/space_engine.md` for algorithm details.

## Algorithms (integrability + basis)
Integrability constraints (general weight-n):
- For each weight `w >= 2` and adjacent position `k`, group terms by CONTEXT
  (word with positions `k` and `k+1` removed).
- For each context and variable pair `(vi, vj)`, require:
  sum(coeff * wedge(dlog(l_k), dlog(l_{k+1}))) == 0.
- Deterministic sampling uses a fixed env table; singular samples are skipped.
  If fewer than two valid samples exist for a constraint, return
  `SymbolError::InsufficientSamples`.

Basis construction:
- Enumerate all allowed words lexicographically by letter id.
- Build sparse constraint rows from the integrability checks.
- Streaming elimination uses REF/dictionary form:
  - pivot is the smallest column index in a row
  - no global RREF cleanup is performed
- Compute the nullspace via back-substitution in descending pivot order to
  preserve the free-variable basis convention used by `reduce_to_basis`.

Determinism sources:
- Word enumeration is lexicographic by id sequence.
- Pivot selection always uses the smallest column index in a row.
- Free-variable basis vectors have identity entries at free columns.

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

Symbol-aware rewrite (optional):
- `mpl-rewrite-symbol` provides a symbol-aware extractor and fingerprint cache.
- Fingerprints degrade to `Unknown` instead of failing, preserving determinism.
- Extractor tie-breaks use a stable structural hash (no randomized hasher).

## CLI
Subcommands:
- `normalize --expr ...`
- `symbol --expr ...`
- `check-integrable --expr ...` (weight-2 only)
- `simplify --expr ... [--iters N] [--node-limit N] [--time-limit-ms N] [--aggressive] [--no-rewrite] [--no-symbol-guard] [--symbol-aware] [--symbol-fuel N] [--symbol-weight-limit N] [--unknown-penalty N] [--non-integrable-penalty N] [--conflict-penalty N]`
- `version`

Notes:
- The symbol guard uses `check_integrable` for weight <= 2 and
  `check_integrable_n` for weight > 2. If symbolization or integrability
  returns an error, the guard is skipped.
- `--symbol-aware` enables the symbol-aware extractor; `--symbol-fuel` defaults
  to 100 and the symbol-aware flags require `--symbol-aware`.
- General weight-n integrability and basis building are library-only APIs.

Examples:
```bash
cargo run -p mpl-simplify -- normalize --expr "(+ x y 0 3 x)"
# => (+ 3 x x y)

cargo run -p mpl-simplify -- normalize --expr "(/ x y z)"
# => (* (^ y -1) (^ z -1) x)

cargo run -p mpl-simplify -- symbol --expr "(log x)"
# => 1 * (x)

cargo run -p mpl-simplify -- check-integrable --expr "(* (log x) (log y))"
# => true

cargo run -p mpl-simplify -- simplify --aggressive --expr "(+ (* x y) (* x z))"
# => (* (+ y z) x)

cargo run -p mpl-simplify -- simplify --aggressive --symbol-aware --symbol-fuel 100 --expr "(+ (* x y) (* x z))"
# => (* (+ y z) x)

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
- `crates/symbol/src/space/tests_weight_n.rs`: general weight-n tests with ignored
  stress cases (run with `--ignored`).
- `crates/symbol/tests/stress.rs`: ignored stress tests for shuffle/fuel and
  higher-weight integrability.

Toy oracle:
- For alphabet `{x, y}` with no constraints, the integrable subspace dimension at
  weight `w` is `w + 1`.

BasisStats one-line format (stable):
- `ncols`, `dim`, `rank`, `rows_attempted`, `rows_inserted`, `samples_used`,
  `envs_total`, `rows_skipped_singular`, `constraints_insufficient_samples`,
  `vars`, `max_row_nnz`, `avg_row_nnz`.
  `samples_used` counts valid sampled constraint-rows (not env count). `rank`
  equals the number of inserted pivot rows; `dim` is the nullspace size.

For stress tests, use `--test-threads=1` to avoid interleaved stats output.

Run:
```bash
cargo fmt --check
cargo test --workspace
cargo test -p mpl-symbol
cargo test -p mpl-symbol --release -- --ignored --nocapture --test-threads=1
# If CI enforces clippy:
cargo clippy --all-targets --all-features -- -D warnings
cargo bench
```

Benchmarks:
- `benches/parse_normalize.rs`: parse + normalize throughput in `mpl-ir`.
- `benches/rewrite_simplify.rs`: rewrite simplification throughput (aggressive).

Manual tools:
- `cargo run -p mpl-rewrite-symbol --bin symbol_param_scan` writes
  `reports/phase2_symbol_scan.md` and `reports/phase2_symbol_scan.csv`.

## v0.1.5 Release Notes
- Added shuffle algebra for symbol products/powers, enabling higher-weight
  symbols for products like log-powers and li2/log mixes.
- Added fuel-limited symbolization (`ShuffleFuel`, `symbol_with_fuel`) and
  deterministic early aborts on fuel exhaustion.
- Added a deconcatenation coproduct carrier (`Coproduct`) for Hopf workflows.
- CLI guard now uses `check_integrable_n` for weight > 2 (weight-2 remains
  `check_integrable`), preserving deterministic downgrade on errors.
- Added ignored stress tests for shuffle fuel boundaries and weight-10
  integrability, plus a weight-10 toy-oracle basis check.
- Milestone note: crate versions remain `0.1.1`/`0.1.2`; this is a node release
  label for documentation and planning purposes.

## Contributing
See `CONTRIBUTING.md` for contribution guidelines and required checks.
One hard constraint: do not change canonical invariants or add rewrite rules
without updating tests (especially `tests/regression_normalize.rs`).

