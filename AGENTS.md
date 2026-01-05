# AGENTS.md - Development Constraints & Roadmap Guardrails

This repository is a Rust workspace for a deterministic, modular simplifier
targeting MPL/GPL-style expressions and amplitude-style symbol workflows.

This file defines:
- **non-negotiable invariants** (canonical form, determinism, dependency DAG),
- **where code belongs** (crate/module responsibilities),
- **how to extend** the system across the next small versions:
  1) `rewrite`: egg Language/rules/extractor
  2) `symbol`: Li3/GPL + integrability + projection
  3) `cli`: `simplify` command as the pipeline entry point

This is written for both humans and automated agents.

---

## 0) Quick Commands

Run before pushing:

```bash
cargo fmt --check
cargo test --workspace
# If CI enforces clippy:
cargo clippy --all-targets --all-features -- -D warnings
# Optional:
cargo bench
```

CLI note (Windows / clap): if an expr starts with `-` (e.g. `-7/3`), pass as:

* `--expr -- -7/3`, or
* `--expr "(- 7/3)"`

---

## 1) Workspace Layout and Responsibilities

### `crates/ir` (mpl-ir)

**Source of truth** for:

* `Expr` AST
* s-expression parsing
* canonical normalization
* canonical printing

Hard constraints:

* OK: deterministic output (no unstable ordering)
* **No dependence on higher-level crates**
* Canonical output must not leak unordered map iteration order

### `crates/verify` (mpl-verify)

* Exact rational evaluation helpers (letters/expressions)
* Sample-based equivalence helpers (numeric sanity checks)
* May depend on `ir`, must not depend on `rewrite` or `cli`

### `crates/symbol` (mpl-symbol)

Symbol-layer tooling:

* Tensor `Symbol` data structure (sparse words, coefficients)
* `symbol(expr)` for supported function nodes (currently log/Li2; future: Li3/GPL)
* `check_integrable(symbol)` (weight-2 legacy entry point)
* `check_integrable_n(symbol)` for general weight-n integrability
* `Alphabet`, `WordConstraints`, `Basis`, `BasisStats`, `build_integrable_basis`, `reduce_to_basis`
* `project_to_integrable` / basis reduction (future)

Constraints:

* Depends on `ir` (and optionally small third-party crates)
* Must NOT depend on `rewrite` or `cli`
* OK: deterministic output (no unstable ordering)

### `crates/rewrite` (mpl-rewrite)

e-graph rewrite engine:

* egg `Language` + lowering/lifting
* rewrite rules grouped into phases
* runner configuration + extractor cost model
* explainability hooks (egg explain or internal derivation record) when available

Constraints:

* Depends on `ir`
* May optionally depend on `symbol` ONLY via clearly-separated helper functions
  (prefer: rewrite does not depend on symbol; pipeline/CLI orchestrates guard logic)
* Must not depend on `cli`

### `crates/cli` (mpl-simplify)

Command-line interface only:

* parse args
* call library crates
* print results deterministically
  No heavy logic in `main.rs`.

---

## 2) Dependency DAG (NO CYCLES)

Intended direction:

```
mpl-ir  <- mpl-verify
mpl-ir  <- mpl-symbol
mpl-ir  <- mpl-rewrite
mpl-cli -> {mpl-ir, mpl-verify, mpl-symbol, mpl-rewrite}
```

Hard rule:

* **No circular dependencies** in Cargo.
* `mpl-ir` remains the bottom layer.
* Avoid package name `"core"` in any Cargo.toml.

---

## 3) Canonical Form Red Lines (v0.1.1 baseline)

Normative spec: `docs/canonical_form.md`.

Do not break without updating docs + regression tests:

* N-ary flattening of `Add`/`Mul`
* Identity removal (`+0`, `*1`) and empty-node conventions
* Strict annihilator: any `Mul` containing `0` normalizes to `0`
* Division eliminated: canonical output contains no `/`
* Power rules: `x^0 -> 1` (including `0^0 -> 1` convention), safe folding, no panics on `(^ 0 -1)`
* Merging same-base powers in products
* Deterministic ordering: constants first, then canonical-string ordering

Any intentional change requires:

