# mpl-simplifier (v0.2.3 release)

## Overview
mpl-simplifier is a deterministic Rust workspace for simplifying symbolic algebraic
expressions. It provides a canonical normal form (v0.1.1 baseline), a symbol
layer for log/li2 with shuffle algebra for products/powers, general weight-n
integrability and a space engine, an egg-based rewrite engine with a
conservative symbol guard, plus an optional symbol-aware extractor backed by
deterministic fingerprints and fuel-limited symbolization, and a file-driven M1
experiments runner.

This project intentionally avoids branch-sensitive functional identities and
full MPL/GPL reconstruction. It does not implement log/li2 identities or
function reconstruction; those are deferred to future milestones.

## Architecture & Crates
Dependency DAG (no cycles, per AGENTS.md):
`mpl-ir` <- {`mpl-symbol`, `mpl-rewrite`, `mpl-verify`}; `mpl-rewrite-symbol` <- {`mpl-ir`, `mpl-rewrite`, `mpl-symbol`}; `mpl-experiments` -> {`mpl-ir`, `mpl-symbol`}; `mpl-simplify` -> {`mpl-ir`, `mpl-rewrite`, `mpl-symbol`, `mpl-rewrite-symbol`}.

| Crate | Purpose | Key Modules / Public API | Depends On |
| --- | --- | --- | --- |
| `mpl-ir` | AST, parser, normalization, canonical printing | `Expr`, `parse_sexpr`, `Expr::normalize`, `Expr::to_canonical_string`, `ParseError` (`crates/ir/src/lib.rs`) | (none) |
| `mpl-symbol` | Symbol tensor, symbolization rules, integrability checks + space engine | `Symbol`, `Word`, `Coeff`, `ShuffleFuel`, `symbol`, `symbol_with_fuel`, `check_integrable`, `space::{check_integrable_n, Alphabet, WordConstraints, Basis, BasisStats, build_integrable_basis, reduce_to_basis}`, `Coproduct`, `SymbolError` (`crates/symbol/src/*.rs`) | `mpl-ir` |
| `mpl-rewrite` | egg language/rules/lowering/lifting + simplifier | `simplify_algebra`, `RewriteConfig`, `RewriteMode`, `RewriteError`, `lower_expr`, `lift_expr` (`crates/rewrite/src/*.rs`) | `mpl-ir`, `egg` |
| `mpl-rewrite-symbol` | Symbol-aware rewrite pipeline + fingerprinting + deterministic extractor | `simplify_symbol_aware`, `SymbolContext`, `FingerprintConfig`, `PenaltyConfig` (`crates/rewrite-symbol/src/lib.rs`) | `mpl-ir`, `mpl-rewrite`, `mpl-symbol` |
| `mpl-verify` | Exact rational eval + sample equivalence | `eval_rational`, `equiv_on_samples`, `EvalError` (`crates/verify/src/lib.rs`) | `mpl-ir` |
| `mpl-experiments` | M1 experiments runner (spec parsing + deterministic outputs) | `load_spec`, `parse_spec_str`, `run_experiment`, `write_outputs` (`crates/experiments/src/*.rs`) | `mpl-ir`, `mpl-symbol` |
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
- `WordAcceptor`: DFA-style word filtering (adapters preserve M1 constraints).
- `MaxAlternationsAcceptor`: limit the number of letter alternations.
- `KGramAcceptor` (k=3): allowed/forbidden triplet constraints.
- `GenealogicalAcceptor`: channel/letter-level "after seeing X, forbid Y later".
- `Basis`: word columns + nullspace vectors with a free-variable convention.
- `BasisStats`: standardized diagnostics for basis construction.
- `build_integrable_basis(alpha, constraints, weight) -> Result<Basis, SymbolError>`
- `build_integrable_basis_with_acceptor(alpha, acceptor, weight, budget)`
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
# => 0.2.3
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
- `crates/experiments/tests/m1_golden.rs`: M1 golden outputs (L1 A2 cluster).

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
- `cargo run -p mpl-experiments -- run --spec crates/experiments/m1/L1_A2_cluster.toml`
  writes M1 outputs (`basis_stats.txt`, `dim_vs_w.csv`, `pairs.csv`,
  `pairs_by_weight.csv`, `triplets.csv`, `triplets_by_weight.csv`,
  `forbidden_pairs.csv`, `genealogical_rules.json`, `topology_metrics.csv`)
  under the spec's `out_dir`.
- `mpl-experiments` sets `default-run = "mpl_experiments"`, so `cargo run -p mpl-experiments -- ...`
  works without specifying `--bin`.