1. update `docs/canonical_form.md`
2. update `tests/regression_normalize.rs`
3. confirm idempotence + determinism

---

## 4) Determinism & Output Stability Rules

* Never rely on `HashMap` iteration order for printing or tests.
* When printing Symbol terms / sets / maps: sort keys (prefer `BTreeMap`).
* E-graph extraction must be deterministic for the same config.
* If sampling is used (integrability checks), sampling must be deterministic:
  fixed sample table + fixed env generation order.
* Word enumeration for the space engine is lexicographic by letter id.
* Pivot selection uses the smallest column index in the row.
* Basis vectors use the free-variable convention (identity on free columns).
* `BasisStats` output must be deterministic for identical inputs.

## 4.5) Space Engine Non-Negotiables (v0.1.4)

* Determinism: use ordered maps/sets or explicit sorting (no `HashMap` iteration).
* Letters are canonical `Expr` values; no branch-sensitive MPL identities.
* Constraints are plug-in only (first-entry/adjacency); the core engine stays alphabet-agnostic.
* Linear algebra MUST use streaming REF/dictionary elimination plus back-substitution.
  Do NOT reintroduce global RREF cleanup (quadratic behavior).
* Diagnostics must use `BasisStats`; avoid ad-hoc prints in library paths.
* Stress tests must remain `#[ignore]` and be documented in README.
* Keep the `{x, y}` toy oracle regression (dim at weight `w` is `w + 1`).

---

## 5) Next Versions: Guardrails and Definition of Done

### 5.1 Rewrite Implementation (egg): Language / Rules / Extractor

**Goal:** Algebraic rewriting and structural minimization without semantic drift.

#### Language design rules

* Keep the egg Language minimal and stable:

  * `Num(Q)`, `Var`, `Add`, `Mul`, `Pow`
  * Allow `Log`/`Li2`/`Li3`/`G` nodes as *opaque wrappers* whose arguments may be rewritten
    (but do not add functional identities as egg rules early).
* Lowering from n-ary `Expr` to egg binary shape must be deterministic
  (fixed fold direction).
* Lifting must:

  * produce a valid `Expr`
  * call `ir.normalize()` to re-canonicalize
  * reject invalid pow exponents (non-integer) with a readable error (no panic)

#### Rewrite rules policy

Rules are grouped by risk:

* **Normalization/safe local rules**: always enabled (should match canonical invariants)
* **Algebraic restructuring rules** (factoring/defactoring): gated and benchmarked
* **Explosive rules** (distribution, full expansion): never default-on

Default rule set (early versions):

* Safe: constant folding, `x+0`, `x*1`, `x*0`, trivial pow rules
* One or two restructuring rules that clearly reduce AST size in common cases (e.g. factoring common multiplier)
* Avoid aggressive distribution until node/time-limits and benchmarks are strong

#### Extractor / cost model

* Start with a deterministic size-based cost:

  * penalize deep nesting
  * prefer n-ary compact forms (post-lift normalize will flatten anyway)
* Include configuration:

  * `iters`, `node_limit`, `time_limit_ms`
* Must return "best so far" if limits hit (no crash).

**Definition of done for a rewrite milestone:**

* OK: `cargo test --workspace` green
* New rewrite rules have:

  * unit tests demonstrating expected simplification
  * regression tests if canonical outputs change (usually should not)
* Add at least one `criterion` benchmark or micro-benchmark for a new rule group
* No flakiness in output

---

### 5.2 Symbol Expansion: Li3 / GPL + Integrability + Projection

**Goal:** Treat symbols as first-class objects for constraints and basis/projection workflows,
without branch-sensitive functional identities.

#### Symbolization policy

* Symbol keys ("letters") are **algebraic expressions** (canonical `Expr`) only.
* Do not introduce log identities like `log(ab)=log a+log b` initially.
* Introduce new function families by adding:

  1. parse/print node in `ir`
  2. `symbol(expr)` mapping rule in `symbol/rules.rs`
  3. tests validating symbol output and integrability checks

#### Extending from Li2 to Li3

* Add `Li3(expr)` node as a carrier.
* Implement `S(Li3(f))` in a way consistent with your chosen convention.
  (If unsure, keep as NotImplemented until spec is written.)