- `cargo run -p mpl-experiments -- count --spec experiments/m2/M2_gene_channel_no_interleave_count_w12.toml`
  writes `counts_only.csv` to the spec `out_dir`.
- `cargo run -p mpl-experiments -- filtration --spec experiments/m6/M6_reg_filtration_chain_w3.toml`
  writes `filtration_summary.csv` and `filtration_summary.md` to the spec `out_dir`.
- `cargo run -p mpl-experiments -- esymb-rank-scan --data-dir reports/converted_jsonl --loops 1..6 --family pow-last --x-set a,b --y-set f,g --normalize auto`
  writes `rank_scan.csv` and `summary.md` under `reports/esymb_rank_scan`.
- `cargo run -p mpl-experiments -- esymb-rank-scan --data-dir reports/converted_jsonl --loops 1..6 --family block2 --pairs auto --alphabet auto --normalize auto --attempt-solve-inconclusive`
  runs block2 scans with auto pair discovery and candidate solves.
- `cargo run -p mpl-experiments -- esymb-rank-scan --data-dir reports/converted_jsonl --loops 1..6 --family prefix --prefix-len 1 --letters a,b --validate-marginals --export-observables`
  runs prefix marginals with conservation checks and writes `marginals_observables.csv`.
- `cargo run -p mpl-experiments -- esymb-span-deps --in reports/esymb_rank_scan/marginals_observables.csv --out-dir reports/esymb_span_deps`
  extracts sparse span relations from exported marginals and writes `span_stats.csv`, `equiv_classes.csv`, `span_deps.csv`, `basis_keys.csv`,
  `basis_expansions_modp.csv`, `support_mask.csv`, `mask_histogram.csv`, `allowed_graph.csv`, and `span_deps.md`.
- `cargo run -p mpl-experiments -- esymb-hankel-subblock --in reports/esymb_rank_scan/marginals_observables.csv --r 2 --k 2 --exact --out-dir reports/esymb_hankel_subblock`
  rebuilds prefix-suffix Hankel subblocks per loop, writes `hankel_subblock_stats.csv`, and (with `--exact`) mod-p row/col dependencies.
- `cargo run -p mpl-experiments -- esymb-rank-scan --data-dir reports/converted_jsonl --loops 1..6 --family prefix-suffix --prefix-len 2 --suffix-len 2 --letters a,b,c --matrix-rank`
  runs prefix/suffix marginals and writes `marginals_matrix_rank.csv`.
- Experiments spec: k-gram `mode = "allowed"` requires non-empty `triplets`
  (parse error includes `InvalidSpecEmptyAllowList`).
- Experiments spec: budgets are opt-in via `[constraints.budget]` with
  `max_states`, `max_transitions`, `max_words`.
- Experiments spec: acceptors are listed under `[constraints.automaton.acceptors]`
  with `kind = "kgram"`, `kind = "genealogical"`, or `kind = "channel_pairs"`.
- Experiments spec: genealogical constraints default to channel-level tracking;
  `[[alphabet.letters]]` may declare `channel`, required when `seen = "channel"`.
- See `docs/experiments_format_m2.md` for the M2 single-run schema and outputs,
  and `docs/experiments_format_m6.md` for the filtration schema and outputs.
- See `docs/performance_bottlenecks.md` for known scale limits.

## Path1 Baseline
Deterministic toy baseline for the space engine and ESymb A/B/C pipeline.

Oracle mode (toy oracle check, dim == w+1):
```bash
cargo run -p mpl-experiments -- path1-toy --mode oracle --weights 1..12 --out-dir reports/path1_toy
```

Scaled mode (synthetic ESymb JSONL + pipeline):
```bash
cargo run -p mpl-experiments -- path1-toy --mode scaled --loops 2..24 --max-alternations 3 --run-esymb --out-dir reports/path1_toy
```

Notes:
- `--run-esymb` requires loops >= 2 because the pipeline uses prefix-suffix r=2,k=2.
- Outputs land under `reports/path1_toy/oracle` and `reports/path1_toy/scaled`.
- Scaled mode supports high-loop JSONL generation (e.g., loops 1..24) and can be
  fed into `esymb-rank-scan` + `esymb-hankel-subblock --exact` for dependency analysis.

## v0.2.3 Release Notes
- Added `path1-toy` for deterministic Path1 baselines (toy oracle + scaled synthetic ESymb JSONL).
- Added `MaxAlternationsAcceptor` to cap alternations for high-loop enumeration.
- Documented high-loop data generation and Hankel dependency analysis flow.