* Require:

  * tests for the expected word structure
  * integrability tests (where applicable)

#### GPL / iterated integrals

* Add a node for a minimal GPL representation only after the symbol data model is stable.
* Preferred approach: represent GPL parameters as a vector structure in `ir`
  (avoid encoding long parameter lists in binary nodes in egg).
* Add symbolization rules incrementally (low weight first).

#### Integrability

* Current: general weight-n integrability via `check_integrable_n` (adjacent wedge + context grouping).
* `check_integrable` remains the weight-2 legacy entry point.
* Provide a deterministic sampling strategy; skip singular points; fail with readable error if no valid samples.
* Basis builder MUST keep streaming REF/dictionary elimination + back-substitution.
  Do NOT reintroduce global RREF cleanup (quadratic behavior).
* Maintain `BasisStats` for diagnostics rather than ad-hoc prints.

#### Projection / basis reduction

* Keep linear algebra isolated in `crates/symbol` modules (e.g. `projection.rs`).
* MVP: project a candidate symbol onto the integrable subspace (weight=2) and report residual.
* Do not attempt full "function reconstruction" early. It is OK to output:

  * "basis coefficients" + "basis atoms" placeholders.

**Definition of done for a symbol milestone:**

* New node + rules + tests are added
* `symbol(expr)` output is deterministic and stable
* `check_integrable` and `check_integrable_n` are correct on curated examples
* Any new math convention is documented (short spec in `docs/`)

---

### 5.3 CLI: Add `simplify` Command (Pipeline Entry Point)

**Goal:** Provide a single stable entry point that composes:
canonical normalization -> (optional) e-graph rewrite -> (optional) symbol guard -> final normalize.

#### CLI design rules

* CLI must remain thin; orchestration logic should live in library crates (or a small pipeline module/crate).
* Subcommands:

  * `normalize` (existing)
  * `symbol` / `check-integrable` (existing)
  * `simplify` (new)

#### `simplify` recommended behavior (default settings)

1. Parse -> `ir.normalize()`
2. Run `rewrite` (egg) with safe rules + limits
3. Optional symbol guard:

   * compute symbol before/after (if implemented)
   * canonicalize symbol letters using algebraic normalization (do not compare raw non-canonical letters)
   * require integrability where applicable
4. Return best expression (deterministic)

Flags:

* `--iters`, `--node-limit`, `--time-limit-ms`
* `--no-rewrite` (normalize-only)
* `--no-symbol-guard` (for debugging)
* `--explain` (future)

**Definition of done for the CLI milestone:**

* Integration tests in `tests/cli.rs` cover:

  * `simplify` works on at least 2 representative cases
  * behavior deterministic
* No breaking change to existing subcommands

---

## 6) Testing & Benchmark Policy

* Regression suite is mandatory for canonical behavior: `tests/regression_normalize.rs`
* Every new rewrite rule must have:

  * a unit test (expected transformation)
  * a safety test (no panic; respects limits)
* Every new symbol rule must have:

  * exact symbol output tests (or canonicalized form tests)
  * integrability tests
* Keep `{x, y}` toy oracle coverage for basis dimension at weight `w` (`w + 1`).
* Heavy stress tests must be `#[ignore]` and documented in README.

Benchmarks:

* Add micro-benchmarks for:

  * rewrite runner behavior on representative expressions
  * symbolization / integrability performance on increasing sizes
* Keep benchmark inputs deterministic and checked into repo.

---

## 7) Error Handling Policy

* No panics on user inputs in library crates.
* Prefer `Result<T, Error>` with typed errors:

  * `NotImplemented`
  * `InvalidArity`
  * `InvalidExponent`
  * `DivisionByZero` (for eval)
* CLI prints readable messages and exits non-zero on errors.

---

## 8) Change Control Checklist (before merging)

* OK: `cargo test --workspace` green
* OK: deterministic output (no unstable ordering)
* OK: dependency DAG unchanged (no cycles)
* OK: canonical red lines respected (or spec+tests updated)
* OK: new rule additions have tests + (if risky) micro-bench