## v0.2.2 Release Notes
- Added ESymb marginals analysis tools in `mpl-experiments`:
  `esymb-span-deps` (forbidden/nonzero keys, equivalence classes, sparse relations,
  mod-p basis expansions, and zero-pattern masks) and `esymb-hankel-subblock`
  (prefix-suffix Hankel subblocks with mod-p rank and optional exact row/col
  dependencies).
- Added `--export-observables` and `--matrix-rank` outputs for ESymb marginals
  scans to support downstream span/Hankel analysis.

## v0.2.1 Release Notes
- Expanded `esymb-rank-scan` with normalization candidates, screen status
  grouping, and mapped recurrence reporting (including predict_next for
  normalized/original sequences).
- Added auto alphabet + auto block2 pair discovery for ESymb scans, plus
  block2-specific normalization candidates.
- Added conservative mod-p rank aggregation (max over primes) and candidate
  recurrence scaffolding for inconclusive sequences.

## v0.2.0 Release Notes
- Added cross-loop analysis in `mpl-experiments` (suffix projection, image rank,
  mapping into lower space, and scan mode).
- Added cross-loop CLI (`mpl-experiments cross-loop`) with deterministic outputs
  and multi-suffix scan support.
- Added cross-loop regression tests and design documentation.
- Added `esymb-rank-scan` in `mpl-experiments` for Hankel rank screening and
  exact recurrence recovery on Esymb JSONL inputs.

## v0.1.9 Release Notes
- Added compiled acceptor graph + incremental DP cache for M6/M2 counting
  (reuses counts across weights and avoids per-weight graph rebuilds).
- Added deterministic `sample_table` selection to specs and output summaries
  (`basis_stats.txt`, `dim_vs_w.csv`, `filtration_summary.csv`).
- Added deterministic per-layer/weight parallelism in M6 filtration via
  `engine.jobs` / `--jobs`.
- Added basis-invariant genealogical outputs: `forbidden_pairs.csv` and
  `genealogical_rules.json` from support-word subsequence checks.

## v0.1.8 Release Notes
- Added M3/M4/M5/M6 experiment suites (`experiments/m3`, `experiments/m4`,
  `experiments/m5`, `experiments/m6`) and new regression coverage.
- New public API (mpl-experiments): `Skeleton2Metrics`, `render_skeleton2_metrics`,
  `load_filtration_spec`, `parse_filtration_spec_str`, `run_filtration`,
  `FiltrationSpec`, `FiltrationLayer`, `FiltrationLayerInfo`, `FiltrationMode`,
  `FiltrationReport`, `FiltrationSummaryRow`, `render_filtration_summary_csv`,
  `render_filtration_summary_md`, `write_filtration_summary`.
- Added M6 filtration spec + outputs (`filtration_summary.csv`/`.md`), documented
  in `docs/experiments_format_m6.md`.
- Canonicalized channel handling for acceptors to avoid `"01"` vs `1` drift;
  `channel_pairs` remains numeric-only with deterministic errors.
- Refactored `mpl-experiments` internals into focused modules; output contracts
  unchanged and existing APIs remain source-compatible.
- Hardened repeat-signature checks with deterministic CSV escaping and
  explicit errors for invalid baseline state.

## v0.1.7 Release Notes
- Added M2 experiment spec documentation (`docs/experiments_format_m2.md`) and
  an M2 spec suite under `experiments/m2`.
- Added triplet outputs (`triplets.csv`, `triplets_by_weight.csv`) to experiment
  outputs (additive; M1 contracts unchanged).
- Added acceptor-based k-gram and genealogical constraints and a count-only
  runner (`mpl-experiments count --spec ...`) for large weights.
- Documented known performance bottlenecks at ~50k columns.

## v0.1.6 Release Notes
- Added `mpl-experiments` M1 runner with TOML specs and deterministic outputs
  (`basis_stats.txt`, `dim_vs_w.csv`, `pairs.csv`, `pairs_by_weight.csv`,
  `triplets.csv`, `triplets_by_weight.csv`, `topology_metrics.csv`) plus stable
  `status/error_code` columns.
- Added L1 A2 golden regressions (dim/rank/pairs/topology) and determinism checks
  for the M1 output contract.
- Hardened M1 spec validation (duplicate letter names, unknown references, and
  empty allow-lists are rejected deterministically).
- Milestone note: crate versions are aligned to `0.2.3`.

## Contributing
See `CONTRIBUTING.md` for contribution guidelines and required checks.
One hard constraint: do not change canonical invariants or add rewrite rules
without updating tests (especially `tests/regression_normalize.rs`).

